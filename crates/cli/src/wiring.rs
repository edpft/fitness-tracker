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
    LandingStore, NormalisationError, NormalisationSummary, RefusalReport, RefusalReporter,
    ResumptionPointResetter, ResumptionPointStore, RunSummary, StatusError, StreamStatus,
    WorkoutExtractor, WorkoutNormaliser,
    extract::{Extraction, ExtractionPorts},
    normalise::{DerivationStanding, Normalisation, NormalisationPorts, Refusals},
    status::ExtractionStatus,
};
use domain::{
    landing::{FetchedAt, Watermark},
    normalised::OperatorZone,
};
use infrastructure::{
    FileRunLock, HevySessionAccountReader, HevySessionTranslator, HevyWorkoutEvents,
    HevyWorkoutLandingStore, PelotonRawExtent, PelotonRideLandingStore,
    PelotonRideSampleLandingStore, PelotonSessionAccountReader, PelotonWorkoutSamples,
    PelotonWorkouts, SqliteCyclingSessionStore, SqliteExtractionRunLog, SqliteGymSessionStore,
    SqliteNormalisationRunLog, SqliteRefusalStore, SqliteResumptionPointStore, connect,
    peloton::{
        PelotonSessionTranslator,
        auth::{PelotonAuth, PelotonCredentials},
    },
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
    /// Two walks under one entry, reported as two, because they are two runs
    /// against two endpoints and a single merged number would hide one of them
    /// failing to land anything (§ 38).
    ExtractedBoth {
        first: Box<RunSummary>,
        second: Box<RunSummary>,
    },
    Derived(Box<NormalisationSummary>),
    Refused(Box<RefusalReport>),
    Reported {
        extraction: Box<StreamStatus>,
        /// `None` for a stream that lands and does not yet derive. Optional
        /// rather than a zeroed report: "nothing normalises this" and "the
        /// normalised layer is empty" are different facts, and printing the
        /// second when the first is true would report a problem that does not
        /// exist.
        derivation: Option<Box<DerivationStatus>>,
    },
    Reset {
        previous: Option<Watermark>,
    },
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
    #[error("{stream} is reached with a login, and {given} was resolved instead")]
    WrongCredential { stream: String, given: &'static str },
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
        PelotonRideLandingStore::STREAM => peloton_rides(command, database).await,
        other => Err(WiringError::Unwired {
            stream: other.to_owned(),
        }),
    }
}

/// Peloton's rides: both walks, one entity, one command.
///
/// **Two walks behind one entry.** The ride list gives a ride's start, duration
/// and device; the performance graph gives its samples, one request per ride
/// against a different endpoint. Neither is an entity on its own and neither is
/// useful without the other, so the operator can no longer ask for one:
/// *"if one command needs to be run before another, those commands aren't
/// meaningfully separate and it shouldn't be possible to run them in the wrong
/// order"*.
///
/// They stay two landing tables and two resumption points. A landing record
/// holds one response as served (§ II.1), and the graph walk is minutes where
/// the list walk is seconds — so a graph walk that fails part-way must not cost
/// the list its watermark. What went is the choice, not the separation.
///
/// The list is walked first, and that is the one ordering that still matters:
/// the graph walk enumerates rides itself, so running it second means the
/// graphs it fetches are for rides this same command has just landed.
async fn peloton_rides(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = PelotonRideLandingStore::new(pool.clone())?;
    let samples_landing = PelotonRideSampleLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            collect_rides(access, landing, samples_landing, resumption, runs, database).await
        }
        Command::Normalise(zone) => {
            // No lock, as on the gym side: a derivation reads raw and writes
            // only its own tables.
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: PelotonSessionAccountReader::new(pool.clone())?,
                    translator: PelotonSessionTranslator,
                    workouts: SqliteCyclingSessionStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        PelotonRideLandingStore::STREAM,
                    )?,
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
                SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                PelotonRawExtent::new(
                    PelotonRideLandingStore::new(pool.clone())?,
                    PelotonRideSampleLandingStore::new(pool.clone())?,
                ),
                SqliteCyclingSessionStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            )
            .derivation_status()
            .await?;

            let reader = ExtractionStatus::new(landing, resumption, runs);
            Ok(Outcome::Reported {
                extraction: Box::new(reader.status().await?),
                derivation: Some(Box::new(derivation)),
            })
        }
        Command::Reset => {
            // Both, because both are behind one entry: resetting one and not
            // the other would leave the two walks at different points in
            // history, which is the state this entry exists to prevent.
            let previous = resumption.read(landing.stream()).await?;
            ExtractionStatus::new(
                samples_landing,
                SqliteResumptionPointStore::new(pool.clone()),
                SqliteExtractionRunLog::new(pool),
            )
            .reset()
            .await?;
            let reader = ExtractionStatus::new(landing, resumption, runs);
            reader.reset().await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

/// Both Peloton walks, in the one order that matters.
///
/// The list is walked first: the graph walk enumerates rides itself, so running
/// it second means the graphs it fetches are for rides this same command has
/// just landed.
///
/// Two `PelotonAuth`s rather than one shared, because each caches the token it
/// fetches to the same file — so the second login costs nothing and neither
/// walk holds the other's state.
async fn collect_rides(
    access: SourceAccess,
    landing: PelotonRideLandingStore,
    samples_landing: PelotonRideSampleLandingStore,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    database: &Path,
) -> Result<Outcome, WiringError> {
    let SourceAccess::EmailPassword {
        base_url,
        auth_base_url,
        email,
        password,
        ..
    } = access
    else {
        return Err(WiringError::WrongCredential {
            stream: PelotonRideLandingStore::STREAM.to_owned(),
            given: "an API key",
        });
    };

    let credentials = PelotonCredentials::new(email, password);
    let rides = Extraction::new(ExtractionPorts {
        source: PelotonWorkouts::new(
            base_url.clone(),
            PelotonAuth::new(auth_base_url.clone(), credentials.clone()),
        ),
        landing,
        resumption: resumption.clone(),
        runs: runs.clone(),
        lock: FileRunLock::beside(database),
        clock: SystemClock,
    });
    let ridden = rides.extract().await?;

    let graphs = Extraction::new(ExtractionPorts {
        source: PelotonWorkoutSamples::new(base_url, PelotonAuth::new(auth_base_url, credentials)),
        landing: samples_landing,
        resumption,
        runs,
        lock: FileRunLock::beside(database),
        clock: SystemClock,
    });
    let sampled = graphs.extract().await?;

    Ok(Outcome::ExtractedBoth {
        first: Box::new(ridden),
        second: Box::new(sampled),
    })
}

/// Hevy's workout events feed, landed into the table shaped for it.
async fn hevy_workouts(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            let SourceAccess::ApiKey {
                base_url, api_key, ..
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: HevyWorkoutLandingStore::STREAM.to_owned(),
                    given: "a login",
                });
            };

            let extraction = Extraction::new(ExtractionPorts {
                source: HevyWorkoutEvents::new(base_url, api_key),
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
                    raw: HevySessionAccountReader::new(pool.clone())?,
                    translator: HevySessionTranslator,
                    workouts: SqliteGymSessionStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        HevyWorkoutLandingStore::STREAM,
                    )?,
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
                SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
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
                SqliteGymSessionStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            )
            .derivation_status()
            .await?;

            let reader = ExtractionStatus::new(landing, resumption, runs);
            Ok(Outcome::Reported {
                extraction: Box::new(reader.status().await?),
                derivation: Some(Box::new(derivation)),
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
    use super::{HevyWorkoutLandingStore, PelotonRideLandingStore, PelotonRideSampleLandingStore};
    use crate::catalogue::{KNOWN, lookup};

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
        let wired = [
            HevyWorkoutLandingStore::STREAM,
            PelotonRideLandingStore::STREAM,
            PelotonRideSampleLandingStore::STREAM,
        ];

        for known in &KNOWN {
            assert!(
                wired.contains(&known.name().as_str()),
                "{} is in the catalogue but no adapter declares it",
                known.name()
            );
        }

        // **Streams that land behind another entry rather than under their own
        // name.** Peloton's graphs are collected by the same command that
        // collects its rides, because neither derives anything without the
        // other — so this one is wired, lands, resumes and locks, and cannot be
        // asked for. Anything else wired but uncollectable is a mistake.
        let landed_behind_another = [PelotonRideSampleLandingStore::STREAM];
        assert_eq!(
            wired.len(),
            KNOWN.len() + landed_behind_another.len(),
            "an adapter is wired but uncollectable"
        );
        for hidden in landed_behind_another {
            assert!(
                lookup(hidden).is_none(),
                "{hidden} lands behind another entry and must not be nameable"
            );
        }
    }
}
