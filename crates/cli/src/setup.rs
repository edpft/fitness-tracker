//! Getting a fresh machine ready to use.
//!
//! **A wizard is not a shortcut for something the tool can work out.** Every
//! value it asks for is one nothing can derive: which zone the operator trains
//! in, and the credentials for the sources they want read. It asks for nothing
//! else, and it *reports* everything else — where things went, and what still
//! needs doing — because the failure mode of a setup command is leaving someone
//! unsure whether it worked.
//!
//! **It does ask for the credential**, and the reasoning that once said
//! otherwise conflated three different things. Passing a secret as a *flag* is
//! genuinely bad — it lands in argv, in shell history and in `ps` output, which
//! is why there is still no `--api-key`. Storing one in a *file* is what § 35
//! explicitly allows. *Prompting* for one touches neither: a typed key never
//! reaches argv, and with echo off it does not reach the scrollback either.
//!
//! **What is taken is checked before it is written** (#61). A credential is the
//! one input here whose correctness the operator cannot see: it is typed with
//! echo off, and a paste that arrives short looks exactly like one that did not.
//! So each is offered to its source, and a rejected one is refused at the prompt
//! rather than stored to fail a fortnight later.
//!
//! **Interactive only when there is somebody there.** A missing value is
//! prompted for at a terminal and refused without one, so the same command works
//! under a scheduler — which is what stops `init` becoming the step that cannot
//! be automated.

use std::{
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use application::GenerationParameterStore as _;
use infrastructure::{
    Credential as StoredCredential, Credentials, SqliteGenerationParameterStore,
    SqliteOperatorSettingsStore, TokenFile, WithingsAuth, WithingsClient, connect,
};

use crate::{Failure, catalogue::Credential, config, exit, paths};

/// What `init` found or made.
pub struct Prepared {
    pub database: PathBuf,
    pub credentials_path: PathBuf,
    pub zone: String,
    /// What became of the generation parameters.
    pub parameters: ParameterOutcome,
    /// What became of each source's credential.
    pub credentials: Vec<(String, CredentialOutcome)>,
}

/// What `init` did about the parameters a prescription is generated against.
///
/// **Seeding them is a step of setting up, not of authoring a programme**
/// (decision 0015). Nothing on the generation path reaches a compiled-in
/// number, so a store with no parameters can hold a programme and prescribe
/// nothing from it — which is exactly the dead end this step exists to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterOutcome {
    /// None were stored, so this build's shipped set was written, dated.
    Seeded,
    /// A set was already in force and is left alone. Re-seeding would supersede
    /// values the operator may have changed deliberately, and § 12 keeps the
    /// old rows either way — so the quiet thing to do is nothing.
    AlreadyInForce,
}

/// What `init` was able to do about a source's credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialOutcome {
    /// Taken, accepted by the source, and written to the credentials file.
    Stored,
    /// Already in the environment, and left there. Nothing is copied into a
    /// file that the environment is already answering for.
    InEnvironment,
    /// Stored by an earlier run, and not asked for again.
    AlreadyStored,
    /// Nobody to ask and nothing to take. Reported as outstanding.
    Outstanding,
    /// Taken, and the source would not have it. Nothing was stored, and what was
    /// there before is left alone rather than replaced with something known to
    /// be wrong.
    Refused(String),
}

/// Prepare this machine: a store, a zone, credentials, and a report of all of
/// them.
///
/// # Errors
///
/// [`Failure`] if no zone can be obtained, or if the store cannot be created or
/// written.
pub async fn init(
    database: &Path,
    declared: Option<&str>,
    force: bool,
    credential_from_stdin: bool,
) -> Result<Prepared, Failure> {
    // **The store creates itself; its directory does not.**
    paths::ensure_parent(database).map_err(|error| {
        Failure::message(
            format!(
                "cannot create the directory for {}: {error}",
                database.display()
            ),
            exit::STORE,
        )
    })?;
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let settings = SqliteOperatorSettingsStore::new(pool.clone());
    let in_force = settings
        .timezone()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    // **Refusing to restate a zone already in force is the whole of the safety
    // here**, and it is a smaller safety than the hand-edited file it replaces
    // needed: § 12 keeps the superseded row, so `--force` supersedes rather than
    // destroys. What it stops is a second `init` — run to connect a source —
    // quietly moving the zone every prescription is generated against.
    let zone = match (in_force.as_deref(), declared) {
        (Some(stated), None) if !force => stated.to_owned(),
        (Some(stated), Some(given)) if !force && stated != given => {
            return Err(Failure::message(
                format!(
                    "the time zone in force is {stated}, and this would state {given}. \
                     Pass --force to supersede it — the old value is kept either way"
                ),
                exit::USAGE,
            ));
        }
        (Some(stated), Some(given)) if stated == given => stated.to_owned(),
        _ => {
            let obtained = zone_for(declared, in_force.as_deref())?;
            settings
                .author(jiff::Timestamp::now(), &obtained)
                .await
                .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
            obtained
        }
    };

    let parameters = seed_parameters(&SqliteGenerationParameterStore::new(pool.clone())).await?;
    pool.close().await;

    let credentials_path =
        paths::credentials(&paths::SystemEnvironment).map_err(config::ConfigError::from)?;
    let credentials = obtain_credentials(&credentials_path, credential_from_stdin).await?;

    Ok(Prepared {
        database: database.to_path_buf(),
        credentials_path,
        zone,
        parameters,
        credentials,
    })
}

/// Put this build's parameters in the store, unless a set is already in force.
///
/// **Dated rather than overwritten** (§ 12). The seed becomes a row like any
/// other, so the value in force at a time stays recoverable and a later change
/// to what this build ships rewrites nothing already authored.
async fn seed_parameters(
    store: &SqliteGenerationParameterStore,
) -> Result<ParameterOutcome, Failure> {
    if store
        .current()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .is_some()
    {
        return Ok(ParameterOutcome::AlreadyInForce);
    }

    store
        .author(
            jiff::Timestamp::now(),
            &domain::prescription::seed::seed()
                .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?,
        )
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    Ok(ParameterOutcome::Seeded)
}

/// Obtain, check and store a credential for each source not yet connected.
///
/// **A source already connected is left alone.** `init` is run once, on a new
/// machine; adding or replacing one source's credential is
/// `fitness credentials <source>`, which is [`credential`] and nothing else.
/// Re-asking here for what is already stored would make `init` the command an
/// operator reruns, and walks them past every source they already have.
///
/// **Three ways in, and none of them is argv.** Standard input when asked for,
/// so a password manager can pipe one; the environment where it already answers,
/// which is left alone rather than copied; and a prompt with echo off where
/// there is somebody to ask.
async fn obtain_credentials(
    path: &Path,
    from_stdin: bool,
) -> Result<Vec<(String, CredentialOutcome)>, Failure> {
    let mut stored = Credentials::read(path)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let mut outcomes = Vec::new();
    let mut changed = false;

    for source in &crate::catalogue::SOURCES {
        let outcome = if let Some(outcome) = connected(source, &stored)? {
            outcome
        } else {
            let outcome = take(source, &mut stored, from_stdin).await?;
            changed |= outcome == CredentialOutcome::Stored;
            outcome
        };
        outcomes.push((source.name().to_owned(), outcome));
    }

    // **A pipe that answered for nothing is a mistake, not a choice.** Something
    // meant to supply a credential and did not, and storing nothing quietly
    // would leave that to be discovered at the source.
    if from_stdin
        && outcomes
            .iter()
            .all(|(_, outcome)| matches!(outcome, CredentialOutcome::Outstanding))
    {
        return Err(Failure::message(
            "no credential on standard input. One line per value, in the order the sources \
             are listed — a key on one line, an email and a password on two"
                .to_owned(),
            exit::USAGE,
        ));
    }

    if changed {
        stored
            .write(path)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    }

    Ok(outcomes)
}

/// Take, check and store one source's credential, replacing any held.
///
/// What `fitness credentials <source>` does. Unlike `init` it asks even when
/// something is stored: asking is the point of running it.
///
/// # Errors
///
/// [`Failure`] if the credentials file cannot be read or written, or a value
/// cannot be read.
pub async fn credential(
    known: &crate::catalogue::KnownSource,
    from_stdin: bool,
) -> Result<(PathBuf, CredentialOutcome), Failure> {
    let path = paths::credentials(&paths::SystemEnvironment).map_err(config::ConfigError::from)?;
    let mut stored = Credentials::read(&path)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let outcome = take(known, &mut stored, from_stdin).await?;
    if outcome == CredentialOutcome::Stored {
        stored
            .write(&path)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    }
    Ok((path, outcome))
}

/// Whether a source can already be reached, and from where.
///
/// **An application is not access until it has been granted**, so a source
/// reached through one is connected only once its token is kept.
fn connected(
    known: &crate::catalogue::KnownSource,
    stored: &Credentials,
) -> Result<Option<CredentialOutcome>, Failure> {
    let answered = if from_environment(known).is_some() {
        Some(CredentialOutcome::InEnvironment)
    } else {
        stored
            .credential(known.name())
            .map(|_| CredentialOutcome::AlreadyStored)
    };
    let Some(answered) = answered else {
        return Ok(None);
    };

    if let Credential::OAuthClient { .. } = known.credential()
        && !token_file(known)?.path().is_file()
    {
        return Ok(None);
    }
    Ok(Some(answered))
}

/// Every variable a source needs, if the environment sets them all.
fn from_environment(known: &crate::catalogue::KnownSource) -> Option<Vec<String>> {
    known
        .required_variables()
        .iter()
        .map(|variable| {
            std::env::var(variable)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .collect()
}

fn token_file(known: &crate::catalogue::KnownSource) -> Result<TokenFile, Failure> {
    paths::token(&paths::SystemEnvironment, known.name())
        .map(TokenFile::new)
        .map_err(|error| Failure::usage(&config::ConfigError::from(error)))
}

/// Take one source's credential, check it, and record it in `stored`.
async fn take(
    known: &crate::catalogue::KnownSource,
    stored: &mut Credentials,
    from_stdin: bool,
) -> Result<CredentialOutcome, Failure> {
    // **An application from the environment is still signed in**, and not
    // copied: the variables own it, and the token is what the sign-in makes.
    if let Credential::OAuthClient { .. } = known.credential()
        && let Some(values) = from_environment(known)
        && let [client_id, client_secret, redirect_uri] = values.as_slice()
    {
        let client = StoredCredential::OAuthClient {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            redirect_uri: redirect_uri.clone(),
        };
        return Ok(match verify(known, &client).await {
            Ok(()) => CredentialOutcome::InEnvironment,
            Err(detail) => CredentialOutcome::Refused(detail),
        });
    }

    // Already answered by the environment. Copying it into a file would
    // duplicate a value that has an owner, and the copy is the one that goes
    // stale.
    if from_environment(known).is_some() {
        return Ok(CredentialOutcome::InEnvironment);
    }

    let Some(taken) = obtain(known, from_stdin)? else {
        return Ok(CredentialOutcome::Outstanding);
    };

    // **Checked before it is written.** What is stored is then known to work
    // rather than merely known to have been typed.
    Ok(match verify(known, &taken).await {
        Ok(()) => {
            stored.set(known.name(), taken);
            CredentialOutcome::Stored
        }
        Err(detail) => CredentialOutcome::Refused(detail),
    })
}

/// Offer a credential to its source and see whether it is accepted.
///
/// **An arm per source, like `cli::wiring`'s**, and for the same reason: which
/// adapter can answer for a credential is a fact about the source, not about the
/// shape of the credential. A build that grows a second key-based source adds an
/// arm here rather than inheriting Hevy's client by accident.
async fn verify(
    known: &crate::catalogue::KnownSource,
    credential: &StoredCredential,
) -> Result<(), String> {
    let base_url = std::env::var(known.base_url_variable())
        .unwrap_or_else(|_| known.default_base_url().to_owned());
    let auth_base_url = |default: &str| {
        std::env::var(known.auth_base_url_variable()).unwrap_or_else(|_| default.to_owned())
    };

    match (known.name(), credential, known.credential()) {
        ("hevy", StoredCredential::ApiKey { key }, _) => {
            infrastructure::HevyWorkoutEvents::new(base_url, key.clone())
                .verify()
                .await
                .map_err(|error| error.to_string())
        }
        (
            "peloton",
            StoredCredential::Login { email, password },
            Credential::EmailPassword {
                default_auth_base_url,
            },
        ) => infrastructure::peloton::auth::PelotonAuth::new(
            auth_base_url(default_auth_base_url),
            infrastructure::peloton::auth::PelotonCredentials::new(email.clone(), password.clone()),
        )
        .bearer()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string()),
        // **Garmin's own arm, though it shares Peloton's credential kind.**
        // Which adapter answers for a login is a fact about the source: these
        // two both take an email and a password and neither can check the
        // other's. Garmin also needs two hosts rather than one, and the second
        // is derived from the first so that a stub reaches both.
        (
            "garmin",
            StoredCredential::Login { email, password },
            Credential::EmailPassword {
                default_auth_base_url,
            },
        ) => {
            let sso = auth_base_url(default_auth_base_url);
            infrastructure::GarminAuth::new(
                sso.clone(),
                infrastructure::garmin::token_base_for(&sso),
                infrastructure::GarminCredentials::new(email.clone(), password.clone()),
                None,
            )
            .bearer()
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
        }
        (
            "withings",
            StoredCredential::OAuthClient {
                client_id,
                client_secret,
                redirect_uri,
            },
            Credential::OAuthClient {
                default_auth_base_url,
            },
        ) => {
            let cache = token_file(known).map_err(|failure| failure.message_text().to_owned())?;
            let auth = WithingsAuth::new(
                base_url,
                auth_base_url(default_auth_base_url),
                WithingsClient::new(
                    client_id.clone(),
                    client_secret.clone(),
                    redirect_uri.clone(),
                ),
                cache,
            );
            sign_in(&auth).await
        }
        (name, _, _) => Err(format!("this build cannot check a {name} credential")),
    }
}

/// Send the operator to a browser, and exchange what they bring back.
///
/// **The paste is read with echo on.** It is an address with a single-use code
/// that expires in thirty seconds, and a paste nobody can see is one nobody can
/// check was whole.
async fn sign_in(auth: &WithingsAuth) -> Result<(), String> {
    if !std::io::stdin().is_terminal() {
        return Err("signing in needs a browser and a terminal to paste into".to_owned());
    }
    let state = WithingsAuth::new_state();
    let address = auth
        .authorisation_url(&state)
        .map_err(|error| error.to_string())?;

    println!();
    println!("Open this address, allow access, and paste the address your browser lands on.");
    println!("Be quick: the code in it lasts thirty seconds.");
    println!();
    println!("  {address}");
    println!();
    print!("Address: ");
    std::io::stdout()
        .flush()
        .map_err(|error| error.to_string())?;

    let mut pasted = String::new();
    std::io::stdin()
        .read_line(&mut pasted)
        .map_err(|error| error.to_string())?;

    auth.sign_in(&pasted, &state)
        .await
        .map_err(|error| error.to_string())
}

/// One source's credential, from standard input or from a prompt.
///
/// `None` where there is nothing to take and nobody to ask, which is a state to
/// report rather than an error: the rest of the setup is still worth having.
fn obtain(
    known: &crate::catalogue::KnownSource,
    from_stdin: bool,
) -> Result<Option<StoredCredential>, Failure> {
    // Once per source, however many values it takes.
    if !from_stdin && std::io::stdin().is_terminal() {
        announce(known);
    }
    match known.credential() {
        Credential::ApiKey => {
            let Some(key) = one_secret(from_stdin, "Paste the key here")? else {
                return Ok(None);
            };
            Ok(Some(StoredCredential::ApiKey { key }))
        }
        Credential::EmailPassword { .. } => {
            let Some(email) = typed(from_stdin, "Which email do you sign in with?")? else {
                return Ok(None);
            };
            let Some(password) = one_secret(from_stdin, "Password")? else {
                return Ok(None);
            };
            Ok(Some(StoredCredential::Login { email, password }))
        }
        // **Not from a pipe.** An application is only useful once somebody has
        // granted it at a browser, so there is nothing a pipe alone can finish —
        // and reading its lines would take the next source's.
        Credential::OAuthClient { .. } if from_stdin => Ok(None),
        Credential::OAuthClient { .. } => {
            let Some(client_id) = typed(false, "Client ID")? else {
                return Ok(None);
            };
            let Some(client_secret) = one_secret(false, "Client secret")? else {
                return Ok(None);
            };
            let Some(redirect_uri) = typed(
                false,
                "Callback URL, exactly as registered on the dashboard",
            )?
            else {
                return Ok(None);
            };
            Ok(Some(StoredCredential::OAuthClient {
                client_id,
                client_secret,
                redirect_uri,
            }))
        }
    }
}

/// Say which source is being asked about, and where its credential comes from.
fn announce(known: &crate::catalogue::KnownSource) {
    println!(
        "A {} credential is needed to read from it. Get one from {}",
        known.name(),
        known.credential_url()
    );
}

/// A value read with echo off, or from standard input.
fn one_secret(from_stdin: bool, prompt: &str) -> Result<Option<String>, Failure> {
    if from_stdin {
        return from_standard_input();
    }
    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let typed = rpassword::prompt_password(format!(
        "{prompt} (it will not be shown), or press enter to skip: "
    ))
    .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let typed = typed.trim().to_owned();
    Ok(if typed.is_empty() { None } else { Some(typed) })
}

/// A value read with echo on: an email is not a secret, and typing one blind is
/// how the wrong account gets connected.
fn typed(from_stdin: bool, prompt: &str) -> Result<Option<String>, Failure> {
    if from_stdin {
        return from_standard_input();
    }
    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    print!("{prompt} (or press enter to skip): ");
    std::io::stdout()
        .flush()
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let line = line.trim().to_owned();
    Ok(if line.is_empty() { None } else { Some(line) })
}

/// One line of standard input.
///
/// **An empty line skips that source rather than failing.** One pipe answers
/// for every source this build knows, in catalogue order — a key on one line, a
/// login on two — and an operator who has a Hevy key and no Peloton account has
/// to be able to say so. What is refused is a pipe that answers for *nothing*,
/// which is checked once at the end rather than per source.
fn from_standard_input() -> Result<Option<String>, Failure> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let line = line.trim().to_owned();
    Ok(if line.is_empty() { None } else { Some(line) })
}

/// The zone: the one given, or the one somebody types.
///
/// Validated here rather than on first use, because the point of a setup
/// command is that a mistake surfaces while the operator is still thinking
/// about it.
fn zone_for(declared: Option<&str>, in_force: Option<&str>) -> Result<String, Failure> {
    if let Some(stated) = declared {
        return validated(stated);
    }

    if !std::io::stdin().is_terminal() {
        return Err(Failure::message(
            "no time zone: pass --timezone. There is nobody to ask, and nothing is \
             compiled in — a default would be an assumption about where you train"
                .to_owned(),
            exit::USAGE,
        ));
    }

    // The zone already in force is what a second `init` is most likely to want,
    // and offering it beats offering this author's own.
    let offered = in_force.unwrap_or("Europe/London");
    print!("Which IANA time zone do you train in? [{offered}] ");
    std::io::stdout()
        .flush()
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let mut typed = String::new();
    std::io::stdin()
        .read_line(&mut typed)
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let typed = typed.trim();
    validated(if typed.is_empty() { offered } else { typed })
}

/// **The offered default is only ever a default at a prompt**, where somebody is
/// looking at it and can say otherwise. Nothing accepts it silently.
fn validated(value: &str) -> Result<String, Failure> {
    config::timezone(Some(value), None)
        .map(|zone| zone.id().to_owned())
        .map_err(|error| Failure::usage(&error))
}
