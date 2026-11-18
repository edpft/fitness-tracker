//! Each call through a port the store answers, as a `call` span (#816).
//!
//! **An adapter is only ever handed out wrapped.** [`super::Store`] holds the
//! pool and nothing else in this crate can open one, so every adapter a use
//! case runs over is a [`Spanned`]: a port this does not implement is a port no
//! use case can be given, and the build says so where it is composed.
//!
//! **Only what is built from the pool is wrapped.** A use case or a composite
//! of ports calls through the ones it was given, so its calls are each spanned
//! once and a call is never inside another.

use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{Arc, Mutex, PoisonError},
};

use application::{
    AccountReader, Activities, CalendarStore, CanonicalCyclingSessionStore,
    CanonicalGymSessionStore, CanonicalLayer, CanonicalWeighInStore, ClassSource, ClassesHeld,
    ClassesListed, ClassesRead, CompositionHistory, CyclingDeliveryStore, CyclingMesocycleStore,
    DeliveryReference, DerivationLedger, DerivedFrom, DestinationName, DestinationReply,
    DiaryAuthor, DiaryStore, EquipmentStore, ExerciseHistory, ExtractionRunLog, FetchLog, Fetched,
    FillChangeStore, FtpHistory, FulfilledSession, GenerationParameterStore, HoldingRides,
    HolidayCalendar, Input, Kept, LandingStore, LastPerformance, Mark, MesocycleInForce,
    MesocycleStore, NewestObservation, NormalisationRunLog, NormalisedCyclingSessionReader,
    NormalisedEntityStore, NormalisedGymSessionReader, NormalisedWeighInReader, Occupant, Output,
    OvernightHrvHistory, Performances, PerformedCyclingSessions, PerformedGymSessions,
    PerformedSessionLog, PlanStore, PlayedPadelMatch, PlayedPadelMatches, PrescribedWorkoutId,
    PrescribedWorkoutStore, PrescriptionDeliveryStore, PrescriptionLifecycle, ProgressionStore,
    PublishedCycling, PublishedProgrammes, PublishedRead, PulseWaveVelocityHistory, RawExtent,
    Reached, Reading, Reads, RecordedDevices, RefusalStore, RenderingDigest,
    RestingHeartRateHistory, ResumptionPointStore, Revision, Revisions, RiddenSession,
    RiddenSessionLog, RiddenVenues, RideDestination, Served, SessionOrdinal, SleepHistory,
    SourceError, StoreError, StressHistory, TrainingDone, WearRecord, WeighInHistory, WorkoutStore,
    graph::{Calendar, Generation, Node, Table},
};
use domain::analytical::Weighed;
use domain::body::{CanonicalWeighIn, WeighInAccount, WeighInCount};
use domain::canonical::{Occurred, Reach, SessionCount};
use domain::cycling::{
    CanonicalBikePlusRide, CanonicalCyclingSession, CyclingMesocycle, CyclingMesocycleId,
    CyclingSession, DeliveredRide, Ftp, RideVenue,
};
use domain::gym::{CanonicalGymSession, NormalisedGymSession, Performance, exercise::RepsExercise};
use domain::landing::{
    ExtractionRun, FetchedAt, LandingRecord, LandingRecordId, LandingStream, PayloadDigest,
    RecordCount, RunId, RunOutcome, SourceRecordId, Watermark,
};
use domain::normalised::{
    NormalisationOutcome, NormalisationRun, NormalisationRunId, Refusal, RefusalCount, StartedAt,
    WorkoutCount,
};
use domain::plan::{Plan, PlanId, PlanName, PlanWindow};
use domain::planner::{Completed, Relative, RerunOrdinal, SessionRole};
use domain::prescription::{
    GymMesocycle, LinearProgression, MesocycleId, PrescribedWorkout, PrescriptionState,
    SbsProgression, Scales, Workout,
};
use domain::provider::ExternalProgramme;
use domain::schedule::{Alteration, Diary, Holidays, Illness, Skip, TrainingPattern};
use infrastructure::Commits;
use jiff::{Timestamp, civil::Date};
use tracing::{Instrument as _, field::Empty};

/// An adapter, each call through its ports a `call` span named for the port
/// and the method, with the rows it returned where it returned a list.
#[derive(Debug, Clone)]
pub struct Spanned<A>(A, Answers);

impl<A> Spanned<A> {
    /// `adapter`, keeping nothing: one not over the store.
    pub(crate) fn new(adapter: A) -> Self {
        Self(adapter, Answers::default())
    }

    /// `adapter` over the store, keeping what its held calls answer in `kept`.
    pub(crate) const fn kept(adapter: A, kept: Answers) -> Self {
        Self(adapter, kept)
    }

    /// A call that is not through a port, spanned as `name`: an adapter's
    /// own method, which a port does not declare.
    pub(crate) fn call<'a, F: Future>(
        &'a self,
        name: &'static str,
        call: impl FnOnce(&'a A) -> F,
    ) -> tracing::instrument::Instrumented<F> {
        call(&self.0).instrument(tracing::info_span!("call", call = name, rows = Empty))
    }

    /// A call that is not through a port, spanned as `name` and answered as
    /// it last was where nothing has been committed since ([`Answers`]).
    ///
    /// **`with` is what else the answer depends on**: what the adapter was
    /// built with, and what `call` is asked.
    pub(crate) fn held<'a, T, E, F>(
        &'a self,
        name: &'static str,
        with: String,
        on: &Reads,
        call: impl FnOnce(&'a A) -> F,
    ) -> impl Future<Output = Result<T, E>> + 'a
    where
        T: Clone + Send + Sync + 'static,
        F: Future<Output = Result<T, E>> + Send + 'a,
    {
        let answers = self.1.clone();
        let on = on.clone();
        let asked = self
            .1
            .answered::<T>(name, &with)
            .map_err(|at| (at, self.call(name, call)));
        async move {
            match asked {
                Ok(answer) => Ok(answer),
                Err((at, call)) => {
                    let answer = call.await?;
                    if let Some(at) = at {
                        answers.keep(name, with, at, &on, answer.clone());
                    }
                    Ok(answer)
                }
            }
        }
    }
}

/// What a call returned, asked how many rows it was.
///
/// By autoref: [`Listed`] answers for a list and [`Unlisted`], one reference
/// further away, for anything else, so a call says its rows where it has
/// them without each port saying which of its calls do.
struct Returned<'a, T>(&'a T);

trait Listed {
    fn rows(&self) -> Option<usize>;
}

impl<T, E> Listed for Returned<'_, Result<Vec<T>, E>> {
    fn rows(&self) -> Option<usize> {
        self.0.as_ref().ok().map(Vec::len)
    }
}

trait Unlisted {
    fn rows(&self) -> Option<usize>;
}

impl<T> Unlisted for &Returned<'_, T> {
    fn rows(&self) -> Option<usize> {
        None
    }
}

/// What the store answered, kept until a commit by this process moves what
/// it was read from (#830).
///
/// **Nothing moves the store but a commit**, and this process sees only its
/// own: the operator, 2026-11-18, *"until they do it's okay for the web server
/// to be unaware of changes made by the CLI."* So a call asked again with
/// nothing it reads moved since is answered from what it answered before, and
/// reads nothing.
///
/// **What it reads is what it declares, and everything upstream of that** in
/// the graph, so a weigh-in landing moves what is derived from weigh-ins and
/// nothing else. The operator, 2026-11-18: *"for Withings, I would expect,
/// say, body weight to update but not, say, sleep."*
///
/// **Keyed on the adapter, the call and its arguments**, and kept only where
/// every commit had settled when it was asked and none was made while it read
/// ([`Commits`]): a read that might have seen the store either side of a commit
/// is answered and forgotten.
#[derive(Clone, Default)]
pub struct Answers {
    commits: Option<Commits>,
    answers: Arc<Mutex<HashMap<Asked, Answer>>>,
}

/// A call through a port, and its arguments as they were debug-formatted.
type Asked = (&'static str, String);

/// What a call answered: the count of commits it was asked at, what it was
/// read from, and the answer.
type Answer = (u64, Arc<BTreeSet<Node>>, Arc<dyn Any + Send + Sync>);

impl std::fmt::Debug for Answers {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Answers")
            .field("commits", &self.commits)
            .finish_non_exhaustive()
    }
}

impl Answers {
    /// Keeping what is answered until the next of `commits`.
    pub(crate) fn until(commits: Commits) -> Self {
        Self {
            commits: Some(commits),
            answers: Arc::default(),
        }
    }

    /// What `call` answered when asked `with`, where nothing it was read from
    /// has moved since, and otherwise the count to keep a new answer at,
    /// where one read now would be worth keeping.
    pub(super) fn answered<T: Clone + 'static>(
        &self,
        call: &'static str,
        with: &str,
    ) -> Result<T, Option<u64>> {
        let Some(commits) = &self.commits else {
            return Err(None);
        };
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(call, with.to_owned()))
            .filter(|(at, from, _)| !commits.moved_since(*at, from))
            .and_then(|(_, _, answer)| answer.downcast_ref::<T>().cloned())
            .ok_or_else(|| commits.settled())
    }

    /// Keep what `call` answered when asked `with`, read from what `on`
    /// declares, at the count of commits settled before it was asked, unless
    /// one was committed while it read.
    pub(super) fn keep<T: Send + Sync + 'static>(
        &self,
        call: &'static str,
        with: String,
        at: u64,
        on: &Reads,
        answer: T,
    ) {
        if self
            .commits
            .as_ref()
            .is_none_or(|commits| commits.committed() != at)
        {
            return;
        }
        self.answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                (call, with),
                (at, Arc::new(read_from(on)), Arc::new(answer)),
            );
    }
}

/// What an answer read from `on` is read from: `on`, and everything upstream
/// of it in the graph.
fn read_from(on: &Reads) -> BTreeSet<Node> {
    let mut from = crate::graph::graph().upstream(on);
    from.extend(on.nodes().cloned());
    from.extend(on.consulted().cloned());
    from
}

/// What the adapter `_` declares it reads.
fn declared<A: Input>(_: &A) -> Reads {
    A::reads()
}

/// `$port` for every [`Spanned`] adapter that implements it, each method
/// listed with its arguments and what its future gives.
///
/// The methods whose answer moves only when the store does are listed under
/// `held`, and each is answered from what it last answered while nothing has
/// been committed since ([`Answers`]).
///
/// The methods that return a value rather than a future are listed under
/// `at_once`, and passed through unspanned: they do not reach the store.
macro_rules! spanned {
    (
        $port:ident $(where $bound:path)? {
            $(type $associated:ident;)*
            $(
                fn $method:ident(&self $(, $argument:ident: $type:ty)* $(,)?) -> $output:ty;
            )*
        }
        $(
            held {
                $(
                    fn $held:ident(&self $(, $held_argument:ident: $held_type:ty)* $(,)?) -> Result<$answer:ty, $error:ty> on $on:expr;
                )*
            }
        )?
        $(
            at_once {
                $(
                    fn $now:ident(&self $(, $now_argument:ident: $now_type:ty)* $(,)?) -> $now_output:ty;
                )*
            }
        )?
    ) => {
        impl<A: $port $(+ $bound)?> $port for Spanned<A> {
            $(type $associated = A::$associated;)*
            $(
                fn $method(&self $(, $argument: $type)*) -> impl Future<Output = $output> + Send {
                    let span = tracing::info_span!(
                        "call",
                        call = concat!(stringify!($port), "::", stringify!($method)),
                        rows = Empty,
                    );
                    let call = self.0.$method($($argument),*);
                    async move {
                        let returned: $output = call.instrument(span.clone()).await;
                        if let Some(rows) = (&Returned(&returned)).rows() {
                            span.record("rows", rows);
                        }
                        returned
                    }
                }
            )*
            $($(
                fn $held(&self $(, $held_argument: $held_type)*) -> impl Future<Output = Result<$answer, $error>> + Send {
                    const CALL: &str = concat!(stringify!($port), "::", stringify!($held));
                    let with = format!("{} {:?}", std::any::type_name::<A>(), ($(&$held_argument,)*));
                    let on: Reads = ($on)(&self.0);
                    let kept = self.1.clone();
                    let call = self.0.$held($($held_argument),*);
                    async move {
                        let at = match kept.answered::<$answer>(CALL, &with) {
                            Ok(answer) => return Ok(answer),
                            Err(at) => at,
                        };
                        let span = tracing::info_span!("call", call = CALL, rows = Empty);
                        let returned: Result<$answer, $error> = call.instrument(span.clone()).await;
                        if let Some(rows) = (&Returned(&returned)).rows() {
                            span.record("rows", rows);
                        }
                        if let (Some(at), Ok(answer)) = (at, &returned) {
                            kept.keep(CALL, with, at, &on, answer.clone());
                        }
                        returned
                    }
                }
            )*)?
            $($(
                fn $now(&self $(, $now_argument: $now_type)*) -> $now_output {
                    self.0.$now($($now_argument),*)
                }
            )*)?
        }
    };
}

impl<A: Input> Input for Spanned<A> {
    fn reads() -> Reads {
        A::reads()
    }
}

impl<A: Output> Output for Spanned<A> {
    fn writes() -> BTreeSet<Table> {
        A::writes()
    }
}

spanned! {
    LandingStore {
        fn latest_digest(&self, id: &SourceRecordId) -> Result<Option<PayloadDigest>, StoreError>;
        fn append(&self, run: RunId, records: Vec<LandingRecord>) -> Result<RecordCount, StoreError>;
    }
    held {
            fn count(&self) -> Result<RecordCount, StoreError> on |adapter: &A| Reads::of([Node::Landing(adapter.stream().clone())]);
    }
    at_once {
            fn stream(&self) -> &LandingStream;
    }
}

spanned! {
    ResumptionPointStore {
        fn read(&self, stream: &LandingStream) -> Result<Option<Watermark>, StoreError>;
        fn advance(&self, stream: &LandingStream, to: Watermark, at: FetchedAt) -> Result<(), StoreError>;
        fn clear(&self, stream: &LandingStream) -> Result<(), StoreError>;
    }
}

spanned! {
    ExtractionRunLog {
        fn begin(&self, stream: &LandingStream, at: FetchedAt) -> Result<RunId, StoreError>;
        fn finish(&self, run: RunId, outcome: RunOutcome) -> Result<(), StoreError>;
        fn latest_success(&self, stream: &LandingStream) -> Result<Option<ExtractionRun>, StoreError>;
    }
}

spanned! {
    FetchLog {
    }
    held {
            fn fetched(&self, stream: &LandingStream) -> Result<Fetched, StoreError> on |_| Reads::of([Node::Landing(stream.clone())]);
    }
}

spanned! {
    RawExtent {
        fn records(&self) -> Result<RecordCount, StoreError>;
    }
}

spanned! {
    AccountReader {
        type Account;
        fn accounts(&self, reading: &Reading) -> Result<Reached<Self::Account>, StoreError>;
    }
    at_once {
            fn stream(&self) -> &LandingStream;
    }
}

spanned! {
    NormalisedEntityStore {
        type Entity;
        fn replace(&self, run: NormalisationRunId, entities: Vec<Self::Entity>) -> Result<WorkoutCount, StoreError>;
        fn revise(&self, run: NormalisationRunId, removed: Vec<LandingRecordId>, entities: Vec<Self::Entity>) -> Result<WorkoutCount, StoreError>;
        fn count(&self) -> Result<WorkoutCount, StoreError>;
    }
    at_once {
            fn stream(&self) -> &LandingStream;
    }
}

spanned! {
    NormalisedGymSessionReader where Input {
        fn all(&self) -> Result<Vec<NormalisedGymSession>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<NormalisedGymSession>, StoreError>;
    }
    held {
            fn written_by(&self) -> Result<DerivedFrom, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    CanonicalGymSessionStore {
        fn replace(&self, sessions: Vec<CanonicalGymSession>) -> Result<SessionCount, StoreError>;
        fn reach(&self) -> Result<Reach, StoreError>;
        fn replace_within(&self, reach: &Reach, sessions: Vec<CanonicalGymSession>) -> Result<SessionCount, StoreError>;
        fn all(&self) -> Result<Vec<CanonicalGymSession>, StoreError>;
        fn count(&self) -> Result<SessionCount, StoreError>;
    }
}

spanned! {
    NormalisedCyclingSessionReader where Input {
        fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
    }
    held {
            fn written_by(&self) -> Result<DerivedFrom, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    CanonicalCyclingSessionStore {
        fn replace(&self, sessions: Vec<CanonicalCyclingSession>) -> Result<SessionCount, StoreError>;
        fn reach(&self) -> Result<Reach, StoreError>;
        fn replace_within(&self, reach: &Reach, entries: Vec<CanonicalCyclingSession>) -> Result<SessionCount, StoreError>;
        fn count(&self) -> Result<SessionCount, StoreError>;
    }
}

spanned! {
    PerformedCyclingSessions {
        fn main_rides(&self) -> Result<Vec<CanonicalBikePlusRide>, StoreError>;
        fn efforts(&self) -> Result<Vec<CanonicalBikePlusRide>, StoreError>;
        fn tested(&self) -> Result<Vec<StartedAt>, StoreError>;
        fn between(&self, from: Date, to: Date) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
    }
}

spanned! {
    Activities {
        fn between(&self, from: Date, to: Date) -> Result<Vec<(Occurred, application::Activity)>, StoreError>;
        fn first(&self) -> Result<Option<Date>, StoreError>;
    }
}

spanned! {
    RefusalStore {
        fn replace(&self, run: NormalisationRunId, refusals: Vec<Refusal>) -> Result<RefusalCount, StoreError>;
        fn revise(&self, run: NormalisationRunId, removed: Vec<SourceRecordId>, refusals: Vec<Refusal>) -> Result<RefusalCount, StoreError>;
        fn all(&self) -> Result<Vec<Refusal>, StoreError>;
    }
    at_once {
            fn stream(&self) -> &LandingStream;
    }
}

spanned! {
    NormalisationRunLog {
        fn begin(&self, stream: &LandingStream, at: FetchedAt) -> Result<NormalisationRunId, StoreError>;
        fn finish(&self, run: NormalisationRunId, outcome: NormalisationOutcome) -> Result<(), StoreError>;
        fn latest_success(&self, stream: &LandingStream) -> Result<Option<NormalisationRun>, StoreError>;
        fn mark(&self, stream: &LandingStream) -> Result<Option<Mark>, StoreError>;
        fn record_mark(&self, run: NormalisationRunId, mark: &Mark) -> Result<(), StoreError>;
    }
}

spanned! {
    ExerciseHistory where Input {
        fn newest_performance(&self) -> Result<Option<Date>, StoreError>;
    }
    held {
            fn last_performances(&self, exercises: &[RepsExercise]) -> Result<BTreeMap<RepsExercise, LastPerformance>, StoreError> on |adapter| declared(adapter);
            fn performances(&self, exercise: RepsExercise) -> Result<Vec<(Performance, Option<FulfilledSession>)>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    Performances where Input {
    }
    held {
            fn of(&self, exercises: &[RepsExercise], from: Date, to: Date) -> Result<Vec<Performance>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    PerformedGymSessions {
        fn between(&self, from: Date, to: Date) -> Result<Vec<CanonicalGymSession>, StoreError>;
        fn fulfilling(&self, prescription: PrescribedWorkoutId) -> Result<Option<(DeliveryReference, CanonicalGymSession)>, StoreError>;
    }
}

spanned! {
    PerformedSessionLog where Input {
    }
    held {
            fn performed_between(&self, from: Date, to: Date) -> Result<Vec<Occurred>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    PlayedPadelMatches {
        fn between(&self, from: Date, to: Date) -> Result<Vec<PlayedPadelMatch>, StoreError>;
    }
}

spanned! {
    RiddenSessionLog where Input {
    }
    held {
            fn ridden_between(&self, from: Date, to: Date) -> Result<Vec<RiddenSession>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    WeighInHistory {
        fn weigh_ins(&self) -> Result<Vec<Weighed>, StoreError>;
    }
}

spanned! {
    OvernightHrvHistory {
        fn nights(&self) -> Result<Vec<domain::analytical::per_device::Recorded<domain::measure::HeartRateVariability>>, StoreError>;
    }
}

spanned! {
    RestingHeartRateHistory {
        fn days(&self) -> Result<Vec<domain::analytical::per_device::Recorded<domain::measure::BeatsPerMinute>>, StoreError>;
    }
}

spanned! {
    StressHistory {
        fn days(&self) -> Result<Vec<domain::analytical::per_device::Recorded<domain::analytical::stress::Measured>>, StoreError>;
    }
}

spanned! {
    TrainingDone {
        fn sessions(&self) -> Result<Vec<domain::analytical::training_done::Session>, StoreError>;
    }
}

spanned! {
    SleepHistory {
        fn nights(&self) -> Result<Vec<domain::analytical::per_device::Recorded<domain::analytical::sleep::Night>>, StoreError>;
    }
}

spanned! {
    PulseWaveVelocityHistory {
        fn measured(&self) -> Result<Vec<domain::analytical::pulse_wave_velocity::Measured>, StoreError>;
    }
}

spanned! {
    CompositionHistory {
        fn measured(&self) -> Result<Vec<domain::analytical::composition::Measured>, StoreError>;
    }
}

spanned! {
    NormalisedWeighInReader where Input {
        fn all(&self) -> Result<Vec<WeighInAccount>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<WeighInAccount>, StoreError>;
    }
    held {
            fn written_by(&self) -> Result<DerivedFrom, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    CanonicalWeighInStore {
        fn replace(&self, weigh_ins: Vec<CanonicalWeighIn>) -> Result<WeighInCount, StoreError>;
        fn reach(&self) -> Result<Reach, StoreError>;
        fn replace_within(&self, reach: &Reach, entries: Vec<CanonicalWeighIn>) -> Result<WeighInCount, StoreError>;
        fn count(&self) -> Result<WeighInCount, StoreError>;
    }
}

spanned! {
    WorkoutStore where Input {
        fn author_workout(&self, intensity: Relative, at: Timestamp, workout: &Workout) -> Result<(), StoreError>;
    }
    held {
            fn workout_at(&self, intensity: Relative, at: Timestamp) -> Result<Option<(Timestamp, Workout)>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    ProgressionStore where Input {
        fn author_linear(&self, at: Timestamp, linear: &LinearProgression) -> Result<(), StoreError>;
        fn author_sbs(&self, at: Timestamp, sbs: &SbsProgression) -> Result<(), StoreError>;
    }
    held {
            fn linear_at(&self, at: Timestamp) -> Result<Option<(Timestamp, LinearProgression)>, StoreError> on |adapter| declared(adapter);
            fn sbs_at(&self, at: Timestamp) -> Result<Option<(Timestamp, SbsProgression)>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    GenerationParameterStore where Input {
        fn bands_on(&self, specific: &domain::gym::equipment::Specific, date: jiff::civil::Date) -> Result<Option<domain::gym::equipment::BandRange>, StoreError>;
    }
    held {
            fn scales_on(&self, date: jiff::civil::Date) -> Result<Scales, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    EquipmentStore {
        fn facts(&self) -> Result<domain::gym::equipment::EquipmentFacts, StoreError>;
        fn record(&self, statement: &domain::gym::equipment::Statement, from: Option<jiff::civil::Date>, recorded_at: Timestamp, corrects: Option<domain::gym::equipment::FactId>) -> Result<domain::gym::equipment::FactId, StoreError>;
    }
}

spanned! {
    FillChangeStore {
        fn record_change(&self, mesocycle: domain::prescription::MesocycleId, change: &domain::prescription::FillChange) -> Result<(), StoreError>;
    }
}

spanned! {
    MesocycleStore {
        fn on(&self, date: Date) -> Result<Option<MesocycleInForce>, StoreError>;
        fn preceding(&self, date: Date) -> Result<Option<MesocycleInForce>, StoreError>;
        fn following(&self, date: Date) -> Result<Option<MesocycleInForce>, StoreError>;
        fn rerun_on(&self, date: Date) -> Result<Option<RerunOrdinal>, StoreError>;
        fn role_on(&self, date: Date) -> Result<Option<SessionRole>, StoreError>;
        fn carried_to(&self, date: Date) -> Result<Option<Date>, StoreError>;
        fn holds(&self, id: MesocycleId) -> Result<bool, StoreError>;
    }
}

spanned! {
    PlanStore where Input {
        fn windows(&self) -> Result<Vec<PlanWindow>, StoreError>;
        fn named(&self, name: &PlanName) -> Result<Option<Plan>, StoreError>;
        fn author(&self, plan: &Plan) -> Result<PlanId, StoreError>;
        fn commit(&self, plan: &PlanName, gym: &[GymMesocycle], cycling: &[CyclingMesocycle]) -> Result<(), StoreError>;
        fn complete(&self, plan: &PlanName, completed: &Completed) -> Result<(), StoreError>;
    }
    held {
            fn plans(&self) -> Result<Vec<Plan>, StoreError> on |adapter| declared(adapter);
            fn completed(&self, plan: &PlanName) -> Result<Option<Completed>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    FtpHistory where Input {
        fn series(&self) -> Result<Vec<Ftp>, StoreError>;
    }
    held {
            fn in_force_on(&self, date: Date) -> Result<Option<Ftp>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    RiddenVenues {
        fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError>;
    }
}

spanned! {
    CyclingMesocycleStore where Input {
    }
    held {
            fn following(&self, date: Date) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> on |adapter| declared(adapter);
            fn on(&self, date: Date) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    CyclingDeliveryStore where Input {
        fn record(&self, delivered: &DeliveredRide) -> Result<(), StoreError>;
    }
    held {
            fn delivered_for(&self, date: Date, destination: &DestinationName) -> Result<Option<DeliveredRide>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    PrescribedWorkoutStore where Input {
        fn issue(&self, workout: &PrescribedWorkout) -> Result<PrescribedWorkoutId, StoreError>;
        fn ordinal_in(&self, plan: &PlanName, date: Date) -> Result<Option<SessionOrdinal>, StoreError>;
    }
    held {
            fn issued_for(&self, date: Date) -> Result<Option<(PrescribedWorkoutId, PrescribedWorkout)>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    PrescriptionLifecycle {
        fn state_of(&self, prescription: PrescribedWorkoutId) -> Result<PrescriptionState, StoreError>;
    }
}

spanned! {
    PrescriptionDeliveryStore where Input {
        fn reference_for(&self, prescription: PrescribedWorkoutId, destination: &DestinationName) -> Result<Option<DeliveryReference>, StoreError>;
        fn record(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn record_rendering(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn hand_over(&self, from: PrescribedWorkoutId, to: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn record_reply(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reply: &DestinationReply, at: Timestamp) -> Result<(), StoreError>;
    }
    held {
            fn occupying(&self, date: Date, destination: &DestinationName) -> Result<Option<Occupant>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    HolidayCalendar where Input {
    }
    held {
            fn holidays(&self) -> Result<Holidays, SourceError> on |adapter| declared(adapter);
    }
}

spanned! {
    CalendarStore {
        fn keep(&self, calendar: Calendar, served: &Served, asked: Timestamp) -> Result<(), StoreError>;
        fn unchanged(&self, calendar: Calendar, asked: Timestamp) -> Result<(), StoreError>;
    }
    held {
            fn kept(&self, calendar: Calendar) -> Result<Option<Kept>, StoreError> on |_| Reads::of([Node::Generation(Generation::Calendar(calendar))]);
    }
}

spanned! {
    DiaryStore where Input {
    }
    held {
            fn diary(&self) -> Result<Diary, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    DiaryAuthor {
        fn record_pattern(&self, schedule: &TrainingPattern) -> Result<(), StoreError>;
        fn record_alteration(&self, alteration: &Alteration) -> Result<(), StoreError>;
        fn record_illness(&self, illness: &Illness) -> Result<(), StoreError>;
        fn record_skip(&self, skip: &Skip) -> Result<(), StoreError>;
    }
}

spanned! {
    RecordedDevices where Input {
    }
    held {
            fn sightings(&self) -> Result<Vec<domain::device::Sighting>, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    WearRecord where Input {
    }
    held {
            fn wear(&self) -> Result<domain::device::Wear, StoreError> on |adapter| declared(adapter);
    }
}

spanned! {
    Revisions {
    }
    held {
            fn revision(&self, node: &Node) -> Result<Revision, StoreError> on |_| Reads::of([node.clone()]);
    }
}

spanned! {
    DerivationLedger {
        fn record_normalised(&self, run: NormalisationRunId, from: &DerivedFrom) -> Result<(), StoreError>;
        fn record_canonical(&self, layer: CanonicalLayer, from: &DerivedFrom) -> Result<(), StoreError>;
    }
    held {
            fn normalised(&self, stream: &LandingStream) -> Result<Option<DerivedFrom>, StoreError> on |_| Reads::of([Node::Normalised(stream.clone())]);
            fn ever_normalised(&self, stream: &LandingStream) -> Result<bool, StoreError> on |_| Reads::of([Node::Normalised(stream.clone())]);
            fn canonical(&self, layer: CanonicalLayer) -> Result<Option<DerivedFrom>, StoreError> on |_| Reads::of([Node::Canonical(layer)]);
    }
}

spanned! {
    NewestObservation {
        fn newest_day(&self) -> Result<Option<Date>, StoreError>;
    }
}

spanned! {
    HoldingRides {
        fn candidates(&self, role: SessionRole) -> Result<Vec<RideVenue>, SourceError>;
        fn session_at(&self, venue: &RideVenue) -> Result<CyclingSession, SourceError>;
    }
}

spanned! {
    ClassSource {
        fn list(&self) -> Result<ClassesListed, SourceError>;
        fn read(&self) -> Result<ClassesRead, SourceError>;
        fn held(&self) -> Result<ClassesHeld, StoreError>;
    }
}

spanned! {
    PublishedProgrammes where Input {
    }
    held {
            fn read(&self, which: PublishedCycling) -> Result<PublishedRead, SourceError> on |adapter| declared(adapter);
    }
    at_once {
            fn which(&self, programme: &ExternalProgramme) -> Option<PublishedCycling>;
    }
}

spanned! {
    RideDestination {
        fn holding(&self) -> Result<usize, SourceError>;
        fn deliver(&self, rides: &domain::sequence::NonEmpty<RideVenue>) -> Result<application::RidesPlaced, SourceError>;
    }
}
