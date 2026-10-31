//! `next`, over this build's adapters.
//!
//! **The use case is `application::next`** (#397); what is here is which
//! adapters it runs over, so `cli` and `web` start the same run and neither
//! composes it for itself (#398).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tracing::Instrument as _;

use application::{
    CanonicalLayer, Clock, DerivedFrom, FetchLog, Fetched, NormalisationError,
    NormalisationSummary, NormalisedLayers, Part, Rederivation, RunLockError, StoreError,
    commit::{CommitPorts, Committing},
    cycling::{Riding, RidingPorts},
    microcycle::{Microcycle, MicrocyclePorts},
    next::{
        CanonicalPorts, CyclingPorts, Done, GymPorts, IfGoing, Next, NextError, NextPorts,
        NoAccess, Progress, Refetch, Report, Scope,
    },
};
use domain::{
    landing::{FetchedAt, LandingStream},
    normalised::OperatorZone,
    prescription::{DestinationName, InvalidDelivery},
    schedule::{DayPart, Discipline},
};
use infrastructure::{
    BtwbExportLandingStore, FileRunLock, GarminActivityFileLandingStore,
    GarminActivityLandingStore, GarminDailyHeartRateLandingStore, GarminExerciseSetLandingStore,
    GarminHrvLandingStore, GarminSleepLandingStore, HevyWorkoutLandingStore,
    MyFitnessPalExportLandingStore, PelotonAccountLandingStore, PelotonRideDetailLandingStore,
    PelotonRideLandingStore, PelotonRideSampleLandingStore, SpreadsheetFileLandingStore,
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCanonicalWeighInStore, SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore,
    SqliteDerivationLedger, SqliteDiaryStore, SqliteExerciseHistory, SqliteExtractionRunLog,
    SqliteFtpHistory, SqliteGenerationParameterStore, SqliteGymMesocycleStore,
    SqliteNormalisedCyclingSessionReader, SqliteNormalisedGymSessionReader,
    SqliteNormalisedWeighInReader, SqlitePelotonClassStore, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, WithingsMeasurementLandingStore,
    connect,
    peloton::{
        PelotonClassSource, PelotonHoldingRides, PelotonProgrammes, PelotonRideDestination,
        catalogue::ClassCatalogue,
    },
};

use crate::{
    access, catalogue,
    config::{SourceAccess, source_access_for},
    holidays::Calendars,
    telemetry::{self, Run, Spanned},
    wiring::{self, Command, Outcome, SystemClock, Unreported, WiringError},
};

/// Why `next` could not be started, or could not answer.
#[derive(Debug, thiserror::Error)]
pub enum NextFailure {
    #[error("this build has no daily loop for {discipline}")]
    NoLoop { discipline: Discipline },
    #[error(transparent)]
    Destination(#[from] InvalidDelivery),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Wiring(#[from] WiringError),
    #[error(transparent)]
    Next(#[from] NextError),
}

/// Collect everything, say where the plan stands, and deliver what is next.
///
/// `skip` takes what is left of the current session's window first, for the
/// reason given (#391). Progress is reported as each step runs.
///
/// # Errors
///
/// [`NextFailure`] if another run of `next` is going, if the store is
/// unavailable, if no week has been recorded, or if a skip cannot be taken. A
/// source that cannot be reached is a failed step in the report and nothing
/// more (§ 36).
pub async fn run(
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
    now: DayPart,
    skip: Option<&str>,
    run: Run,
    progress: &(impl Progress + Sync),
) -> Result<Report, NextFailure> {
    let if_going = run.if_going();
    let refetch = run.refetch();
    let spanned = Spanned::new(progress, run.span);
    let answered = over(
        database,
        zone,
        credentials,
        refetch,
        Asked::Next {
            now,
            skip,
            if_going,
        },
        &spanned,
    )
    .instrument(spanned.run.clone())
    .await;
    telemetry::run_ended(&spanned.run, &answered);
    match answered? {
        Answered::Report(report) => Ok(*report),
        Answered::Collected(_) | Answered::Wanted(_) => unreachable_answer(),
    }
}

/// Collect and derive only `scope`, joining any run collecting part of it, and
/// deliver nothing (#559).
///
/// # Errors
///
/// [`NextFailure`] if the store or a lock is unavailable. A source that cannot
/// be reached is a failed step and nothing more (§ 36).
pub async fn collect(
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
    now: DayPart,
    scope: &Scope,
    run: Run,
    progress: &(impl Progress + Sync),
) -> Result<Vec<Done>, NextFailure> {
    let refetch = run.refetch();
    let spanned = Spanned::new(progress, run.span);
    let answered = over(
        database,
        zone,
        credentials,
        refetch,
        Asked::Collect { scope, now },
        &spanned,
    )
    .instrument(spanned.run.clone())
    .await;
    telemetry::run_ended(&spanned.run, &answered);
    match answered? {
        Answered::Collected(run) => Ok(run),
        Answered::Wanted(_) | Answered::Report(_) => unreachable_answer(),
    }
}

/// Whether a page's run over `scope` would do anything (#612): fetch a
/// stream that could hold something new, or bring a layer current. A page
/// that would do nothing starts no run.
///
/// # Errors
///
/// [`NextFailure`] if the store is unavailable.
pub async fn wanted(
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
    now: DayPart,
    scope: &Scope,
    frontend: telemetry::Frontend,
) -> Result<bool, NextFailure> {
    match over(
        database,
        zone,
        credentials,
        frontend.refetch(),
        Asked::Wanted { scope, now },
        &Unheard,
    )
    .await?
    {
        Answered::Wanted(wanted) => Ok(wanted),
        Answered::Collected(_) | Answered::Report(_) => unreachable_answer(),
    }
}

/// Nothing to report to: asking whether a run is wanted runs nothing.
struct Unheard;

impl Progress for Unheard {
    fn started(&self, _step: &application::next::Step) {}
    fn estimated(
        &self,
        _step: &application::next::Step,
        _outstanding: u64,
        _takes: std::time::Duration,
    ) {
    }
    fn finished(&self, _done: &Done) {}
}

/// Whether a run, started by either frontend, holds any part of `scope`
/// (#467): a page reading what one is collecting shows that rather than the
/// store mid-derivation.
///
/// # Errors
///
/// [`RunLockError::Unavailable`] if a lock cannot be tried.
pub fn collecting(database: &Path, scope: &Scope) -> Result<bool, RunLockError> {
    FileRunLock::beside(database)
        .held(scope.parts())
        .map(|held| !held.is_empty())
}

/// Which of the use case's two acts is asked for.
enum Asked<'a> {
    Next {
        now: DayPart,
        skip: Option<&'a str>,
        if_going: IfGoing,
    },
    Collect {
        scope: &'a Scope,
        now: DayPart,
    },
    Wanted {
        scope: &'a Scope,
        now: DayPart,
    },
}

enum Answered {
    Report(Box<Report>),
    Collected(Vec<Done>),
    Wanted(bool),
}

/// An act answered as the other, which `over` never does.
fn unreachable_answer<T>() -> Result<T, NextFailure> {
    Err(NextFailure::Store(StoreError::Unavailable {
        detail: "next answered a different question from the one asked".to_owned(),
    }))
}

/// The use case over this build's adapters, asked `asked`.
async fn over(
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
    refetch: Refetch,
    asked: Asked<'_>,
    progress: &(impl Progress + Sync),
) -> Result<Answered, NextFailure> {
    let pool = connect(database).await?;

    // **This build collects the gym from Hevy and cycling from Peloton**, and
    // the catalogue says so: a second source for either is an entry there and
    // an arm in `wiring`.
    let gym = access_to(known(Discipline::Gym)?.collects(), credentials);
    let gym_records = wiring::hevy_records(gym, database).await?;
    let cycling = access_to(known(Discipline::Cycling)?.collects(), credentials);
    let cycling_records = wiring::peloton_records(cycling, database).await?;
    let weigh_in_records =
        wiring::withings_records(stream_access(WEIGH_INS, credentials), database).await?;
    let (watch_records, watch_days) =
        wiring::garmin_records(stream_access(WATCH, credentials), database).await?;

    // **A credential that is absent costs what needs it, and nothing else.**
    // The catalogue answers choosing a class and building a mesocycle without
    // Peloton; refreshing it and delivering to the stack are what it costs.
    let unreachable = |error: &access::AccessError| NoAccess {
        detail: error.to_string(),
    };
    let peloton = access::peloton(credentials);
    let stored = SqlitePelotonClassStore::new(pool.clone());
    let classes = peloton.as_ref().ok().map(|peloton| &peloton.classes);
    let catalogue = ClassCatalogue::over(&stored, classes);
    let class_source = peloton
        .as_ref()
        .map(|peloton| PelotonClassSource::new(&peloton.classes, &stored))
        .map_err(unreachable);
    let ride_destination = peloton
        .as_ref()
        .map(|peloton| PelotonRideDestination::new(&peloton.classes, &peloton.stack))
        .map_err(unreachable);
    let gym_destination = access::hevy_routines(known(Discipline::Gym)?.delivers_to(), credentials)
        .map_err(|error| unreachable(&error));

    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());
    let parameters = SqliteGenerationParameterStore::new(pool.clone());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let next = Next::new(NextPorts {
        gym_records,
        cycling_records,
        weigh_in_records,
        watch_records,
        watch_days,
        normalised: Layers {
            database: database.to_path_buf(),
            zone: zone.clone(),
        },
        canonical: canonical(&pool),
        ledger: SqliteDerivationLedger::new(pool.clone()),
        classes: class_source,
        holidays: Calendars,
        diary: SqliteDiaryStore::new(pool.clone()),
        plans: plans.clone(),
        microcycle: microcycle(&pool, &plans, &deliveries)?,
        committing: Committing::new(
            CommitPorts {
                plans: plans.clone(),
                parameters: parameters.clone(),
                holding: PelotonHoldingRides::new(catalogue),
                ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
                published: PelotonProgrammes::new(catalogue),
            },
            zone.clone(),
        ),
        gym: GymPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            mesocycles: SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
            plans,
            parameters,
            prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
            lifecycle: deliveries.clone(),
            deliveries,
            destination: gym_destination,
        },
        cycling: CyclingPorts {
            riding: riding(&pool, zone, catalogue)?,
            destination: ride_destination,
        },
        // Beside the store, as extraction's are: a copy of the store elsewhere
        // is a separate system with its own lock.
        lock: FileRunLock::beside(database),
        fetches: Fetches(SqliteExtractionRunLog::new(pool.clone())),
        zone: zone.clone(),
        refetch,
    });

    // **Boxed, because it holds every source's fetch at once** (#468).
    Ok(match asked {
        Asked::Next {
            now,
            skip,
            if_going,
        } => Answered::Report(Box::new(
            Box::pin(next.run(now, skip, if_going, progress)).await?,
        )),
        Asked::Collect { scope, now } => {
            Answered::Collected(Box::pin(next.collect(scope, now, progress)).await?)
        }
        Asked::Wanted { scope, now } => Answered::Wanted(next.wanted(scope, now).await?),
    })
}

/// What a chart reads, from which the parts its page's run collects follow
/// (#559).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reads {
    /// A landing stream, read as it landed.
    Landed(&'static str),
    /// A normalised layer, and every walk it is derived from.
    Normalised(&'static str),
    /// A canonical layer, and every normalised layer it is built from.
    Canonical(CanonicalLayer),
}

/// The parts a run collecting what `reads` names takes: each part read, and
/// every part it is derived from.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn scope(reads: impl IntoIterator<Item = Reads>) -> Result<Scope, WiringError> {
    let stream = |name: &str| LandingStream::try_from(name).map_err(WiringError::from);
    let normalised = |name: &str| -> Result<Vec<Part>, WiringError> {
        let mut parts = vec![Part::Normalising(stream(name)?)];
        for landed in landed_from(name) {
            parts.push(Part::Fetching(stream(landed)?));
        }
        Ok(parts)
    };
    let mut parts = Vec::new();
    for read in reads {
        match read {
            Reads::Landed(name) => parts.push(Part::Fetching(stream(name)?)),
            Reads::Normalised(name) => parts.extend(normalised(name)?),
            Reads::Canonical(layer) => {
                parts.push(Part::Canonicalising(layer));
                for name in built_from(layer) {
                    parts.extend(normalised(name)?);
                }
            }
        }
    }
    Ok(Scope::new(parts))
}

/// The walks `next` lands a normalised layer's records by. None for a stream
/// read from a folder, which `next` derives but does not fetch.
///
/// **The landing tables [`wiring::inputs`] counts for the stream**, which are
/// what its arm of [`wiring::run`] derives from.
fn landed_from(normalised: &str) -> &'static [&'static str] {
    match normalised {
        HevyWorkoutLandingStore::STREAM => &[HevyWorkoutLandingStore::STREAM],
        PelotonRideLandingStore::STREAM => &[
            PelotonRideLandingStore::STREAM,
            PelotonRideDetailLandingStore::STREAM,
            PelotonRideSampleLandingStore::STREAM,
            PelotonAccountLandingStore::STREAM,
        ],
        WithingsMeasurementLandingStore::STREAM => &[WithingsMeasurementLandingStore::STREAM],
        GarminHrvLandingStore::STREAM => &[
            GarminHrvLandingStore::STREAM,
            GarminSleepLandingStore::STREAM,
        ],
        GarminActivityLandingStore::STREAM => &[
            GarminActivityLandingStore::STREAM,
            GarminExerciseSetLandingStore::STREAM,
            GarminActivityFileLandingStore::STREAM,
        ],
        GarminDailyHeartRateLandingStore::STREAM => &[
            GarminDailyHeartRateLandingStore::STREAM,
            GarminSleepLandingStore::STREAM,
        ],
        GarminSleepLandingStore::STREAM => &[GarminSleepLandingStore::STREAM],
        _ => &[],
    }
}

/// The normalised layers each canonical layer is built from: the streams that
/// write the normalised tables its reader reads.
const fn built_from(layer: CanonicalLayer) -> &'static [&'static str] {
    match layer {
        CanonicalLayer::Gym => &[
            HevyWorkoutLandingStore::STREAM,
            GarminActivityLandingStore::STREAM,
            SpreadsheetFileLandingStore::STREAM,
            BtwbExportLandingStore::STREAM,
        ],
        CanonicalLayer::Cycling => &[PelotonRideLandingStore::STREAM],
        CanonicalLayer::WeighIns => &[
            WithingsMeasurementLandingStore::STREAM,
            SpreadsheetFileLandingStore::STREAM,
            MyFitnessPalExportLandingStore::STREAM,
        ],
    }
}

/// Each canonical layer, and the normalised layers it is built from.
pub(crate) fn canonical(
    pool: &infrastructure::SqlitePool,
) -> CanonicalPorts<
    SqliteNormalisedGymSessionReader,
    SqliteCanonicalGymSessionStore,
    SqliteNormalisedCyclingSessionReader,
    SqliteCanonicalCyclingSessionStore,
    SqliteNormalisedWeighInReader,
    SqliteCanonicalWeighInStore,
> {
    CanonicalPorts {
        normalised_gym: SqliteNormalisedGymSessionReader::new(pool.clone()),
        canonical_gym: SqliteCanonicalGymSessionStore::new(pool.clone()),
        normalised_cycling: SqliteNormalisedCyclingSessionReader::new(pool.clone()),
        canonical_cycling: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        normalised_weigh_ins: SqliteNormalisedWeighInReader::new(pool.clone()),
        canonical_weigh_ins: SqliteCanonicalWeighInStore::new(pool.clone()),
    }
}

/// The commit this build was made from, and that commit's date, from the
/// `FITNESS_BUILD` the flake's wrapper sets around each installed binary.
///
/// **Read when the binary runs, not when it compiles** (#658). It changes with
/// every commit, and read at compile time it compiled `cli` and `web` again
/// for the package, so what was installed was never the build the checks ran.
///
/// **`None` outside nix**, where nothing says which commit is running.
pub fn build() -> Option<&'static str> {
    static BUILD: OnceLock<Option<String>> = OnceLock::new();
    BUILD
        .get_or_init(|| std::env::var("FITNESS_BUILD").ok())
        .as_deref()
}

/// Where the plan stands, over this build's stores.
fn microcycle(
    pool: &infrastructure::SqlitePool,
    plans: &SqlitePlanStore,
    deliveries: &SqlitePrescriptionDeliveryStore,
) -> Result<
    Microcycle<
        SqliteDiaryStore,
        SqlitePlanStore,
        SqlitePrescriptionDeliveryStore,
        SqliteCyclingDeliveryStore,
        SqliteCanonicalGymSessionStore,
        SqliteCanonicalCyclingSessionStore,
    >,
    NextFailure,
> {
    Ok(Microcycle::new(
        MicrocyclePorts {
            diary: SqliteDiaryStore::new(pool.clone()),
            plans: plans.clone(),
            gym_deliveries: deliveries.clone(),
            cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
            gym_performed: SqliteCanonicalGymSessionStore::new(pool.clone()),
            cycling_performed: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        },
        destination(Discipline::Gym)?,
        destination(Discipline::Cycling)?,
    ))
}

/// The extraction run log, read for when each stream was last fetched, and
/// the clock that says how long ago that was (#612).
struct Fetches(SqliteExtractionRunLog);

impl FetchLog for Fetches {
    async fn fetched(&self, stream: &LandingStream) -> Result<Fetched, StoreError> {
        self.0.fetched(stream).await
    }
}

impl Clock for Fetches {
    fn now(&self) -> FetchedAt {
        SystemClock.now()
    }
}

/// Every stream this build derives, each through its own arm of
/// [`wiring::run`], which is also how one is derived from the command line.
struct Layers {
    database: PathBuf,
    zone: OperatorZone,
}

impl Layers {
    fn known(stream: &LandingStream) -> Result<&'static catalogue::KnownStream, WiringError> {
        catalogue::lookup(&stream.to_string()).ok_or_else(|| WiringError::Unwired {
            stream: stream.to_string(),
        })
    }
}

/// A failure to read what a layer is derived from is the store's.
fn unavailable(error: &WiringError) -> StoreError {
    match error {
        WiringError::Store(error) => error.clone(),
        other => StoreError::Unavailable {
            detail: other.to_string(),
        },
    }
}

impl NormalisedLayers for Layers {
    fn streams(&self) -> Vec<LandingStream> {
        catalogue::KNOWN
            .iter()
            .filter(|known| wiring::derives(known))
            .filter_map(|known| known.landing_stream().ok())
            .collect()
    }

    async fn inputs(&self, stream: &LandingStream) -> Result<DerivedFrom, StoreError> {
        let known = Self::known(stream).map_err(|error| unavailable(&error))?;
        wiring::inputs(known, &self.database, &self.zone)
            .await
            .map_err(|error| unavailable(&error))
    }

    async fn normalise(
        &self,
        stream: &LandingStream,
    ) -> Result<NormalisationSummary, NormalisationError> {
        let failed = |error: &WiringError| match error {
            WiringError::Normalisation(error) => error.clone(),
            other => NormalisationError::Store(unavailable(other)),
        };
        let known = Self::known(stream).map_err(|error| failed(&error))?;
        match wiring::run(
            Command::Normalise(self.zone.clone(), Rederivation::Incremental),
            known,
            &self.database,
            &Unreported,
        )
        .await
        {
            Ok(Outcome::Derived(summary)) => Ok(*summary),
            Ok(_) => Err(NormalisationError::Store(StoreError::Unavailable {
                detail: format!("deriving {stream} answered something other than a derivation"),
            })),
            Err(error) => Err(failed(&error)),
        }
    }
}

/// How a stream `next` collects is reached: its system's credential and root.
///
/// **No access is a step that fails, not a run that does** (§ 36): a stream
/// read from a folder, or a source with no credential, is reported and passed.
fn access_to(
    stream: &catalogue::KnownStream,
    credentials: &infrastructure::Credentials,
) -> Result<SourceAccess, NoAccess> {
    let system = stream.system().ok_or_else(|| NoAccess {
        detail: format!("{} is read from a folder, not a system", stream.name()),
    })?;
    source_access_for(system, None, credentials).map_err(|error| NoAccess {
        detail: error.to_string(),
    })
}

/// How an entry `next` collects that no discipline does is reached, by its
/// name: body weight, and what the watch records.
fn stream_access(
    name: &str,
    credentials: &infrastructure::Credentials,
) -> Result<SourceAccess, NoAccess> {
    let stream = catalogue::lookup(name).ok_or_else(|| NoAccess {
        detail: format!("this build does not collect {name}"),
    })?;
    access_to(stream, credentials)
}

/// Where body weight comes from.
pub const WEIGH_INS: &str = "withings.measurements";

/// What the watch records is collected under (#432): its activities while a
/// gym session could have been performed (#612), and its days and nights until
/// yesterday is whole.
pub const WATCH: &str = "garmin.activities";

/// The cycling use case, over this build's stores and catalogue.
///
/// # Errors
///
/// [`NextFailure`] if the catalogue names no destination for cycling.
pub fn riding<'a>(
    pool: &infrastructure::SqlitePool,
    zone: &OperatorZone,
    catalogue: ClassCatalogue<'a>,
) -> Result<
    Riding<
        SqliteCyclingMesocycleStore,
        SqlitePlanStore,
        PelotonHoldingRides<'a>,
        SqliteCanonicalCyclingSessionStore,
        SqliteFtpHistory,
        SqliteCyclingDeliveryStore,
    >,
    NextFailure,
> {
    Ok(Riding::new(
        RidingPorts {
            mesocycles: SqliteCyclingMesocycleStore::new(pool.clone()),
            plans: SqlitePlanStore::new(pool.clone(), zone.clone()),
            holding: PelotonHoldingRides::new(catalogue),
            ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
            ftp: SqliteFtpHistory::new(pool.clone()),
            deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
        },
        destination(Discipline::Cycling)?,
    ))
}

/// Where a discipline's sessions are put, as the catalogue names it.
///
/// # Errors
///
/// [`NextFailure`] if this build has no loop for the discipline, or its
/// destination's name is not one.
pub fn destination(discipline: Discipline) -> Result<DestinationName, NextFailure> {
    Ok(DestinationName::try_from(
        known(discipline)?.delivers_to().name().to_owned(),
    )?)
}

fn known(discipline: Discipline) -> Result<&'static catalogue::KnownDiscipline, NextFailure> {
    catalogue::discipline(discipline.as_str()).ok_or(NextFailure::NoLoop { discipline })
}

#[cfg(test)]
mod tests {
    use application::{CanonicalLayer, Part, next::Scope};
    use domain::landing::LandingStream;

    use super::landed_from;
    use crate::{body_weight, catalogue, composition, heart_rate, one_rep_max, per_device, wiring};

    fn named(scope: &Scope) -> Vec<String> {
        scope
            .parts()
            .iter()
            .map(|part| match part {
                Part::Fetching(stream) => format!("fetching {stream}"),
                Part::Normalising(stream) => format!("normalising {stream}"),
                Part::Canonicalising(layer) => format!("canonicalising {}", layer.as_str()),
                Part::Delivering => "delivering".to_owned(),
            })
            .collect()
    }

    /// **Each chart's page collects what its chart reads and nothing else**
    /// (#559): the HRV chart its nights, the sleep records naming each night's
    /// watch and Garmin's recordings naming the watch (#569), never Hevy,
    /// Withings or Peloton, and never a delivery.
    #[test]
    fn each_chart_collects_what_it_reads() {
        assert_eq!(
            named(&per_device::overnight_hrv_collects().expect("a scope")),
            [
                "fetching garmin.activities",
                "fetching garmin.activity_files",
                "fetching garmin.daily_heart_rate",
                "fetching garmin.exercise_sets",
                "fetching garmin.hrv",
                "fetching garmin.sleep",
                "normalising garmin.activities",
                "normalising garmin.daily_heart_rate",
                "normalising garmin.hrv",
            ]
        );
        assert_eq!(
            named(&one_rep_max::collects().expect("a scope")),
            [
                "fetching garmin.activities",
                "fetching garmin.activity_files",
                "fetching garmin.exercise_sets",
                "fetching hevy.workouts",
                "fetching withings.measurements",
                "normalising btwb.exports",
                "normalising garmin.activities",
                "normalising hevy.workouts",
                "normalising myfitnesspal.exports",
                "normalising spreadsheets.files",
                "normalising withings.measurements",
                "canonicalising gym",
                "canonicalising weigh-ins",
            ]
        );
        assert_eq!(
            named(&body_weight::collects().expect("a scope")),
            [
                "fetching withings.measurements",
                "normalising myfitnesspal.exports",
                "normalising spreadsheets.files",
                "normalising withings.measurements",
                "canonicalising weigh-ins",
            ]
        );
        for scope in [
            per_device::resting_heart_rate_collects(),
            heart_rate::collects(),
            composition::collects(),
        ] {
            let scope = scope.expect("a scope");
            assert!(!scope.has(&Part::Delivering));
            assert!(!scope.has(&Part::Fetching(
                LandingStream::try_from("hevy.workouts").expect("a stream")
            )));
        }
        assert!(
            heart_rate::collects()
                .expect("a scope")
                .has(&Part::Canonicalising(CanonicalLayer::Cycling))
        );
        // #569: a ride's monitor is Peloton's account's, and a day's watch
        // Garmin's own records'; the register is Garmin's, for history.
        let account = Part::Fetching(LandingStream::try_from("peloton.account").expect("a stream"));
        assert!(heart_rate::collects().expect("a scope").has(&account));
        let resting = per_device::resting_heart_rate_collects().expect("a scope");
        assert!(!resting.has(&account));
        for part in [
            Part::Fetching(LandingStream::try_from("garmin.sleep").expect("a stream")),
            Part::Normalising(
                LandingStream::try_from("garmin.daily_heart_rate").expect("a stream"),
            ),
            Part::Normalising(LandingStream::try_from("garmin.activities").expect("a stream")),
        ] {
            assert!(resting.has(&part), "{part:?}");
        }
    }

    /// **A run the terminal started is seen by its locks** (#467): the
    /// estimated 1RM page is collecting while a run holds the gym's canonical
    /// layer, and body weight, which does not read it, is not.
    #[test]
    fn a_run_holding_a_part_a_page_reads_is_collecting_it() {
        use application::{NextLock as _, Taken};

        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let database = directory.path().join("fitness.db");
        let one_rep_max = one_rep_max::collects().expect("a scope");
        let body_weight = body_weight::collects().expect("a scope");
        assert!(!super::collecting(&database, &one_rep_max).expect("tried"));

        let Ok(Taken::All(_running)) = infrastructure::FileRunLock::beside(&database).try_acquire(
            &std::collections::BTreeSet::from([Part::Canonicalising(CanonicalLayer::Gym)]),
        ) else {
            panic!("the run takes its part");
        };
        assert!(super::collecting(&database, &one_rep_max).expect("tried"));
        assert!(!super::collecting(&database, &body_weight).expect("tried"));
    }

    /// **Every stream `next` fetches and derives says what it is landed by**,
    /// so a chart reading it fetches it: a stream added without saying would
    /// be derived by a page's run from whatever the last whole run landed.
    #[test]
    fn every_fetched_stream_says_what_it_is_landed_by() {
        for known in catalogue::KNOWN
            .iter()
            .filter(|known| wiring::derives(known) && known.system().is_some())
        {
            assert!(
                !landed_from(&known.name()).is_empty(),
                "{} says nothing of what lands it",
                known.name()
            );
        }
    }
}
