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
    FileRunLock, GarminActivityFileLandingStore, GarminActivityLandingStore, GarminAuth,
    GarminCredentials, GarminExerciseSetLandingStore, GarminHrv, GarminHrvAccountReader,
    GarminHrvLandingStore, GarminHrvTranslator, HevySessionAccountReader, HevySessionTranslator,
    HevyWorkoutEvents, HevyWorkoutLandingStore, PelotonRawExtent, PelotonRideLandingStore,
    PelotonRideSampleLandingStore, PelotonSessionAccountReader, PelotonWorkoutSamples,
    PelotonWorkouts, SqliteCyclingSessionStore, SqliteExtractionRunLog, SqliteGymSessionStore,
    SqliteNormalisationRunLog, SqliteOvernightHrvStore, SqliteRefusalStore,
    SqliteResumptionPointStore, SqliteWeighInStore, TokenFile, WithingsAuth, WithingsClient,
    WithingsMeasurementLandingStore, WithingsMeasurements, WithingsWeighInAccountReader,
    WithingsWeighInTranslator, connect, garmin,
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
    /// Several walks under one entry, reported one by one, because they are
    /// separate runs against separate endpoints and a single merged number would
    /// hide one of them failing to land anything (§ 38). In the order they ran.
    ExtractedEach(Vec<RunSummary>),
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
    #[error("{stream} is reached with {wanted}, and {given} was resolved instead")]
    WrongCredential {
        stream: String,
        wanted: &'static str,
        given: &'static str,
    },
    #[error(transparent)]
    Stream(#[from] domain::landing::InvalidStream),
    #[error("{stream} lands, and nothing derives it yet")]
    NothingDerives { stream: &'static str },
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
        WithingsMeasurementLandingStore::STREAM => withings_measurements(command, database).await,
        GarminHrvLandingStore::STREAM => garmin_hrv(command, database).await,
        GarminActivityLandingStore::STREAM => garmin_activities(command, database).await,
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
            wanted: "a login",
            given: "something else",
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

    Ok(Outcome::ExtractedEach(vec![ridden, sampled]))
}

/// Garmin's overnight HRV, landed and derived into nights.
///
/// **The entity was settled against payloads that had landed** rather than
/// against a guess (#161): the first run brought back 634 nights, and what they
/// say — which figures Garmin states, which it omits, and what its status is
/// actually a classification of — is in [`domain::body::hrv`].
///
/// **No token cache**, as on Peloton and for the same reason: this adapter
/// holds a password, so it can sign in again unattended, and a login per
/// extraction is cheaper than a cache nothing else reads.
async fn garmin_hrv(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = GarminHrvLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            let SourceAccess::EmailPassword {
                base_url,
                auth_base_url,
                email,
                password,
                ..
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: GarminHrvLandingStore::STREAM.to_owned(),
                    wanted: "a login",
                    given: "something else",
                });
            };

            let auth = GarminAuth::new(
                auth_base_url.clone(),
                garmin::token_base_for(&auth_base_url),
                GarminCredentials::new(email, password),
                None,
            );
            let extraction = Extraction::new(ExtractionPorts {
                source: GarminHrv::new(base_url, auth, today()),
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
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: GarminHrvAccountReader::new(pool.clone())?,
                    translator: GarminHrvTranslator,
                    workouts: SqliteOvernightHrvStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(pool.clone(), GarminHrvLandingStore::STREAM)?,
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
                SqliteRefusalStore::new(pool.clone(), GarminHrvLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                GarminHrvLandingStore::new(pool.clone())?,
                SqliteOvernightHrvStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), GarminHrvLandingStore::STREAM)?,
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

/// Garmin's activities, their exercise sets and their files, landed and
/// nothing more.
///
/// **Every activity, because that is what Garmin serves.** The type is a field
/// on each record rather than an endpoint, so this walk takes the whole list and
/// separating gym sessions from rides is left to whatever derives them. The
/// operator settled that, 2026-09-18, on #166.
///
/// **Two walks behind one entry**, as Peloton's graphs are behind its rides: the
/// list summarises a gym session per movement, and the sets are a request per
/// activity against a different endpoint (#173). Neither is a gym session
/// without the other, so the operator cannot ask for one alone.
///
/// **And a third, for the recordings** (#175): each activity's FIT file, which
/// holds the samples the list only summarises. Every activity, on the
/// operator's word — they are his files.
///
/// **Nothing derives them yet, and that is the deliverable.** What the sets hold
/// — whether they are the sets performed, and whether one the watch guessed is
/// marked as such — is read off payloads that have landed, the way the HRV
/// entity was. So `normalise` and `refusals` refuse here rather than standing up
/// a translator against a guess.
async fn garmin_activities(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = GarminActivityLandingStore::new(pool.clone())?;
    let sets_landing = GarminExerciseSetLandingStore::new(pool.clone())?;
    let files_landing = GarminActivityFileLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            collect_activities(
                access,
                (landing, sets_landing, files_landing),
                resumption,
                runs,
                database,
            )
            .await
        }
        Command::Normalise(_) | Command::Refusals => Err(WiringError::NothingDerives {
            stream: GarminActivityLandingStore::STREAM,
        }),
        Command::Status => {
            let reader = ExtractionStatus::new(landing, resumption, runs);
            Ok(Outcome::Reported {
                extraction: Box::new(reader.status().await?),
                derivation: None,
            })
        }
        Command::Reset => {
            // All three, for the reason Peloton's two are: resetting one and
            // not the others leaves the walks at different points in history.
            let previous = resumption.read(landing.stream()).await?;
            ExtractionStatus::new(
                sets_landing,
                SqliteResumptionPointStore::new(pool.clone()),
                SqliteExtractionRunLog::new(pool.clone()),
            )
            .reset()
            .await?;
            ExtractionStatus::new(
                files_landing,
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

/// All three Garmin walks, in the one order that matters.
///
/// The list first: the other two enumerate it themselves, so running them
/// after it means what they ask for is for activities this same command has
/// just listed.
///
/// A `GarminAuth` per walk, as Peloton's two walks have one each: none holds
/// another's state, and a further sign-in costs one request.
async fn collect_activities(
    access: SourceAccess,
    (landing, sets_landing, files_landing): (
        GarminActivityLandingStore,
        GarminExerciseSetLandingStore,
        GarminActivityFileLandingStore,
    ),
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
            stream: GarminActivityLandingStore::STREAM.to_owned(),
            wanted: "a login",
            given: "something else",
        });
    };

    let credentials = GarminCredentials::new(email, password);
    let token_base = garmin::token_base_for(&auth_base_url);
    let auth = || {
        GarminAuth::new(
            auth_base_url.clone(),
            token_base.clone(),
            credentials.clone(),
            None,
        )
    };

    let listed = Extraction::new(ExtractionPorts {
        source: garmin::GarminActivities::new(base_url.clone(), auth()),
        landing,
        resumption: resumption.clone(),
        runs: runs.clone(),
        lock: FileRunLock::beside(database),
        clock: SystemClock,
    })
    .extract()
    .await?;

    let sets = Extraction::new(ExtractionPorts {
        source: garmin::GarminExerciseSets::new(base_url.clone(), auth()),
        landing: sets_landing,
        resumption: resumption.clone(),
        runs: runs.clone(),
        lock: FileRunLock::beside(database),
        clock: SystemClock,
    })
    .extract()
    .await?;

    let recordings = Extraction::new(ExtractionPorts {
        source: garmin::GarminActivityFiles::new(base_url, auth()),
        landing: files_landing,
        resumption,
        runs,
        lock: FileRunLock::beside(database),
        clock: SystemClock,
    })
    .extract()
    .await?;

    Ok(Outcome::ExtractedEach(vec![listed, sets, recordings]))
}

/// The last night worth asking Garmin about.
///
/// UTC, because a night's event time is its calendar date at midnight UTC and
/// the walk must not run past what it can place.
fn today() -> jiff::civil::Date {
    jiff::Timestamp::now()
        .to_zoned(jiff::tz::TimeZone::UTC)
        .date()
}

/// Withings' measure groups, landed and derived into Body Scan weigh-ins.
async fn withings_measurements(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = WithingsMeasurementLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::Extract(access) => {
            let SourceAccess::OAuthClient {
                base_url,
                auth_base_url,
                client,
                token,
                ..
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: WithingsMeasurementLandingStore::STREAM.to_owned(),
                    wanted: "an application",
                    given: "something else",
                });
            };

            let auth = WithingsAuth::new(
                base_url.clone(),
                auth_base_url,
                WithingsClient::new(client.client_id, client.client_secret, client.redirect_uri),
                TokenFile::new(token),
            );
            let extraction = Extraction::new(ExtractionPorts {
                source: WithingsMeasurements::new(base_url, auth),
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
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: WithingsWeighInAccountReader::new(pool.clone())?,
                    translator: WithingsWeighInTranslator,
                    workouts: SqliteWeighInStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        WithingsMeasurementLandingStore::STREAM,
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
                SqliteRefusalStore::new(pool.clone(), WithingsMeasurementLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                WithingsMeasurementLandingStore::new(pool.clone())?,
                SqliteWeighInStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), WithingsMeasurementLandingStore::STREAM)?,
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
            let SourceAccess::ApiKey {
                base_url, api_key, ..
            } = access
            else {
                return Err(WiringError::WrongCredential {
                    stream: HevyWorkoutLandingStore::STREAM.to_owned(),
                    wanted: "a key",
                    given: "something else",
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
    use super::{
        GarminActivityFileLandingStore, GarminActivityLandingStore, GarminExerciseSetLandingStore,
        GarminHrvLandingStore, HevyWorkoutLandingStore, PelotonRideLandingStore,
        PelotonRideSampleLandingStore, WithingsMeasurementLandingStore,
    };
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
            WithingsMeasurementLandingStore::STREAM,
            GarminHrvLandingStore::STREAM,
            GarminActivityLandingStore::STREAM,
            GarminExerciseSetLandingStore::STREAM,
            GarminActivityFileLandingStore::STREAM,
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
        // collects its rides, and Garmin's exercise sets and files by the one
        // that collects its activities, because neither derives anything without the
        // other — so these are wired, land, resume and lock, and cannot be
        // asked for. Anything else wired but uncollectable is a mistake.
        let landed_behind_another = [
            PelotonRideSampleLandingStore::STREAM,
            GarminExerciseSetLandingStore::STREAM,
            GarminActivityFileLandingStore::STREAM,
        ];
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
