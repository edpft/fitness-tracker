//! Getting a fresh machine ready to use.
//!
//! **A wizard is not a shortcut for something the tool can work out.** Every
//! value it asks for is one nothing can derive: which zone the operator trains
//! in, and where they want their store. It asks for nothing else, and it
//! *reports* everything else — where things went, and what still needs doing —
//! because the failure mode of a setup command is leaving someone unsure
//! whether it worked.
//!
//! **It does ask for the credential**, and the reasoning that once said
//! otherwise conflated three different things. Passing a secret as a *flag* is
//! genuinely bad — it lands in argv, in shell history and in `ps` output, which
//! is why there is still no `--api-key`. Storing one in a *file* is what § 35
//! explicitly allows. *Prompting* for one touches neither: a typed key never
//! reaches argv, and with echo off it does not reach the scrollback either.
//!
//! What survives from the objection is where it goes: not into the settings
//! file, which is meant to be kept with an operator's dotfiles, but into a file
//! of its own created owner-only. See [`infrastructure::credentials`].
//!
//! **Interactive only when there is somebody there.** A missing value is
//! prompted for at a terminal and refused without one, so the same command works
//! under a scheduler — which is what stops `init` becoming the step that cannot
//! be automated.

use std::{
    io::{IsTerminal, Write},
    path::Path,
};

use application::GenerationParameterStore as _;
use infrastructure::{Credentials, Settings, SqliteGenerationParameterStore, connect, credentials};

use crate::{Failure, config, exit, paths};

/// What `init` found or made.
pub struct Prepared {
    pub settings_path: std::path::PathBuf,
    pub database: std::path::PathBuf,
    pub zone: String,
    /// What became of the generation parameters.
    pub parameters: ParameterOutcome,
    /// What became of each source's key.
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

/// What `init` was able to do about a source's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOutcome {
    /// Taken and written to the credentials file.
    Stored,
    /// Already in the environment, and left there. Nothing is copied into a
    /// file that the environment is already answering for.
    InEnvironment,
    /// Nobody to ask and nothing to take. Reported as outstanding.
    Outstanding,
}

/// Prepare this machine: a store, a settings file, and a report of both.
///
/// # Errors
///
/// [`Failure`] if settings already exist and `force` was not asked for, if no
/// zone can be obtained, or if the store cannot be created.
pub async fn init(
    settings_path: &Path,
    database: &Path,
    declared: Option<&str>,
    force: bool,
    key_from_stdin: bool,
) -> Result<Prepared, Failure> {
    // **Refusing to overwrite is the whole of the safety here.** A settings file
    // is hand-edited, so silently replacing one loses work that nothing else
    // holds a copy of.
    if settings_path.exists() && !force {
        return Err(Failure::message(
            format!(
                "{} already exists. Edit it, or pass --force to replace it",
                settings_path.display()
            ),
            exit::USAGE,
        ));
    }

    let zone = zone_for(declared, settings_path)?;

    // Creating the store *before* writing the settings, so a machine that cannot
    // hold a store is not left with a settings file claiming it is set up.
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
    let parameters = seed_parameters(&SqliteGenerationParameterStore::new(pool.clone())).await?;
    pool.close().await;

    Settings {
        timezone: Some(zone.clone()),
        // Left out unless it was asked for: a settings file naming the default
        // location pins what should follow the specification if it moves.
        database: None,
    }
    .write(settings_path)
    .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let credentials = keys(settings_path, key_from_stdin)?;

    Ok(Prepared {
        settings_path: settings_path.to_path_buf(),
        database: database.to_path_buf(),
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

/// Obtain and store a key for each source this build knows.
///
/// **Three ways in, and none of them is argv.** Standard input when asked for,
/// so a password manager can pipe one; the environment where it already answers,
/// which is left alone rather than copied; and a prompt with echo off where
/// there is somebody to ask.
fn keys(
    settings_path: &Path,
    from_stdin: bool,
) -> Result<Vec<(String, CredentialOutcome)>, Failure> {
    let path = credentials::beside(settings_path);
    let mut stored = Credentials::read(&path)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let mut outcomes = Vec::new();
    let mut changed = false;

    for source in &crate::catalogue::SOURCES {
        let name = source.name().to_owned();

        // Already answered by the environment. Copying it into a file would
        // duplicate a value that has an owner, and the copy is the one that goes
        // stale.
        if std::env::var_os(source.api_key_variable()).is_some() {
            outcomes.push((name, CredentialOutcome::InEnvironment));
            continue;
        }

        let Some(key) = obtain(source, from_stdin)? else {
            outcomes.push((name, CredentialOutcome::Outstanding));
            continue;
        };

        stored.set(&name, &key);
        changed = true;
        outcomes.push((name, CredentialOutcome::Stored));
    }

    if changed {
        stored
            .write(&path)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    }

    Ok(outcomes)
}

/// One source's key, from standard input or from a prompt.
///
/// `None` where there is nothing to take and nobody to ask, which is a state to
/// report rather than an error: the rest of the setup is still worth having.
fn obtain(
    source: &crate::catalogue::KnownSource,
    from_stdin: bool,
) -> Result<Option<String>, Failure> {
    if from_stdin {
        let mut typed = String::new();
        std::io::stdin()
            .read_line(&mut typed)
            .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;
        let typed = typed.trim().to_owned();
        return if typed.is_empty() {
            Err(Failure::message(
                format!("no {} on standard input", source.api_key_variable()),
                exit::USAGE,
            ))
        } else {
            Ok(Some(typed))
        };
    }

    if !std::io::stdin().is_terminal() {
        return Ok(None);
    }

    println!(
        "A {} key is needed to read your workouts. Get one from {}",
        source.name(),
        source.credential_url()
    );
    let typed = rpassword::prompt_password(
        "Paste it here (it will not be shown), or press enter to skip: ",
    )
    .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let typed = typed.trim().to_owned();
    Ok(if typed.is_empty() { None } else { Some(typed) })
}

/// The zone: the one given, or the one somebody types.
///
/// Validated here rather than on first use, because the point of a setup
/// command is that a mistake surfaces while the operator is still thinking
/// about it.
fn zone_for(declared: Option<&str>, settings_path: &Path) -> Result<String, Failure> {
    if let Some(stated) = declared {
        return validated(stated);
    }

    if !std::io::stdin().is_terminal() {
        return Err(Failure::message(
            format!(
                "no time zone: pass --timezone. There is nobody to ask, and nothing is \
                 compiled in — a default would be an assumption about where you train. \
                 Nothing was created; the settings would have gone to {}",
                settings_path.display()
            ),
            exit::USAGE,
        ));
    }

    print!("Which IANA time zone do you train in? [Europe/London] ");
    std::io::stdout()
        .flush()
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let mut typed = String::new();
    std::io::stdin()
        .read_line(&mut typed)
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let typed = typed.trim();
    validated(if typed.is_empty() {
        "Europe/London"
    } else {
        typed
    })
}

/// **The offered default is only ever a default at a prompt**, where somebody is
/// looking at it and can say otherwise. Nothing accepts it silently.
fn validated(value: &str) -> Result<String, Failure> {
    let settings = Settings::default();
    config::timezone(Some(value), &settings, Path::new("the settings file"))
        .map(|zone| zone.id().to_owned())
        .map_err(|error| Failure::usage(&error))
}
