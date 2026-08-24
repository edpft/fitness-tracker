//! Getting a fresh machine ready to use.
//!
//! **A wizard is not a shortcut for something the tool can work out.** Every
//! value it asks for is one nothing can derive: which zone the operator trains
//! in, and where they want their store. It asks for nothing else, and it
//! *reports* everything else — where things went, and what still needs doing —
//! because the failure mode of a setup command is leaving someone unsure
//! whether it worked.
//!
//! **It does not ask for the credential.** § 35 keeps that in the environment,
//! so the most this can do is say which variable to set and where to get a key.
//! Prompting for a secret and writing it to a file would be the one thing this
//! module could do that the operator cannot easily undo.
//!
//! **Interactive only when there is somebody there.** A missing value is
//! prompted for at a terminal and refused without one, so the same command works
//! under a scheduler — which is what stops `init` becoming the step that cannot
//! be automated.

use std::{
    io::{IsTerminal, Write},
    path::Path,
};

use infrastructure::{Settings, connect};

use crate::{Failure, config, exit, paths};

/// What `init` found or made.
pub struct Prepared {
    pub settings_path: std::path::PathBuf,
    pub database: std::path::PathBuf,
    pub zone: String,
    /// Whether the credential is already in the environment. Reported rather
    /// than fixed.
    pub credential_set: bool,
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
    pool.close().await;

    Settings {
        timezone: Some(zone.clone()),
        // Left out unless it was asked for: a settings file naming the default
        // location pins what should follow the specification if it moves.
        database: None,
    }
    .write(settings_path)
    .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    Ok(Prepared {
        settings_path: settings_path.to_path_buf(),
        database: database.to_path_buf(),
        zone,
        credential_set: crate::catalogue::SOURCES
            .iter()
            .all(|source| std::env::var_os(source.api_key_variable()).is_some()),
    })
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
