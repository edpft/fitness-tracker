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

use std::path::{Path, PathBuf};

use application::{
    DerivationStatus, DerivationStatusReporter, DerivedFrom, ExtractionError,
    ExtractionStatusReporter, LandingStore, NormalisationError, NormalisationSummary,
    RefusalReport, RefusalReporter, ResumptionPointResetter, ResumptionPointStore, RunSummary,
    StatusError, StreamStatus, WorkoutEventSource, WorkoutExtractor, WorkoutNormaliser,
    extract::{Extraction, ExtractionPorts},
    next::{Daily, NoAccess, Records},
    normalise::{DerivationStanding, Normalisation, NormalisationPorts, Refusals},
    status::ExtractionStatus,
};
use domain::{
    landing::{FetchedAt, LandingStream, Watermark},
    normalised::OperatorZone,
};
use infrastructure::{
    BtwbExportAccountReader, BtwbExportLandingStore, BtwbTranslator, FileRunLock, FolderFiles,
    GarminActivityFileLandingStore, GarminActivityLandingStore, GarminAuth,
    GarminBodyBatteryEventLandingStore, GarminBodyBatteryLandingStore, GarminCredentials,
    GarminDailyHeartRateAccountReader, GarminDailyHeartRateLandingStore,
    GarminDailySummaryLandingStore, GarminDetectedMoveLandingStore, GarminExerciseSetLandingStore,
    GarminFitnessAgeLandingStore, GarminFloorsLandingStore, GarminGymAccountReader,
    GarminGymRawExtent, GarminGymTranslator, GarminHealthSnapshotLandingStore, GarminHrv,
    GarminHrvAccountReader, GarminHrvLandingStore, GarminHrvTranslator,
    GarminIntensityMinutesLandingStore, GarminPersonalRecordLandingStore,
    GarminRacePredictionLandingStore, GarminRespirationLandingStore,
    GarminRestingHeartRateTranslator, GarminSleepLandingStore, GarminSpo2LandingStore,
    GarminStepsLandingStore, GarminStressLandingStore, GarminVo2MaxLandingStore,
    HevySessionAccountReader, HevySessionTranslator, HevyWorkoutEvents, HevyWorkoutLandingStore,
    MyFitnessPalExportAccountReader, MyFitnessPalExportLandingStore, MyFitnessPalWeighInTranslator,
    PelotonAccountLandingStore, PelotonRawExtent, PelotonRideDetailLandingStore,
    PelotonRideLandingStore, PelotonRideSampleLandingStore, PelotonSessionAccountReader,
    PelotonWorkoutSamples, PelotonWorkouts, SpreadsheetFileAccountReader,
    SpreadsheetFileLandingStore, SpreadsheetTranslator, SqliteBtwbStore, SqliteCyclingSessionStore,
    SqliteEditOverlayStore, SqliteExtractionRunLog, SqliteGymSessionStore,
    SqliteMeasuredGymSessionStore, SqliteMyFitnessPalStore, SqliteNormalisationRunLog,
    SqliteOvernightHrvStore, SqlitePool, SqliteRefusalStore, SqliteRestingHeartRateStore,
    SqliteResumptionPointStore, SqliteSpreadsheetStore, SqliteWeighInStore, TokenFile,
    WithingsAuth, WithingsClient, WithingsEcgLandingStore, WithingsEcgSignalLandingStore,
    WithingsEcgSignals, WithingsEcgs, WithingsMeasurementLandingStore, WithingsMeasurements,
    WithingsWeighInAccountReader, WithingsWeighInTranslator, connect, garmin,
    garmin::{days::GarminDay, latest::GarminLatest, months::GarminMonth},
    peloton::{
        PelotonSessionTranslator, PelotonWorkoutDetails,
        auth::{PelotonAuth, PelotonCredentials},
        paired::PelotonAccount,
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
    /// Land every file in a folder the operator named. What `extract` means for
    /// a stream served by a folder rather than a system.
    ExtractFolder(PathBuf),
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
    ///
    /// **A walk that failed is one of the entries, not the absence of them**
    /// (#367). This carried `RunSummary`, so the only way to report a failed
    /// walk was to propagate its error — which discarded the walks that had
    /// already run, and left the caller printing the catalogue entry's own name
    /// over a failure belonging to a walk behind it. On 2026-10-05 that told the
    /// operator `peloton.rides` was not collected, when `peloton.rides` had
    /// landed fifty rides and `peloton.ride_samples` was the walk refused.
    ExtractedEach(Vec<Walk>),
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

/// One of several walks behind a single catalogue entry.
///
/// The stream is the walk's own, read off its landing store before the ports are
/// assembled — the one thing that can answer which stream a run is extracting
/// without being told ([`LandingStore::stream`]). The entry's name is the wrong
/// answer here: `peloton.rides` names two walks and is the stream of only one.
pub struct Walk {
    pub stream: LandingStream,
    pub result: Result<RunSummary, ExtractionError>,
}

/// Run one walk and label it with the stream it ran against.
///
/// **The failure is carried, not raised.** A `?` here is how #367 happened: it
/// is the one place where a walk's error can swallow the walks either side of
/// it, and the only way to keep that from coming back is for a walk's outcome to
/// have nowhere else to go.
async fn walk(
    stream: LandingStream,
    extraction: impl WorkoutExtractor,
    walks: &dyn WalkReporter,
) -> Walk {
    walks.started(&stream);
    let walk = Walk {
        stream,
        result: extraction.extract().await,
    };
    walks.finished(&walk);
    walk
}

/// Told as each walk behind an entry starts and as it finishes (#437).
///
/// **So a long collection says what it is doing while it does it.** The walks
/// behind one entry run in turn, and their outcomes used to reach the operator
/// only once every one had finished: on 2026-10-14 `extract garmin.activities`
/// had been silent for sixteen minutes, alive and waiting on Garmin, with
/// nothing on screen to say so. A frontend implements this to show each walk as
/// it starts and report it as soon as it ends.
///
/// The outcome still comes back in [`Outcome::ExtractedEach`], whole, so what a
/// part-collected entry costs remains the caller's to decide.
pub trait WalkReporter: Sync {
    fn started(&self, stream: &LandingStream);
    fn finished(&self, walk: &Walk);
}

/// Reports nothing: for a caller with no one to tell.
pub struct Unreported;

impl WalkReporter for Unreported {
    fn started(&self, _stream: &LandingStream) {}
    fn finished(&self, _walk: &Walk) {}
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
    #[error("{stream} is collected from its source, not a folder")]
    NotAFolder { stream: &'static str },
    #[error(transparent)]
    Overlay(#[from] infrastructure::OverlayError),
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
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    match known.name().as_str() {
        HevyWorkoutLandingStore::STREAM => hevy_workouts(command, database).await,
        PelotonRideLandingStore::STREAM => peloton_rides(command, database, walks).await,
        WithingsMeasurementLandingStore::STREAM => {
            withings_measurements(command, database, walks).await
        }
        GarminHrvLandingStore::STREAM => garmin_hrv(command, database).await,
        GarminActivityLandingStore::STREAM => garmin_activities(command, database, walks).await,
        GarminDailyHeartRateLandingStore::STREAM => {
            garmin_daily_heart_rate(command, database).await
        }
        SpreadsheetFileLandingStore::STREAM => spreadsheet_files(command, database).await,
        BtwbExportLandingStore::STREAM => btwb_exports(command, database).await,
        MyFitnessPalExportLandingStore::STREAM => myfitnesspal_exports(command, database).await,
        other => Err(WiringError::Unwired {
            stream: other.to_owned(),
        }),
    }
}

/// What `known`'s normalised layer would be derived from now (#468): how many
/// landing records its derivation reads, the corrections it applies, and the
/// zone.
///
/// **A count is the whole of raw's extent**, because raw is append-only
/// (§ II.1): a record is never removed or rewritten, so the same count is the
/// same records. Corrections are not, so they are named, and a correction
/// retracted and another asserted reads differently. The landing tables and
/// the overlay are the ones the stream's arm of [`run`] derives from.
///
/// # Errors
///
/// [`WiringError`] if the store is unavailable, or the stream has no
/// derivation here.
pub async fn inputs(
    known: &KnownStream,
    database: &Path,
    zone: &OperatorZone,
) -> Result<DerivedFrom, WiringError> {
    use application::RawExtent as _;

    let pool = connect(database).await?;
    let (records, overlay) = match known.name().as_str() {
        HevyWorkoutLandingStore::STREAM => (
            HevyWorkoutLandingStore::new(pool.clone())?
                .records()
                .await?,
            Some(HevyWorkoutLandingStore::STREAM),
        ),
        PelotonRideLandingStore::STREAM => (
            PelotonRawExtent::new(
                PelotonRideLandingStore::new(pool.clone())?,
                PelotonRideSampleLandingStore::new(pool.clone())?,
            )
            .records()
            .await?,
            None,
        ),
        WithingsMeasurementLandingStore::STREAM => (
            WithingsMeasurementLandingStore::new(pool.clone())?
                .records()
                .await?,
            None,
        ),
        GarminHrvLandingStore::STREAM => (
            GarminHrvLandingStore::new(pool.clone())?.records().await?,
            None,
        ),
        GarminActivityLandingStore::STREAM => (
            GarminGymRawExtent::new(
                GarminActivityLandingStore::new(pool.clone())?,
                GarminExerciseSetLandingStore::new(pool.clone())?,
                GarminActivityFileLandingStore::new(pool.clone())?,
            )
            .records()
            .await?,
            Some(GarminExerciseSetLandingStore::STREAM),
        ),
        GarminDailyHeartRateLandingStore::STREAM => (
            GarminDailyHeartRateLandingStore::new(pool.clone())?
                .records()
                .await?,
            None,
        ),
        SpreadsheetFileLandingStore::STREAM => (
            SpreadsheetFileLandingStore::new(pool.clone())?
                .records()
                .await?,
            None,
        ),
        BtwbExportLandingStore::STREAM => (
            BtwbExportLandingStore::new(pool.clone())?.records().await?,
            None,
        ),
        MyFitnessPalExportLandingStore::STREAM => (
            MyFitnessPalExportLandingStore::new(pool.clone())?
                .records()
                .await?,
            None,
        ),
        other => {
            return Err(WiringError::Unwired {
                stream: other.to_owned(),
            });
        }
    };
    let corrections = match overlay {
        None => Vec::new(),
        Some(stream) => SqliteEditOverlayStore::new(pool, stream)?
            .all()
            .await?
            .iter()
            .map(|correction| correction.id().to_string())
            .collect(),
    };
    Ok(DerivedFrom::from(format!(
        "records {records}; corrections {}; zone {}",
        corrections.join(","),
        zone.id()
    )))
}

/// Garmin's daily heart rate, of which the resting heart rate is derived (#254).
///
/// **Collected two ways**: with the activities, because it says which days a
/// watch was worn (#427), and here on its own, signing in for itself as the HRV
/// walk does. Both walks keep one resumption point, so whichever runs second
/// asks only for the day the first left off at.
async fn garmin_daily_heart_rate(
    command: Command,
    database: &Path,
) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = GarminDailyHeartRateLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let stream = GarminDailyHeartRateLandingStore::STREAM;

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder { stream }),
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
                    stream: stream.to_owned(),
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
                source: garmin::days::GarminDays::new(
                    GarminDay::HeartRate,
                    base_url,
                    auth,
                    today(),
                ),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });
            Ok(Outcome::Extracted(Box::new(extraction.extract().await?)))
        }
        Command::Normalise(zone) => {
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: GarminDailyHeartRateAccountReader::new(pool.clone())?,
                    translator: GarminRestingHeartRateTranslator,
                    workouts: SqliteRestingHeartRateStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(pool.clone(), stream)?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );
            Ok(Outcome::Derived(Box::new(normalisation.normalise().await?)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone(), stream)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                GarminDailyHeartRateLandingStore::new(pool.clone())?,
                SqliteRestingHeartRateStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), stream)?,
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
async fn peloton_rides(
    command: Command,
    database: &Path,
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = PelotonRideLandingStore::new(pool.clone())?;
    let samples_landing = PelotonRideSampleLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder {
            stream: PelotonRideLandingStore::STREAM,
        }),
        Command::Extract(access) => {
            collect_rides(
                access,
                PelotonLandings {
                    rides: landing,
                    samples: samples_landing,
                    details: PelotonRideDetailLandingStore::new(pool.clone())?,
                    account: PelotonAccountLandingStore::new(pool.clone())?,
                },
                resumption,
                runs,
                database,
                walks,
            )
            .await
        }
        Command::Normalise(zone) => {
            let summary = peloton_normalisation(pool, zone)?.normalise().await?;
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
            // Every walk that resumes, because all are behind one entry:
            // resetting one and not the others would leave them at different
            // points in history, which is the state this entry exists to
            // prevent. The account has no resumption point.
            let previous = resumption.read(landing.stream()).await?;
            ExtractionStatus::new(
                samples_landing,
                SqliteResumptionPointStore::new(pool.clone()),
                SqliteExtractionRunLog::new(pool.clone()),
            )
            .reset()
            .await?;
            ExtractionStatus::new(
                PelotonRideDetailLandingStore::new(pool.clone())?,
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

/// The one way a resolved Peloton login becomes an authenticator.
///
/// **Whether the token is cached is not a per-call-site decision** (#366). It
/// was: `PelotonAuth::caching_in` was called in `plan` and nowhere else, so
/// every `fitness next` that collected cycling performed two complete Auth0
/// password logins seconds apart, and extraction neither read nor wrote the
/// token file. The second of the two was refused on 2026-10-05 and the graph
/// walk landed nothing, while the catalogue refresh — which went through `plan`
/// — succeeded on a cached token in the same invocation.
///
/// **A machine with neither `XDG_STATE_HOME` nor `HOME` gets no cache** rather
/// than an error: logging in again costs a few seconds, and refusing to run
/// costs the session.
pub fn peloton_auth(auth_base_url: String, credentials: PelotonCredentials) -> PelotonAuth {
    let cache = crate::paths::token(&crate::paths::SystemEnvironment, crate::catalogue::PELOTON)
        .ok()
        .map(|path| TokenFile::new(path, credentials.digest()));
    PelotonAuth::new(auth_base_url, credentials, cache)
}

/// Both Peloton walks, in the one order that matters.
///
/// The list is walked first: the graph walk enumerates rides itself, so running
/// it second means the graphs it fetches are for rides this same command has
/// just landed.
///
/// Two `PelotonAuth`s rather than one shared, because each reads the token the
/// other cached — so the second walk costs no login and neither holds the
/// other's state.
///
/// **The second walk runs whatever the first did.** The graph walk enumerates
/// rides from the source rather than from the list walk's table, so it has no
/// premise the first walk could break — and a walk skipped because of another's
/// failure is a walk with no outcome to report, which is the shape #367 was.
async fn collect_rides(
    access: SourceAccess,
    landings: PelotonLandings,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    database: &Path,
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    let (((rides, graphs), details), account) =
        peloton_walks(access, landings, resumption, runs, database)?;
    let ridden = walk(rides.stream, rides.extraction, walks).await;
    let sampled = walk(graphs.stream, graphs.extraction, walks).await;
    let detailed = walk(details.stream, details.extraction, walks).await;
    let paired = walk(account.stream, account.extraction, walks).await;
    Ok(Outcome::ExtractedEach(vec![
        ridden, sampled, detailed, paired,
    ]))
}

/// Peloton's landing tables: the rides, their graphs and their detail, the
/// and the account that says what the bike is paired with (#427).
struct PelotonLandings {
    rides: PelotonRideLandingStore,
    samples: PelotonRideSampleLandingStore,
    details: PelotonRideDetailLandingStore,
    account: PelotonAccountLandingStore,
}

/// The walks over the rides, each resuming, and the account, which does not.
type PelotonWalks<A, B, C, D> = (
    (
        (application::next::Walk<A>, application::next::Walk<B>),
        application::next::Walk<C>,
    ),
    application::next::Walk<D>,
);

/// Peloton's walks, the list first: the graph and detail walks enumerate
/// rides themselves, so running them after it means what they fetch is for
/// rides this same run has just landed. The account depends on none of them.
fn peloton_walks(
    access: SourceAccess,
    landings: PelotonLandings,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    database: &Path,
) -> Result<
    PelotonWalks<
        impl WorkoutExtractor + Sync + use<>,
        impl WorkoutExtractor + Sync + use<>,
        impl WorkoutExtractor + Sync + use<>,
        impl WorkoutExtractor + Sync + use<>,
    >,
    WiringError,
> {
    let PelotonLandings {
        rides: landing,
        samples: samples_landing,
        details: details_landing,
        account: account_landing,
    } = landings;
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
    let rides = application::next::Walk {
        stream: landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: PelotonWorkouts::new(
                base_url.clone(),
                peloton_auth(auth_base_url.clone(), credentials.clone()),
            ),
            landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    };
    let graphs = application::next::Walk {
        stream: samples_landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: PelotonWorkoutSamples::new(
                base_url.clone(),
                peloton_auth(auth_base_url.clone(), credentials.clone()),
            ),
            landing: samples_landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    };
    let details = application::next::Walk {
        stream: details_landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: PelotonWorkoutDetails::new(
                base_url.clone(),
                peloton_auth(auth_base_url.clone(), credentials.clone()),
            ),
            landing: details_landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    };
    let account = application::next::Walk {
        stream: account_landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: PelotonAccount::new(base_url, peloton_auth(auth_base_url, credentials)),
            landing: account_landing,
            resumption,
            runs,
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    };
    Ok((((rides, graphs), details), account))
}

/// Peloton's sessions, derived from both walks.
///
/// No lock, as on the gym side: a derivation reads raw and writes only its own
/// tables.
fn peloton_normalisation(
    pool: SqlitePool,
    zone: OperatorZone,
) -> Result<impl WorkoutNormaliser + Sync + use<>, WiringError> {
    Ok(Normalisation::new(
        NormalisationPorts {
            raw: PelotonSessionAccountReader::new(pool.clone())?,
            translator: PelotonSessionTranslator,
            workouts: SqliteCyclingSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), PelotonRideLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool),
            clock: SystemClock,
        },
        zone,
    ))
}

/// Peloton's rides as `next` collects them: every walk behind the entry.
///
/// # Errors
///
/// [`WiringError`] if the store is unavailable. A login that is absent or of
/// the wrong kind is [`NoAccess`], which `next` reports and steps past.
pub async fn peloton_records(
    access: Result<SourceAccess, NoAccess>,
    database: &Path,
) -> Result<
    Records<
        PelotonWalks<
            impl WorkoutExtractor + Sync + use<>,
            impl WorkoutExtractor + Sync + use<>,
            impl WorkoutExtractor + Sync + use<>,
            impl WorkoutExtractor + Sync + use<>,
        >,
    >,
    WiringError,
> {
    let pool = connect(database).await?;
    let entry = LandingStream::try_from(PelotonRideLandingStore::STREAM)?;
    let walks = match access {
        Err(why) => Err(why),
        Ok(access) => accessible(peloton_walks(
            access,
            PelotonLandings {
                rides: PelotonRideLandingStore::new(pool.clone())?,
                samples: PelotonRideSampleLandingStore::new(pool.clone())?,
                details: PelotonRideDetailLandingStore::new(pool.clone())?,
                account: PelotonAccountLandingStore::new(pool.clone())?,
            },
            SqliteResumptionPointStore::new(pool.clone()),
            SqliteExtractionRunLog::new(pool),
            database,
        ))?,
    };
    Ok(Records { entry, walks })
}

/// Garmin's overnight HRV, landed and derived into nights.
///
/// **The entity was settled against payloads that had landed** rather than
/// against a guess (#161): the first run brought back 634 nights, and what they
/// say — which figures Garmin states, which it omits, and what its status is
/// actually a classification of — is in [`domain::body::hrv`].
///
/// **No token cache**, and this walk signs in for itself: the adapter holds a
/// password, so it can do that unattended, and Garmin's other three walks share
/// one `Arc<GarminAuth>` rather than a file. Peloton's cache is kept because
/// its two walks and a cycling delivery cannot share an instance (#366).
async fn garmin_hrv(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = GarminHrvLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder {
            stream: GarminHrvLandingStore::STREAM,
        }),
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
async fn garmin_activities(
    command: Command,
    database: &Path,
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = GarminActivityLandingStore::new(pool.clone())?;
    let sets_landing = GarminExerciseSetLandingStore::new(pool.clone())?;
    let files_landing = GarminActivityFileLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder {
            stream: GarminActivityLandingStore::STREAM,
        }),
        Command::Extract(access) => {
            let daily_landing = GarminDailyHeartRateLandingStore::new(pool.clone())?;
            collect_activities(
                access,
                (landing, sets_landing, files_landing, daily_landing),
                resumption,
                runs,
                (pool.clone(), database),
                walks,
            )
            .await
        }
        Command::Normalise(zone) => {
            // The overlay is read here, as it is for Hevy and for the same
            // reason: § II.2 applies a correction while the layer is rebuilt,
            // and this is that moment. It is keyed on `garmin.exercise_sets`
            // because a set is what the corrected figures belong to, and that
            // is the record whose content is wrong.
            let overlay =
                SqliteEditOverlayStore::new(pool.clone(), GarminExerciseSetLandingStore::STREAM)?
                    .overlay()
                    .await?;

            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: GarminGymAccountReader::new(pool.clone())?,
                    translator: GarminGymTranslator::correcting(overlay),
                    workouts: SqliteMeasuredGymSessionStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        GarminActivityLandingStore::STREAM,
                    )?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );
            Ok(Outcome::Derived(Box::new(normalisation.normalise().await?)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone(), GarminActivityLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                GarminGymRawExtent::new(
                    GarminActivityLandingStore::new(pool.clone())?,
                    GarminExerciseSetLandingStore::new(pool.clone())?,
                    GarminActivityFileLandingStore::new(pool.clone())?,
                ),
                SqliteMeasuredGymSessionStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), GarminActivityLandingStore::STREAM)?,
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
/// **One `GarminAuth`, shared**, unlike Peloton's two: Garmin rate-limits its
/// sign-in page, and a run that signed in once per walk was refused by it on
/// the operator's account. The token is held behind a lock, so the walks share
/// it without sharing anything else.
///
/// **Each walk reports its own outcome**, as Peloton's two do (#367): the three
/// all run, and one that could not is named rather than standing in for the
/// entry.
async fn collect_activities(
    access: SourceAccess,
    (landing, sets_landing, files_landing, daily_landing): (
        GarminActivityLandingStore,
        GarminExerciseSetLandingStore,
        GarminActivityFileLandingStore,
        GarminDailyHeartRateLandingStore,
    ),
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    (pool, database): (SqlitePool, &Path),
    walks: &dyn WalkReporter,
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
    let auth = std::sync::Arc::new(GarminAuth::new(
        auth_base_url.clone(),
        token_base,
        credentials,
        None,
    ));

    let activities_stream = landing.stream().clone();
    let listed = walk(
        activities_stream,
        Extraction::new(ExtractionPorts {
            source: garmin::GarminActivities::new(base_url.clone(), std::sync::Arc::clone(&auth)),
            landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    let sets_stream = sets_landing.stream().clone();
    let sets = walk(
        sets_stream,
        Extraction::new(ExtractionPorts {
            source: garmin::GarminExerciseSets::new(base_url.clone(), std::sync::Arc::clone(&auth)),
            landing: sets_landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    let files_stream = files_landing.stream().clone();
    let recordings = walk(
        files_stream,
        Extraction::new(ExtractionPorts {
            source: garmin::GarminActivityFiles::new(
                base_url.clone(),
                std::sync::Arc::clone(&auth),
            ),
            landing: files_landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    let days = Days {
        base_url: &base_url,
        auth: &auth,
        resumption: &resumption,
        runs: &runs,
        database,
        walks,
    };
    // The wrist's all-day heart rate, which says which days a watch was worn
    // and, for about four months, what it read during a ride (#427).
    let mut each = vec![
        listed,
        sets,
        recordings,
        days.walk(GarminDay::HeartRate, daily_landing).await,
    ];
    each.extend(days.every(pool).await?);

    Ok(Outcome::ExtractedEach(each))
}

/// What every one of Garmin's metric walks shares: the account, the books and
/// the reporter.
struct Days<'a> {
    base_url: &'a str,
    auth: &'a std::sync::Arc<GarminAuth>,
    resumption: &'a SqliteResumptionPointStore,
    runs: &'a SqliteExtractionRunLog,
    database: &'a Path,
    walks: &'a dyn WalkReporter,
}

impl Days<'_> {
    /// **Everything else the watch records** (#269), each its own stream: a
    /// day at a time, a month at a time, or as it stands. Nothing derives them yet, so they land behind the entry
    /// that already walks Garmin day by day, as daily heart rate did until
    /// #474 gave it something to derive.
    async fn every(&self, pool: SqlitePool) -> Result<Vec<Walk>, WiringError> {
        Ok(vec![
            self.walk(
                GarminDay::Sleep,
                GarminSleepLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::Stress,
                GarminStressLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::BodyBattery,
                GarminBodyBatteryLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::BodyBatteryEvents,
                GarminBodyBatteryEventLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::Respiration,
                GarminRespirationLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(GarminDay::Spo2, GarminSpo2LandingStore::new(pool.clone())?)
                .await,
            self.walk(
                GarminDay::IntensityMinutes,
                GarminIntensityMinutesLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::Floors,
                GarminFloorsLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::DailySummary,
                GarminDailySummaryLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::Steps,
                GarminStepsLandingStore::new(pool.clone())?,
            )
            .await,
            self.walk(
                GarminDay::DetectedMoves,
                GarminDetectedMoveLandingStore::new(pool.clone())?,
            )
            .await,
            self.month(
                GarminMonth::Vo2Max,
                GarminVo2MaxLandingStore::new(pool.clone())?,
            )
            .await,
            self.month(
                GarminMonth::HealthSnapshots,
                GarminHealthSnapshotLandingStore::new(pool.clone())?,
            )
            .await,
            self.latest(
                GarminLatest::FitnessAge,
                GarminFitnessAgeLandingStore::new(pool.clone())?,
            )
            .await,
            self.latest(
                GarminLatest::RacePredictions,
                GarminRacePredictionLandingStore::new(pool.clone())?,
            )
            .await,
            self.latest(
                GarminLatest::PersonalRecords,
                GarminPersonalRecordLandingStore::new(pool)?,
            )
            .await,
        ])
    }

    /// One daily metric, walked into its own table.
    async fn walk(&self, day: GarminDay, landing: impl LandingStore + Send + Sync) -> Walk {
        self.into(
            garmin::days::GarminDays::new(
                day,
                self.base_url,
                std::sync::Arc::clone(self.auth),
                today(),
            ),
            landing,
        )
        .await
    }

    /// One metric served over a span of dates, a month to a record.
    async fn month(&self, month: GarminMonth, landing: impl LandingStore + Send + Sync) -> Walk {
        self.into(
            garmin::months::GarminMonths::new(
                month,
                self.base_url,
                std::sync::Arc::clone(self.auth),
                today(),
            ),
            landing,
        )
        .await
    }

    /// One metric served only as it stands now.
    async fn latest(&self, latest: GarminLatest, landing: impl LandingStore + Send + Sync) -> Walk {
        self.into(
            garmin::latest::GarminLatestOf::new(
                latest,
                self.base_url,
                std::sync::Arc::clone(self.auth),
                today(),
            ),
            landing,
        )
        .await
    }

    async fn into<S>(&self, source: S, landing: impl LandingStore + Send + Sync) -> Walk
    where
        S: WorkoutEventSource + Send + Sync,
        S::Resume: Send + Sync,
    {
        walk(
            landing.stream().clone(),
            Extraction::new(ExtractionPorts {
                source,
                landing,
                resumption: self.resumption.clone(),
                runs: self.runs.clone(),
                lock: FileRunLock::beside(self.database),
                clock: SystemClock,
            }),
            self.walks,
        )
        .await
    }
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

/// Withings' measure groups, landed and derived into Body Scan weigh-ins, and
/// the Body Scan's ECGs landed beside them (#270).
async fn withings_measurements(
    command: Command,
    database: &Path,
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = WithingsMeasurementLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder {
            stream: WithingsMeasurementLandingStore::STREAM,
        }),
        Command::Extract(access) => {
            collect_withings(access, landing, resumption, runs, (pool, database), walks).await
        }
        Command::Normalise(zone) => {
            let summary = withings_normalisation(pool, zone)?.normalise().await?;
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
            // The ECGs too, for the reason Garmin's walks give: resetting one
            // and not the others leaves them at different points in history.
            let previous = resumption.read(landing.stream()).await?;
            ExtractionStatus::new(
                WithingsEcgLandingStore::new(pool.clone())?,
                SqliteResumptionPointStore::new(pool.clone()),
                SqliteExtractionRunLog::new(pool.clone()),
            )
            .reset()
            .await?;
            ExtractionStatus::new(
                WithingsEcgSignalLandingStore::new(pool.clone())?,
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

/// Every Withings walk, in the one order that matters: the measurements, then
/// the ECG list, then the signals the list names.
///
/// **One `WithingsAuth`, shared.** The refresh token rotates on every use, so
/// three walks each refreshing for itself would each spend the one before's.
///
/// **Nothing derives the ECGs yet**, so they land behind the entry that already
/// walks Withings, as Garmin's metrics do behind its activities (#269).
async fn collect_withings(
    access: SourceAccess,
    landing: WithingsMeasurementLandingStore,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    (pool, database): (SqlitePool, &Path),
    walks: &dyn WalkReporter,
) -> Result<Outcome, WiringError> {
    let (base_url, auth) = withings_auth(access)?;

    let measured = walk(
        landing.stream().clone(),
        Extraction::new(ExtractionPorts {
            source: WithingsMeasurements::new(base_url.clone(), std::sync::Arc::clone(&auth)),
            landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    let ecgs_landing = WithingsEcgLandingStore::new(pool.clone())?;
    let ecgs = walk(
        ecgs_landing.stream().clone(),
        Extraction::new(ExtractionPorts {
            source: WithingsEcgs::new(base_url.clone(), std::sync::Arc::clone(&auth)),
            landing: ecgs_landing,
            resumption: resumption.clone(),
            runs: runs.clone(),
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    let signals_landing = WithingsEcgSignalLandingStore::new(pool)?;
    let signals = walk(
        signals_landing.stream().clone(),
        Extraction::new(ExtractionPorts {
            source: WithingsEcgSignals::new(base_url, auth),
            landing: signals_landing,
            resumption,
            runs,
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
        walks,
    )
    .await;

    Ok(Outcome::ExtractedEach(vec![measured, ecgs, signals]))
}

/// Hevy's workout events, as one walk.
fn hevy_walk(
    access: SourceAccess,
    landing: HevyWorkoutLandingStore,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    database: &Path,
) -> Result<application::next::Walk<impl WorkoutExtractor + Sync + use<>>, WiringError> {
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
    Ok(application::next::Walk {
        stream: landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: HevyWorkoutEvents::new(base_url, api_key),
            landing,
            resumption,
            runs,
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    })
}

/// Hevy's sessions, derived from what raw holds.
///
/// No lock. A derivation reads raw and writes only its own tables, so it
/// neither takes the extraction lock nor advances the resumption point — the
/// two commands can run at once.
///
/// The overlay is read here, as the zone is, and handed to the translator
/// already resolved: § II.2 applies a correction while the layer is rebuilt,
/// and this is that moment. A store that holds none yields an empty overlay and
/// the derivation is the one it always was.
async fn hevy_normalisation(
    pool: SqlitePool,
    zone: OperatorZone,
) -> Result<impl WorkoutNormaliser + Sync + use<>, WiringError> {
    let overlay = SqliteEditOverlayStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?
        .overlay()
        .await?;
    Ok(Normalisation::new(
        NormalisationPorts {
            raw: HevySessionAccountReader::new(pool.clone())?,
            translator: HevySessionTranslator::correcting(overlay),
            workouts: SqliteGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool),
            clock: SystemClock,
        },
        zone,
    ))
}

/// The API root and a sign-in for it, from the application's own credential.
fn withings_auth(
    access: SourceAccess,
) -> Result<(String, std::sync::Arc<WithingsAuth>), WiringError> {
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

    let registration =
        WithingsClient::new(client.client_id, client.client_secret, client.redirect_uri);
    let auth = WithingsAuth::new(
        base_url.clone(),
        auth_base_url,
        registration.clone(),
        TokenFile::new(token, registration.digest()),
    );
    Ok((base_url, std::sync::Arc::new(auth)))
}

/// Withings' measure groups, walked with the application's own credential.
fn withings_walk(
    access: SourceAccess,
    landing: WithingsMeasurementLandingStore,
    resumption: SqliteResumptionPointStore,
    runs: SqliteExtractionRunLog,
    database: &Path,
) -> Result<application::next::Walk<impl WorkoutExtractor + Sync + use<>>, WiringError> {
    let (base_url, auth) = withings_auth(access)?;
    Ok(application::next::Walk {
        stream: landing.stream().clone(),
        extraction: Extraction::new(ExtractionPorts {
            source: WithingsMeasurements::new(base_url, auth),
            landing,
            resumption,
            runs,
            lock: FileRunLock::beside(database),
            clock: SystemClock,
        }),
    })
}

/// Body Scan weigh-ins, derived from what raw holds.
fn withings_normalisation(
    pool: SqlitePool,
    zone: OperatorZone,
) -> Result<impl WorkoutNormaliser + Sync + use<>, WiringError> {
    Ok(Normalisation::new(
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
    ))
}

/// Withings' weigh-ins as `next` collects them (#409, #357): **once a day**.
///
/// The operator, 2026-10-16: *"Once a daily Withings weigh in has been
/// collected, we don't need to collect another one until the next day."*
///
/// # Errors
///
/// [`WiringError`] if the store is unavailable. A credential that is absent or
/// of the wrong kind is [`NoAccess`], which `next` reports and steps past.
pub async fn withings_records(
    access: Result<SourceAccess, NoAccess>,
    database: &Path,
) -> Result<
    Records<
        Daily<application::next::Walk<impl WorkoutExtractor + Sync + use<>>, SqliteWeighInStore>,
    >,
    WiringError,
> {
    let pool = connect(database).await?;
    let entry = LandingStream::try_from(WithingsMeasurementLandingStore::STREAM)?;
    let newest = SqliteWeighInStore::new(pool.clone())?;
    let walks = match access {
        Err(why) => Err(why),
        Ok(access) => accessible(withings_walk(
            access,
            WithingsMeasurementLandingStore::new(pool.clone())?,
            SqliteResumptionPointStore::new(pool.clone()),
            SqliteExtractionRunLog::new(pool),
            database,
        ))?
        .map(|walks| Daily { walks, newest }),
    };
    Ok(Records { entry, walks })
}

/// Hevy's workouts as `next` collects them: one walk.
///
/// # Errors
///
/// [`WiringError`] if the store is unavailable. A key that is absent or of the
/// wrong kind is [`NoAccess`], which `next` reports and steps past.
pub async fn hevy_records(
    access: Result<SourceAccess, NoAccess>,
    database: &Path,
) -> Result<Records<application::next::Walk<impl WorkoutExtractor + Sync + use<>>>, WiringError> {
    let pool = connect(database).await?;
    let entry = LandingStream::try_from(HevyWorkoutLandingStore::STREAM)?;
    let walks = match access {
        Err(why) => Err(why),
        Ok(access) => accessible(hevy_walk(
            access,
            HevyWorkoutLandingStore::new(pool.clone())?,
            SqliteResumptionPointStore::new(pool.clone()),
            SqliteExtractionRunLog::new(pool),
            database,
        ))?,
    };
    Ok(Records { entry, walks })
}

/// A credential of the wrong kind is no access, like an absent one; anything
/// else is a failure to build the walks at all.
fn accessible<W>(built: Result<W, WiringError>) -> Result<Result<W, NoAccess>, WiringError> {
    match built {
        Ok(walks) => Ok(Ok(walks)),
        Err(error @ WiringError::WrongCredential { .. }) => Ok(Err(NoAccess {
            detail: error.to_string(),
        })),
        Err(other) => Err(other),
    }
}

/// Hevy's workout events feed, landed into the table shaped for it.
async fn hevy_workouts(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(_) => Err(WiringError::NotAFolder {
            stream: HevyWorkoutLandingStore::STREAM,
        }),
        Command::Extract(access) => {
            let summary = hevy_walk(access, landing, resumption, runs, database)?
                .extraction
                .extract()
                .await?;
            Ok(Outcome::Extracted(Box::new(summary)))
        }
        Command::Normalise(zone) => {
            let summary = hevy_normalisation(pool, zone).await?.normalise().await?;
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

/// The operator's historical spreadsheets: every file in the folder named, once
/// per distinct content (#263), and derived into manual weigh-ins (#273) and
/// manual gym sessions (#274).
async fn spreadsheet_files(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = SpreadsheetFileLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(folder) => {
            let extraction = Extraction::new(ExtractionPorts {
                source: FolderFiles::new(folder),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });
            Ok(Outcome::Extracted(Box::new(extraction.extract().await?)))
        }
        Command::Extract(_) => Err(WiringError::WrongCredential {
            stream: SpreadsheetFileLandingStore::STREAM.to_owned(),
            wanted: "a folder",
            given: "a credential",
        }),
        Command::Normalise(zone) => {
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: SpreadsheetFileAccountReader::new(pool.clone())?,
                    translator: SpreadsheetTranslator,
                    workouts: SqliteSpreadsheetStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        SpreadsheetFileLandingStore::STREAM,
                    )?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );
            Ok(Outcome::Derived(Box::new(normalisation.normalise().await?)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone(), SpreadsheetFileLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                SpreadsheetFileLandingStore::new(pool.clone())?,
                SqliteSpreadsheetStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), SpreadsheetFileLandingStore::STREAM)?,
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
            ExtractionStatus::new(landing, resumption, runs)
                .reset()
                .await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

/// Beyond The White Board's export: the file it sent, landed whole (#265), and
/// derived as the gym sessions it records (#285).
async fn btwb_exports(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = BtwbExportLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(folder) => {
            let extraction = Extraction::new(ExtractionPorts {
                source: FolderFiles::new(folder),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });
            Ok(Outcome::Extracted(Box::new(extraction.extract().await?)))
        }
        Command::Extract(_) => Err(WiringError::WrongCredential {
            stream: BtwbExportLandingStore::STREAM.to_owned(),
            wanted: "a folder",
            given: "a credential",
        }),
        Command::Normalise(zone) => {
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: BtwbExportAccountReader::new(pool.clone())?,
                    translator: BtwbTranslator,
                    workouts: SqliteBtwbStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        BtwbExportLandingStore::STREAM,
                    )?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );
            Ok(Outcome::Derived(Box::new(normalisation.normalise().await?)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone(), BtwbExportLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                BtwbExportLandingStore::new(pool.clone())?,
                SqliteBtwbStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), BtwbExportLandingStore::STREAM)?,
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
            ExtractionStatus::new(landing, resumption, runs)
                .reset()
                .await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

/// MyFitnessPal's export: the file the operator decrypted, landed whole, and
/// derived as the weights he logged in it (#242).
async fn myfitnesspal_exports(command: Command, database: &Path) -> Result<Outcome, WiringError> {
    let pool = connect(database).await?;
    let landing = MyFitnessPalExportLandingStore::new(pool.clone())?;
    let resumption = SqliteResumptionPointStore::new(pool.clone());
    let runs = SqliteExtractionRunLog::new(pool.clone());

    match command {
        Command::ExtractFolder(folder) => {
            let extraction = Extraction::new(ExtractionPorts {
                source: FolderFiles::new(folder),
                landing,
                resumption,
                runs,
                lock: FileRunLock::beside(database),
                clock: SystemClock,
            });
            Ok(Outcome::Extracted(Box::new(extraction.extract().await?)))
        }
        Command::Extract(_) => Err(WiringError::WrongCredential {
            stream: MyFitnessPalExportLandingStore::STREAM.to_owned(),
            wanted: "a folder",
            given: "a credential",
        }),
        Command::Normalise(zone) => {
            let normalisation = Normalisation::new(
                NormalisationPorts {
                    raw: MyFitnessPalExportAccountReader::new(pool.clone())?,
                    translator: MyFitnessPalWeighInTranslator,
                    workouts: SqliteMyFitnessPalStore::new(pool.clone())?,
                    refusals: SqliteRefusalStore::new(
                        pool.clone(),
                        MyFitnessPalExportLandingStore::STREAM,
                    )?,
                    runs: SqliteNormalisationRunLog::new(pool),
                    clock: SystemClock,
                },
                zone,
            );
            Ok(Outcome::Derived(Box::new(normalisation.normalise().await?)))
        }
        Command::Refusals => {
            let reporter = Refusals::new(
                SqliteRefusalStore::new(pool.clone(), MyFitnessPalExportLandingStore::STREAM)?,
                SqliteNormalisationRunLog::new(pool),
            );
            Ok(Outcome::Refused(Box::new(reporter.refusals().await?)))
        }
        Command::Status => {
            let derivation = DerivationStanding::new(
                MyFitnessPalExportLandingStore::new(pool.clone())?,
                SqliteMyFitnessPalStore::new(pool.clone())?,
                SqliteRefusalStore::new(pool.clone(), MyFitnessPalExportLandingStore::STREAM)?,
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
            ExtractionStatus::new(landing, resumption, runs)
                .reset()
                .await?;
            Ok(Outcome::Reset { previous })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BtwbExportLandingStore, GarminActivityFileLandingStore, GarminActivityLandingStore,
        GarminBodyBatteryEventLandingStore, GarminBodyBatteryLandingStore,
        GarminDailyHeartRateLandingStore, GarminDailySummaryLandingStore,
        GarminDetectedMoveLandingStore, GarminExerciseSetLandingStore,
        GarminFitnessAgeLandingStore, GarminFloorsLandingStore, GarminHealthSnapshotLandingStore,
        GarminHrvLandingStore, GarminIntensityMinutesLandingStore,
        GarminPersonalRecordLandingStore, GarminRacePredictionLandingStore,
        GarminRespirationLandingStore, GarminSleepLandingStore, GarminSpo2LandingStore,
        GarminStepsLandingStore, GarminStressLandingStore, GarminVo2MaxLandingStore,
        HevyWorkoutLandingStore, MyFitnessPalExportLandingStore, PelotonRideLandingStore,
        PelotonRideSampleLandingStore, SpreadsheetFileLandingStore,
        WithingsMeasurementLandingStore,
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
            GarminDailyHeartRateLandingStore::STREAM,
            GarminSleepLandingStore::STREAM,
            GarminStressLandingStore::STREAM,
            GarminBodyBatteryLandingStore::STREAM,
            GarminBodyBatteryEventLandingStore::STREAM,
            GarminRespirationLandingStore::STREAM,
            GarminSpo2LandingStore::STREAM,
            GarminIntensityMinutesLandingStore::STREAM,
            GarminFloorsLandingStore::STREAM,
            GarminDailySummaryLandingStore::STREAM,
            GarminStepsLandingStore::STREAM,
            GarminDetectedMoveLandingStore::STREAM,
            GarminVo2MaxLandingStore::STREAM,
            GarminHealthSnapshotLandingStore::STREAM,
            GarminFitnessAgeLandingStore::STREAM,
            GarminRacePredictionLandingStore::STREAM,
            GarminPersonalRecordLandingStore::STREAM,
            SpreadsheetFileLandingStore::STREAM,
            BtwbExportLandingStore::STREAM,
            MyFitnessPalExportLandingStore::STREAM,
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
            GarminSleepLandingStore::STREAM,
            GarminStressLandingStore::STREAM,
            GarminBodyBatteryLandingStore::STREAM,
            GarminBodyBatteryEventLandingStore::STREAM,
            GarminRespirationLandingStore::STREAM,
            GarminSpo2LandingStore::STREAM,
            GarminIntensityMinutesLandingStore::STREAM,
            GarminFloorsLandingStore::STREAM,
            GarminDailySummaryLandingStore::STREAM,
            GarminStepsLandingStore::STREAM,
            GarminDetectedMoveLandingStore::STREAM,
            GarminVo2MaxLandingStore::STREAM,
            GarminHealthSnapshotLandingStore::STREAM,
            GarminFitnessAgeLandingStore::STREAM,
            GarminRacePredictionLandingStore::STREAM,
            GarminPersonalRecordLandingStore::STREAM,
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

    /// A walk that cannot run: enough to see what is reported, and when.
    struct Refused;

    impl application::WorkoutExtractor for Refused {
        async fn extract(&self) -> Result<application::RunSummary, application::ExtractionError> {
            Err(application::ExtractionError::AlreadyRunning)
        }
    }

    /// Writes down what it was told, in order.
    #[derive(Default)]
    struct Recorded(std::sync::Mutex<Vec<String>>);

    impl super::WalkReporter for Recorded {
        fn started(&self, stream: &domain::landing::LandingStream) {
            if let Ok(mut told) = self.0.lock() {
                told.push(format!("started {stream}"));
            }
        }

        fn finished(&self, walk: &super::Walk) {
            if let Ok(mut told) = self.0.lock() {
                let how = if walk.result.is_ok() { "ok" } else { "refused" };
                told.push(format!("finished {} {how}", walk.stream));
            }
        }
    }

    /// **Each walk is told as it starts and as it ends, before the next begins**
    /// (#437), so a frontend can say which walk is under way and report each
    /// as soon as it is done rather than all of them at the end.
    #[test]
    fn each_walk_is_reported_as_it_starts_and_as_it_ends() {
        let stream = |name: &str| {
            domain::landing::LandingStream::try_from(name).expect("a well-formed stream name")
        };
        let recorded = Recorded::default();
        let both = async {
            super::walk(stream("garmin.activities"), Refused, &recorded).await;
            super::walk(stream("garmin.exercise_sets"), Refused, &recorded).await;
        };
        // Nothing here waits on anything, so one poll finishes it and no
        // runtime is needed.
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            std::future::Future::poll(std::pin::pin!(both), &mut context).is_ready(),
            "two walks that cannot run finish at once"
        );

        let told = recorded.0.lock().expect("the record is readable").clone();
        assert_eq!(
            told,
            vec![
                "started garmin.activities",
                "finished garmin.activities refused",
                "started garmin.exercise_sets",
                "finished garmin.exercise_sets refused",
            ]
        );
    }
}
