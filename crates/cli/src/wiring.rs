//! Which adapters serve which stream.
//!
//! The composition root proper: the only module in this crate that names a
//! concrete adapter. Every stream gets exactly one arm of [`run`], because
//! choosing implementations is what a composition root is for and the choice
//! differs per stream — `hevy.workouts` has an HTTP feed and a landing table
//! shaped for one; a source that hands over a CSV export will have neither.
//!
//! A stream in the catalogue with no arm here is a build that can name
//! something it cannot do. That is a mistake rather than an invocation error,
//! and it says so.

use std::path::Path;

use application::{
    DerivationStatus, DerivationStatusReporter, ExtractionError, ExtractionStatusReporter,
    LandingStore, NormalisationError,
    NormalisationSummary, RefusalReport, RefusalReporter, ResumptionPointResetter,
    ResumptionPointStore, RunSummary, StatusError, StreamStatus, WorkoutExtractor,
    WorkoutNormaliser,
    extract::{Extraction, ExtractionPorts},
    normalise::{DerivationStanding, Normalisation, NormalisationPorts, Refusals},
    status::ExtractionStatus,
};
use domain::{
    gym::OperatorZone,
    landing::{FetchedAt, Watermark},
};
use infrastructure::{
    FileRunLock, HevyWorkoutEvents, HevyWorkoutLandingReader, HevyWorkoutLandingStore,
    HevyWorkoutTranslator,
    SqliteExtractionRunLog, SqliteGymWorkoutStore, SqliteNormalisationRunLog, SqliteRefusalStore,
    SqliteResumptionPointStore, connect,
};

use crate::{catalogue::KnownStream, config::SourceAccess};

/// What the operator asked for.
///
/// `Extract` carries what it takes to reach the source, because it is the only
/// one of the three that contacts it. `status` and `reset` read what previous
/// runs left behind and must keep working with no credential and no network.
pub enum Command {
    Extract(SourceAccess),
    /// Derive the normalised layer from what raw already holds. Contacts no
    /// source, which is why it carries a zone rather than a credential.
    Normalise(OperatorZone),
    /// Read back what the last derivation would not accept. Takes no zone: it
    /// consults none to produce the list.
    Refusals,
    Status,
    Reset,
}

/// What happened, in terms the output module can print.
pub enum Outcome {
    Extracted(Box<RunSummary>),
    Derived(Box<NormalisationSummary>),
    Refused(Box<RefusalReport>),
    Reported {
        extraction: Box<StreamStatus>,
        derivation: Box<DerivationStatus>,
    },
    Reset { previous: Option<Watermark> },
}

/// The wall clock, which is the only thing a real run should take its timings
/// from — and emphatically not where the resumption point comes from.
struct SystemClock;

impl application::Clock for SystemClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::from(jiff::Timestamp::now())
    }
}

/// Why a command could not be carried out here.
#[derive(Debug, thiserror::Error)]
pub enum WiringError {
    #[error(transparent)]
    Extraction(#[from] ExtractionError),
    #[error(transparent)]
    Normalisation(#[from] NormalisationError),
    #[error(transparent)]
    Status(#[from] StatusError),
    #[error(transparent)]
    Store(#[from] application::StoreError),
    #[error("this build knows the stream {stream} but has no adapters wired for it")]
    Unwired { stream: String },
    #[error(transparent)]
    Stream(#[from] domain::landing::InvalidStream),
}

/// Carry out `command` against `known`, with whatever adapters that stream
/// takes.
///
/// Note what is *not* passed on: the stream. Each arm builds adapters that are
/// already bound to one, and the use cases read it back out of them. The
/// catalogue's name selects the arm and nothing more, so a run's identity can
/// only ever come from the adapters actually doing the work.
///
/// # Errors
///
/// [`WiringError`] if the run fails, the store is unavailable, or the stream
/// has no adapters here.
pub async fn run(
    command: Command,
    known: &KnownStream,
    database: &Path,
) -> Result<Outcome, WiringError> {
    match known.name().as_str() {
        HevyWorkoutLandingStore::STREAM => hevy_workouts(command, database).await,
        other => Err(WiringError::Unwired {
            stream: other.to_owned(),
        }),
    }
}

/// Hevy's workout events feed, landed into the table shaped for it.
async fn hevy_workouts(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            let extraction = Extraction::new(ExtractionPorts {
                source: HevyWorkoutEvents::new(access.base_url, access.api_key),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });

            let summary = extraction.extract().await?;
            Ok(Outcome::Extracted(Box::new(summary)))
        }
        Command::Normalise(zone) => {
            // No lock. A derivation reads raw and writes only its own tables,
            // so it neither takes the extraction lock nor advances the
            // resumption point — the two commands can run at once.
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: HevyWorkoutLandingReader::new(pool.clone())?,
                    translator: HevyWorkoutTranslator,
                    workouts: SqliteGymWorkoutStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(pool.clone())?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );

            let summary = normalisation.normalise().await?;
            Ok(Outcome::Derived(Box::new(summary)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone())?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            // Both halves, because § 38 is about the whole chain: an
            // extraction that is up to date and a derivation that is eight
            // records behind is a system with a silent problem.
            let derivation = DerivationStanding::new(
                HevyWorkoutLandingStore::new(pool.clone())?,
                SqliteGymWorkoutStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone())?,
                SqliteNormalisationRunLog::new(pool),
            )
            .derivation_status()
            .await?;

            let reader = ExtractionStatus::new(landing, resumption, runs);
            Ok(Outcome::Reported {
                extraction: Box::new(reader.status().await?),
                derivation: Box::new(derivation),
            })
        }
        Command::Reset => {
            // Read first, so the operator is told what was discarded rather
            // than only that something was.
            let previous = resumption.read(landing.stream()).await?;
            let reader = ExtractionStatus::new(landing, resumption, runs);
            reader.reset().await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HevyWorkoutLandingStore;
    use crate::catalogue::KNOWN;

    /// Every catalogue entry must be reachable, and must name the same stream
    /// its adapters do.
    ///
    /// This is the one seam the compiler cannot close. `run` dispatches on the
    /// catalogue's name and the adapters declare their own, so the two agreeing
    /// is what makes the whole chain hold: disagree, and an operator asking for
    /// `hevy.workouts` either gets "no adapters wired" for a stream this build
    /// plainly has, or — worse — reaches adapters keeping a different stream's
    /// books. Adding a stream without adding its arm fails here rather than in
    /// front of an operator.
    #[test]
    fn every_catalogue_entry_is_wired_to_adapters_that_name_it() {
        let wired = [HevyWorkoutLandingStore::STREAM];

        for known in &KNOWN {
            assert!(
                wired.contains(&known.name().as_str()),
                "{} is in the catalogue but no adapter declares it",
                known.name()
            );
        }
        assert_eq!(
            wired.len(),
            KNOWN.len(),
            "an adapter is wired but uncollectable"
        );
    }
}
