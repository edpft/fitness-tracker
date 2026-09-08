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
    FileRunLock, HevyWorkoutEvents, HevyWorkoutLandingReader, HevyWorkoutLandingStore,
    HevyWorkoutTranslator, PelotonRideAccountReader, PelotonWorkoutLandingStore,
    PelotonWorkoutSampleLandingStore, PelotonWorkoutSamples, PelotonWorkouts,
    SqliteBikePlusRideStore, SqliteExtractionRunLog, SqliteGymWorkoutStore,
    SqliteNormalisationRunLog, SqliteRefusalStore, SqliteResumptionPointStore, connect,
    peloton::{
        PelotonRideTranslator,
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
    /// A stream that lands but has no derivation of its own.
    ///
    /// **Which is not the same as nothing deriving from it.** A Bike+ ride
    /// composes a workout record and a performance graph (§ 3.1), and is
    /// derived under the stream that *names* the ride — so the graphs are read
    /// by `normalise peloton.workouts` and this is the message for asking the
    /// other way round. A second command deriving the same entity would be two
    /// answers to one question.
    #[error(
        "{stream} lands, and {derived_by} is what derives from it: an entity composed from two \
         of a source's responses is derived under the stream that names it. Run `fitness \
         normalise {derived_by}`"
    )]
    NotYetDerived {
        stream: String,
        derived_by: &'static str,
    },
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
        PelotonWorkoutLandingStore::STREAM => peloton_workouts(command, database).await,
        PelotonWorkoutSampleLandingStore::STREAM => peloton_samples(command, database).await,
        other => Err(WiringError::Unwired {
            stream: other.to_owned(),
        }),
    }
}

/// Peloton's performance graphs, landed into the table shaped for them.
///
/// **Landing only**, as its sibling is: there is no cycling workout entity yet
/// and so nothing to translate a graph into (#56).
///
/// A walk here is a page of the workout list plus one request per workout on
/// it, so the whole history is roughly one request per workout — minutes rather
/// than seconds. It takes the same lock as any other stream and holds it for
/// that long, which is why it is a stream of its own: a slow walk of the graphs
/// cannot block a quick one of the workouts.
async fn peloton_samples(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = PelotonWorkoutSampleLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool);

    match command {
        Command::Extract(access) => {
            let SourceAccess::EmailPassword {
                base_url,
                auth_base_url,
                email,
                password,
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: PelotonWorkoutSampleLandingStore::STREAM.to_owned(),
                    given: "an API key",
                });
            };

            let auth = PelotonAuth::new(auth_base_url, PelotonCredentials::new(email, password));
            let extraction = Extraction::new(ExtractionPorts {
                source: PelotonWorkoutSamples::new(base_url, auth),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });

            let summary = extraction.extract().await?;
            Ok(Outcome::Extracted(Box::new(summary)))
        }
        Command::Normalise(_) | Command::Refusals => Err(WiringError::NotYetDerived {
            stream: PelotonWorkoutSampleLandingStore::STREAM.to_owned(),
            derived_by: PelotonWorkoutLandingStore::STREAM,
        }),
        Command::Status => {
            let reader = ExtractionStatus::new(landing, resumption, runs);
            Ok(Outcome::Reported {
                extraction: Box::new(reader.status().await?),
                derivation: None,
            })
        }
        Command::Reset => {
            let previous = resumption.read(landing.stream()).await?;
            let reader = ExtractionStatus::new(landing, resumption, runs);
            reader.reset().await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

/// Peloton's workout list, landed into the table shaped for it and derived into
/// Bike+ rides.
///
/// **The derivation reads two landing tables**, which is what constitution
/// 3.1.0 allowed: a ride's start and duration come from this stream and its
/// samples from `peloton.workout_samples`, and neither response is an entity
/// alone. So a ride is derived under *this* stream's name — the one the source
/// names a record by — and `normalise peloton.workout_samples` stays an error
/// rather than becoming a second way to ask for the same thing.
async fn peloton_workouts(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = PelotonWorkoutLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            let SourceAccess::EmailPassword {
                base_url,
                auth_base_url,
                email,
                password,
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: PelotonWorkoutLandingStore::STREAM.to_owned(),
                    given: "an API key",
                });
            };

            let auth = PelotonAuth::new(auth_base_url, PelotonCredentials::new(email, password));
            let extraction = Extraction::new(ExtractionPorts {
                source: PelotonWorkouts::new(base_url, auth),
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
            // No lock, as on the gym side: a derivation reads raw and writes
            // only its own tables.
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: PelotonRideAccountReader::new(pool.clone())?,
                    translator: PelotonRideTranslator,
                    workouts: SqliteBikePlusRideStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        PelotonWorkoutLandingStore::STREAM,
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
                SqliteRefusalStore::new(pool.clone(), PelotonWorkoutLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                PelotonWorkoutLandingStore::new(pool.clone())?,
                SqliteBikePlusRideStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), PelotonWorkoutLandingStore::STREAM)?,
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
            let previous = resumption.read(landing.stream()).await?;
            let reader = ExtractionStatus::new(landing, resumption, runs);
            reader.reset().await?;
            Ok(Outcome::Reset { previous })
        }
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
            let SourceAccess::ApiKey { base_url, api_key } = access else {
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
                    raw: HevyWorkoutLandingReader::new(pool.clone())?,
                    translator: HevyWorkoutTranslator,
                    workouts: SqliteGymWorkoutStore::new(pool.clone())?,
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
                SqliteGymWorkoutStore::new(pool.clone())?,
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
    use super::{
        HevyWorkoutLandingStore, PelotonWorkoutLandingStore, PelotonWorkoutSampleLandingStore,
    };
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
        let wired = [
            HevyWorkoutLandingStore::STREAM,
            PelotonWorkoutLandingStore::STREAM,
            PelotonWorkoutSampleLandingStore::STREAM,
        ];

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
