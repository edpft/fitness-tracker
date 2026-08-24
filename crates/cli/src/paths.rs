//! Where the tool keeps its store and its settings.
//!
//! **A default location is not a hardcoded path.** § 34 forbids compiling in an
//! assumption about a host, and the two are different things: these resolve from
//! the environment at every invocation, and moving between machines needs no
//! code change. What § 34 rules out is a path that cannot be moved, not one that
//! does not have to be typed.
//!
//! The existing refusal to default the store said *"a store that appears
//! wherever the command was run is worse than one that has to be named"*. That
//! is an argument against a **relative** default — a `local.db` per directory,
//! silently multiplying — and it survives here intact, because a base directory
//! is one fixed place per machine rather than one per shell.
//!
//! ## The rules, in full
//!
//! The XDG Base Directory specification is two lines, which is why this is
//! spelled out rather than taken as a dependency: a variable if it is set to an
//! absolute path, and a fixed fallback under the home directory otherwise. The
//! specification is explicit that a relative value is to be ignored, and that is
//! the one part a naive reading gets wrong.
//!
//! ## What goes where
//!
//! - **Data** is the store. It is the thing that would hurt to lose, and it is
//!   not something an operator edits.
//! - **Config** is what the operator states: the zone they train in. It is
//!   hand-edited, backed up with dotfiles, and reproducible.
//!
//! - **Credentials** sit beside the settings, in a file of their own that is
//!   created owner-only. § 35 allows a key in local config; what it must not
//!   share is the file an operator keeps with their dotfiles. See
//!   [`infrastructure::credentials`].

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The directory name both bases get, and the binary an operator types.
const APPLICATION: &str = "fitness-tracker";

/// The store's file name inside the data directory.
const STORE: &str = "store.db";

/// The settings file's name inside the config directory.
const SETTINGS: &str = "config.toml";

/// Why a base directory could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "cannot work out where to keep {what}: neither {variable} nor HOME is set to an absolute \
     path. Pass the location explicitly, or set one of them"
)]
pub struct NoBaseDirectory {
    what: &'static str,
    variable: &'static str,
}

/// What the environment says, so this is testable without touching the real one.
///
/// A trait rather than two `Option<OsString>` arguments because the fallback
/// consults a *third* variable, and a caller passing them positionally would
/// eventually pass them in the wrong order.
pub trait Environment {
    fn var(&self, key: &str) -> Option<OsString>;
}

/// The real environment.
pub struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn var(&self, key: &str) -> Option<OsString> {
        std::env::var_os(key)
    }
}

/// Where the store lives, unless the operator says otherwise.
///
/// # Errors
///
/// [`NoBaseDirectory`] if neither `XDG_DATA_HOME` nor `HOME` gives an absolute
/// path.
pub fn store(environment: &impl Environment) -> Result<PathBuf, NoBaseDirectory> {
    base(
        environment,
        "XDG_DATA_HOME",
        &[".local", "share"],
        "the store",
    )
    .map(|base| base.join(APPLICATION).join(STORE))
}

/// Where the settings live.
///
/// # Errors
///
/// [`NoBaseDirectory`] if neither `XDG_CONFIG_HOME` nor `HOME` gives an absolute
/// path.
pub fn settings(environment: &impl Environment) -> Result<PathBuf, NoBaseDirectory> {
    base(environment, "XDG_CONFIG_HOME", &[".config"], "the settings")
        .map(|base| base.join(APPLICATION).join(SETTINGS))
}

/// One base directory: the variable if it names an absolute path, else the
/// fallback under `HOME`.
///
/// **A relative value is ignored rather than resolved.** The specification says
/// so, and the reason is worth keeping in mind: resolving it against the working
/// directory is exactly the per-shell store the old refusal was written to
/// prevent.
fn base(
    environment: &impl Environment,
    variable: &'static str,
    fallback: &[&str],
    what: &'static str,
) -> Result<PathBuf, NoBaseDirectory> {
    if let Some(stated) = environment.var(variable) {
        let stated = PathBuf::from(stated);
        if stated.is_absolute() {
            return Ok(stated);
        }
    }

    let home = environment
        .var("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or(NoBaseDirectory { what, variable })?;

    Ok(fallback
        .iter()
        .fold(home, |path, segment| path.join(segment)))
}

/// Create a file's parent directory, so writing it does not fail on a fresh
/// machine.
///
/// # Errors
///
/// The underlying I/O error, which the caller reports with the path in hand.
pub fn ensure_parent(file: &Path) -> std::io::Result<()> {
    file.parent().map_or(Ok(()), std::fs::create_dir_all)
}

#[cfg(test)]
mod tests {
    use super::{Environment, NoBaseDirectory, settings, store};
    use std::{collections::BTreeMap, ffi::OsString};

    struct Fake(BTreeMap<&'static str, &'static str>);

    impl Environment for Fake {
        fn var(&self, key: &str) -> Option<OsString> {
            self.0.get(key).map(OsString::from)
        }
    }

    fn environment(pairs: &[(&'static str, &'static str)]) -> Fake {
        Fake(pairs.iter().copied().collect())
    }

    #[test]
    fn the_variables_win_where_they_are_absolute() {
        let set = environment(&[
            ("XDG_DATA_HOME", "/data"),
            ("XDG_CONFIG_HOME", "/config"),
            ("HOME", "/home/someone"),
        ]);

        assert_eq!(
            store(&set).expect("a data base"),
            std::path::Path::new("/data/fitness-tracker/store.db")
        );
        assert_eq!(
            settings(&set).expect("a config base"),
            std::path::Path::new("/config/fitness-tracker/config.toml")
        );
    }

    #[test]
    fn home_supplies_the_specified_fallbacks() {
        let set = environment(&[("HOME", "/home/someone")]);

        assert_eq!(
            store(&set).expect("a data base"),
            std::path::Path::new("/home/someone/.local/share/fitness-tracker/store.db")
        );
        assert_eq!(
            settings(&set).expect("a config base"),
            std::path::Path::new("/home/someone/.config/fitness-tracker/config.toml")
        );
    }

    /// **The specification says to ignore a relative value**, and this is the
    /// part a naive reading gets wrong: resolving it against the working
    /// directory would reintroduce the per-shell store that refusing to default
    /// was written to prevent.
    #[test]
    fn a_relative_variable_is_ignored_rather_than_resolved() {
        let set = environment(&[
            ("XDG_DATA_HOME", "relative/data"),
            ("HOME", "/home/someone"),
        ]);

        assert_eq!(
            store(&set).expect("a data base"),
            std::path::Path::new("/home/someone/.local/share/fitness-tracker/store.db")
        );
    }

    /// An empty variable is not a location. It is the shape a `VAR=` in a shell
    /// profile takes, and treating it as the root would put the store in `/`.
    #[test]
    fn an_empty_variable_falls_back() {
        let set = environment(&[("XDG_CONFIG_HOME", ""), ("HOME", "/home/someone")]);

        assert_eq!(
            settings(&set).expect("a config base"),
            std::path::Path::new("/home/someone/.config/fitness-tracker/config.toml")
        );
    }

    /// Nothing to go on is reported, never guessed. A store in the working
    /// directory is what this whole module exists to avoid.
    #[test]
    fn nothing_to_go_on_is_an_error_rather_than_a_guess() {
        let bare = environment(&[]);

        let refused: Result<_, NoBaseDirectory> = store(&bare);
        assert!(refused.is_err());
        assert!(settings(&bare).is_err());

        let message = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(message.contains("XDG_DATA_HOME"), "{message}");
        assert!(message.contains("the store"), "{message}");
    }

    /// A relative `HOME` is no more usable than an absent one.
    #[test]
    fn a_relative_home_is_not_a_home() {
        let set = environment(&[("HOME", "somewhere")]);
        assert!(store(&set).is_err());
    }
}
