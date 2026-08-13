//! What this build knows how to collect.
//!
//! One entry per landing stream, not per source. A source serving a second
//! kind of thing — Hevy's routines, its measurements — is a second entry here
//! and a second arm in [`crate::wiring`], because the two resume, run and lock
//! independently and have nothing else in common.
//!
//! Everything a source needs from the environment is derived from its name
//! rather than written down per source, so the second source needs no new
//! constants and no new flag.

use domain::landing::{InvalidStream, LandingStream};

/// A stream this build can collect, and what it takes to reach it.
pub struct KnownStream {
    source: &'static str,
    entity: &'static str,
    /// The API root, with no version segment: the adapter owns the path it
    /// calls, and a base that already ends in `/v1` composes `/v1/v1/…`.
    ///
    /// A default rather than a constant, because the contract tests point this
    /// at a local stub.
    default_base_url: &'static str,
    /// Where an operator goes to get a credential, quoted back at them when
    /// the variable is unset.
    credential_url: &'static str,
}

impl KnownStream {
    /// The name an operator types, and the one every message prints back.
    pub fn name(&self) -> String {
        format!("{}.{}", self.source, self.entity)
    }

    pub fn default_base_url(&self) -> &'static str {
        self.default_base_url
    }

    pub fn credential_url(&self) -> &'static str {
        self.credential_url
    }

    /// # Errors
    ///
    /// [`InvalidStream`] if an entry here does not name a stream. Pinned by a
    /// test, so it is a mistake in this table rather than in an invocation.
    pub fn landing_stream(&self) -> Result<LandingStream, InvalidStream> {
        LandingStream::try_from(self.name().as_str())
    }

    /// Where the credential comes from. Env-only, with no flag anywhere: a
    /// secret passed on the command line lands in shell history and in `ps`
    /// output for every other user on the machine.
    pub fn api_key_variable(&self) -> String {
        self.variable("API_KEY")
    }

    /// Where the base URL comes from when no flag overrides it.
    pub fn base_url_variable(&self) -> String {
        self.variable("API_BASE_URL")
    }

    /// Per source rather than per stream: a credential and an API root belong
    /// to the system that issued them, and every stream of one source shares
    /// both.
    fn variable(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.source.to_uppercase())
    }
}

/// Every stream this build can collect.
pub const KNOWN: [KnownStream; 1] = [KnownStream {
    source: "hevy",
    entity: "workouts",
    default_base_url: "https://api.hevyapp.com",
    credential_url: "https://hevy.com/settings?developer",
}];

/// The entry an operator named, if this build has one.
pub fn lookup(name: &str) -> Option<&'static KnownStream> {
    KNOWN.iter().find(|known| known.name() == name)
}

/// What to offer when an operator names something this build does not have.
pub fn known_names() -> String {
    KNOWN
        .iter()
        .map(KnownStream::name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{KNOWN, lookup};

    /// Every entry names a stream the domain accepts, and can be found by the
    /// name it prints. A typo here would otherwise surface as an invocation
    /// that cannot be spelled at all.
    #[test]
    fn every_entry_round_trips_through_its_name() {
        for known in &KNOWN {
            let stream = known
                .landing_stream()
                .expect("a catalogue entry names a stream");
            assert_eq!(stream.to_string(), known.name());
            assert!(lookup(&known.name()).is_some());
        }
    }

    /// The variables are derived, so this pins the shape rather than the list.
    #[test]
    fn the_environment_variables_are_named_after_the_source() {
        let hevy = lookup("hevy.workouts").expect("hevy.workouts is in the catalogue");
        assert_eq!(hevy.api_key_variable(), "HEVY_API_KEY");
        assert_eq!(hevy.base_url_variable(), "HEVY_API_BASE_URL");
    }

    /// The base URL is the root. A base that already carried `/v1` composed
    /// `/v1/v1/workouts/events` on a live run, and no stub-based test could
    /// see it.
    #[test]
    fn the_default_base_url_carries_no_version_segment() {
        let hevy = lookup("hevy.workouts").expect("hevy.workouts is in the catalogue");
        assert_eq!(hevy.default_base_url(), "https://api.hevyapp.com");
    }
}
