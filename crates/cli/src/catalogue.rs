//! What this build knows how to talk to.
//!
//! Two tables, and the split between them is the point. A **source** is a
//! system: it issues one credential and answers at one API root, and everything
//! this build does with that system shares both. A **stream** is one kind of
//! thing that system serves, and is what a landing table, a resumption point
//! and a run lock are all bound to — so a source serving a second kind of thing
//! is a second stream and a second arm in [`crate::wiring`], because the two
//! resume, run and lock independently.
//!
//! Splitting them is what lets a *destination* exist. Delivering a session
//! lands nothing, so it has no stream to be named after and no watermark to
//! carry, and filing it as one would invent a landing table for something that
//! never lands. It is a source this build can reach, and nothing more.
//!
//! Everything either of them needs from the environment is derived from the
//! source's name rather than written down per entry, so the second source needs
//! no new constants and no new flag.

use domain::landing::{InvalidStream, LandingStream};

/// A system this build can talk to, and what it takes to reach it.
///
/// **The source, not the stream, is what holds a credential and an API root.**
/// They belong to the system that issued them, and every stream that system
/// serves shares both — as does every *destination* it offers. Separating the
/// two is what lets a session be delivered to Hevy without inventing a landing
/// stream for something that lands nothing.
pub struct KnownSource {
    /// The name an operator types, and the one every message prints back.
    name: &'static str,
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

impl KnownSource {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn default_base_url(&self) -> &'static str {
        self.default_base_url
    }

    pub const fn credential_url(&self) -> &'static str {
        self.credential_url
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

    fn variable(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.name.to_uppercase())
    }
}

/// A stream this build can collect, and what it takes to reach it.
pub struct KnownStream {
    source: &'static KnownSource,
    entity: &'static str,
}

impl KnownStream {
    /// The name an operator types, and the one every message prints back.
    pub fn name(&self) -> String {
        format!("{}{}{}", self.source.name, SEPARATOR, self.entity)
    }

    /// What serves this stream. Where the credential and the API root come
    /// from, and what a destination of the same system shares with it.
    pub const fn source(&self) -> &'static KnownSource {
        self.source
    }

    pub const fn default_base_url(&self) -> &'static str {
        self.source.default_base_url
    }

    pub fn api_key_variable(&self) -> String {
        self.source.api_key_variable()
    }

    pub fn base_url_variable(&self) -> String {
        self.source.base_url_variable()
    }

    /// # Errors
    ///
    /// [`InvalidStream`] if an entry here does not name a stream. Pinned by a
    /// test, so it is a mistake in this table rather than in an invocation.
    pub fn landing_stream(&self) -> Result<LandingStream, InvalidStream> {
        LandingStream::try_from(self.name().as_str())
    }
}

/// What separates a stream's two halves. The domain owns the rule; this is the
/// one place the CLI spells it.
const SEPARATOR: char = domain::landing::STREAM_SEPARATOR;

/// Every system this build can talk to.
pub const SOURCES: [KnownSource; 1] = [KnownSource {
    name: "hevy",
    default_base_url: "https://api.hevyapp.com",
    credential_url: "https://hevy.com/settings?developer",
}];

/// Every stream this build can collect.
pub const KNOWN: [KnownStream; 1] = [KnownStream {
    source: &SOURCES[0],
    entity: "workouts",
}];

/// A kind of training, and the one source and one sink it has.
///
/// **The third table, and the only one the porcelain reads.** A stream is what
/// this build can collect and a source is a system it can reach; neither answers
/// "what does the daily loop for *lifting* consist of", which is the question
/// `gym next` asks. Cycling would be a second entry naming Peloton on both
/// sides, and it is the entry rather than a second copy of the command that
/// makes it a second discipline.
///
/// **One source and one sink is a property of a discipline, not of this build.**
/// It is what makes the porcelain possible at all: `next` never has to choose
/// where to collect from or where to deliver to, because within a discipline
/// there is nothing to choose between. The plumbing commands stay flat and take
/// a stream name, because collecting is not a discipline-shaped act — body
/// weight has a source and no sink and no session to prescribe.
pub struct KnownDiscipline {
    name: &'static str,
    collects: &'static KnownStream,
    delivers_to: &'static KnownSource,
}

impl KnownDiscipline {
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// What its record is collected from.
    pub const fn collects(&self) -> &'static KnownStream {
        self.collects
    }

    /// Where its sessions are put, which is a *source* rather than a stream:
    /// delivering lands nothing, so there is no landing table to be named after.
    pub const fn delivers_to(&self) -> &'static KnownSource {
        self.delivers_to
    }
}

/// Every kind of training this build has a daily loop for.
pub const DISCIPLINES: [KnownDiscipline; 1] = [KnownDiscipline {
    name: "gym",
    collects: &KNOWN[0],
    delivers_to: &SOURCES[0],
}];

/// The discipline of that name, if this build knows it.
pub fn discipline(name: &str) -> Option<&'static KnownDiscipline> {
    DISCIPLINES.iter().find(|known| known.name() == name)
}

/// The entry an operator named, if this build has one.
pub fn lookup(name: &str) -> Option<&'static KnownStream> {
    KNOWN.iter().find(|known| known.name() == name)
}

/// The system of that name, if this build knows it.
///
/// What a *destination* is looked up by: delivering a session lands nothing, so
/// it has no stream to be named after, and reaching for one would be inventing
/// a landing table for something that never lands.
pub fn source(name: &str) -> Option<&'static KnownSource> {
    SOURCES.iter().find(|known| known.name() == name)
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
    use super::{DISCIPLINES, KNOWN, discipline, lookup, source};

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

    /// **Every discipline's source and sink are ones this build actually has.**
    /// The table is three static references, so a mistake here is a mistake
    /// nothing else can catch: a discipline pointing at a stream that was
    /// renamed would compile and then fail at the first `next`.
    #[test]
    fn every_discipline_names_a_stream_and_a_source_this_build_has() {
        for known in &DISCIPLINES {
            assert!(
                lookup(&known.collects().name()).is_some(),
                "{} collects a stream this build does not have",
                known.name()
            );
            assert!(
                source(known.delivers_to().name()).is_some(),
                "{} delivers to a source this build does not have",
                known.name()
            );
            assert!(
                discipline(known.name()).is_some(),
                "{} cannot be found by the name it prints",
                known.name()
            );
        }
    }

    /// **The gym collects from and delivers to one system.** Not a fact about
    /// Hevy — a fact about what makes the porcelain possible: within a
    /// discipline there is nothing to choose between, so `gym next` never has to
    /// ask which source or which sink. A discipline that acquired two of either
    /// would need a different command, and this is where that would be noticed.
    #[test]
    fn the_gym_has_one_source_and_one_sink() {
        let gym = discipline("gym").expect("the gym is in the catalogue");
        assert_eq!(gym.collects().name(), "hevy.workouts");
        assert_eq!(gym.delivers_to().name(), "hevy");
        assert_eq!(
            gym.collects().source().name(),
            gym.delivers_to().name(),
            "and they are the same system, which is what a round trip means"
        );
    }

    /// The variables are derived, so this pins the shape rather than the list.
    #[test]
    fn the_environment_variables_are_named_after_the_source() {
        let hevy = lookup("hevy.workouts").expect("hevy.workouts is in the catalogue");
        assert_eq!(hevy.api_key_variable(), "HEVY_API_KEY");
        assert_eq!(hevy.base_url_variable(), "HEVY_API_BASE_URL");
    }

    /// **A destination is reached through the source, not through a stream.**
    /// Delivering a session lands nothing, so it has no stream to be named
    /// after — and it still needs the same credential and the same API root as
    /// the stream that does.
    #[test]
    fn a_source_is_reachable_without_naming_a_stream() {
        let hevy = source("hevy").expect("hevy is a known source");
        assert_eq!(hevy.api_key_variable(), "HEVY_API_KEY");
        assert_eq!(hevy.default_base_url(), "https://api.hevyapp.com");

        let stream = lookup("hevy.workouts").expect("hevy.workouts is in the catalogue");
        assert_eq!(stream.source().name(), hevy.name());
        assert_eq!(stream.api_key_variable(), hevy.api_key_variable());
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
