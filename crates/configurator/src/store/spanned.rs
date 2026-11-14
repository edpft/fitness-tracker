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

use std::collections::{BTreeMap, BTreeSet};

use application::{
    AccountReader, CalendarStore, CanonicalCyclingSessionStore, CanonicalGymSessionStore,
    CanonicalLayer, CanonicalWeighInStore, ClassSource, ClassesHeld, ClassesListed, ClassesRead,
    CompositionHistory, CyclingDeliveryStore, CyclingMesocycleStore, DeliveryReference,
    DerivationLedger, DerivedFrom, DestinationName, DestinationReply, DiaryAuthor, DiaryStore,
    EquipmentStore, ExerciseHistory, ExtractionRunLog, FetchLog, Fetched, FillChangeStore,
    FtpHistory, GenerationParameterStore, HoldingRides, HolidayCalendar, Input, Kept, LandingStore,
    LastPerformance, Mark, MesocycleInForce, MesocycleStore, NewestObservation,
    NormalisationRunLog, NormalisedCyclingSessionReader, NormalisedEntityStore,
    NormalisedGymSessionReader, NormalisedWeighInReader, Occupant, Output, OvernightHrvHistory,
    Performance, PerformedCyclingSessions, PerformedGymSessions, PerformedSessionLog, PlanStore,
    PlayedPadelGame, PlayedPadelGames, PrescribedWorkoutId, PrescribedWorkoutStore,
    PrescriptionDeliveryStore, PrescriptionLifecycle, ProgressionStore, PublishedCycling,
    PublishedProgrammes, PublishedRead, PulseWaveVelocityHistory, RawExtent, Reached, Reading,
    Reads, RecordedDevices, RefusalStore, RenderingDigest, RestingHeartRateHistory,
    ResumptionPointStore, RiddenSession, RiddenSessionLog, RiddenVenues, Served, SessionOrdinal,
    SleepHistory, SourceError, StoreError, StressHistory, TrainingDone, WearRecord, WeighInHistory,
    WorkoutStore,
    graph::{Calendar, Table},
};
use domain::analytical::Weighed;
use domain::body::{CanonicalWeighIn, WeighInAccount, WeighInCount};
use domain::canonical::{Occurred, Reach, SessionCount};
use domain::cycling::{
    CanonicalCyclingSession, CyclingMesocycle, CyclingMesocycleId, CyclingSession, DeliveredRide,
    Ftp, RideVenue,
};
use domain::gym::{CanonicalGymSession, NormalisedGymSession, exercise::RepsExercise};
use domain::landing::{
    ExtractionRun, FetchedAt, LandingRecord, LandingRecordId, LandingStream, PayloadDigest,
    RecordCount, RunId, RunOutcome, SourceRecordId, Watermark,
};
use domain::normalised::{
    NormalisationOutcome, NormalisationRun, NormalisationRunId, Refusal, RefusalCount, WorkoutCount,
};
use domain::plan::{Plan, PlanId, PlanName, PlanWindow};
use domain::planner::{Completed, Relative, RerunOrdinal, SessionRole};
use domain::prescription::{
    GymMesocycle, LinearProgression, MesocycleId, PrescribedWorkout, PrescriptionState,
    SbsProgression, Scales, Workout,
};
use domain::provider::ExternalProgramme;
use domain::schedule::{Alteration, Diary, Holidays, Illness, Skip, TrainingPattern};
use jiff::{Timestamp, civil::Date};
use tracing::{Instrument as _, field::Empty};

/// An adapter, each call through its ports a `call` span named for the port
/// and the method, with the rows it returned where it returned a list.
#[derive(Debug, Clone)]
pub struct Spanned<A>(A);

impl<A> Spanned<A> {
    pub(crate) const fn new(adapter: A) -> Self {
        Self(adapter)
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

/// `$port` for every [`Spanned`] adapter that implements it, each method
/// listed with its arguments and what its future gives.
///
/// The methods that return a value rather than a future are listed under
/// `at_once`, and passed through unspanned: they do not reach the store.
macro_rules! spanned {
    (
        $port:ident {
            $(type $associated:ident;)*
            $(
                fn $method:ident(&self $(, $argument:ident: $type:ty)* $(,)?) -> $output:ty;
            )*
        }
        $(
            at_once {
                $(
                    fn $now:ident(&self $(, $now_argument:ident: $now_type:ty)* $(,)?) -> $now_output:ty;
                )*
            }
        )?
    ) => {
        impl<A: $port> $port for Spanned<A> {
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
        fn count(&self) -> Result<RecordCount, StoreError>;
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
        fn fetched(&self, stream: &LandingStream) -> Result<Fetched, StoreError>;
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
    NormalisedGymSessionReader {
        fn all(&self) -> Result<Vec<NormalisedGymSession>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<NormalisedGymSession>, StoreError>;
        fn written_by(&self) -> Result<DerivedFrom, StoreError>;
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
    NormalisedCyclingSessionReader {
        fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
        fn written_by(&self) -> Result<DerivedFrom, StoreError>;
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
        fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError>;
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
    ExerciseHistory {
        fn last_performances(&self, exercises: &[RepsExercise]) -> Result<BTreeMap<RepsExercise, LastPerformance>, StoreError>;
        fn performances(&self, exercise: RepsExercise) -> Result<Vec<Performance>, StoreError>;
        fn newest_performance(&self) -> Result<Option<Date>, StoreError>;
    }
}

spanned! {
    PerformedGymSessions {
        fn between(&self, from: Date, to: Date) -> Result<Vec<CanonicalGymSession>, StoreError>;
        fn fulfilling(&self, prescription: PrescribedWorkoutId) -> Result<Option<(DeliveryReference, CanonicalGymSession)>, StoreError>;
    }
}

spanned! {
    PerformedSessionLog {
        fn performed_between(&self, from: Date, to: Date) -> Result<Vec<Occurred>, StoreError>;
    }
}

spanned! {
    PlayedPadelGames {
        fn all(&self) -> Result<Vec<PlayedPadelGame>, StoreError>;
    }
}

spanned! {
    RiddenSessionLog {
        fn ridden_between(&self, from: Date, to: Date) -> Result<Vec<RiddenSession>, StoreError>;
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
    NormalisedWeighInReader {
        fn all(&self) -> Result<Vec<WeighInAccount>, StoreError>;
        fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<WeighInAccount>, StoreError>;
        fn written_by(&self) -> Result<DerivedFrom, StoreError>;
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
    WorkoutStore {
        fn workout_at(&self, intensity: Relative, at: Timestamp) -> Result<Option<(Timestamp, Workout)>, StoreError>;
        fn author_workout(&self, intensity: Relative, at: Timestamp, workout: &Workout) -> Result<(), StoreError>;
    }
}

spanned! {
    ProgressionStore {
        fn linear_at(&self, at: Timestamp) -> Result<Option<(Timestamp, LinearProgression)>, StoreError>;
        fn author_linear(&self, at: Timestamp, linear: &LinearProgression) -> Result<(), StoreError>;
        fn sbs_at(&self, at: Timestamp) -> Result<Option<(Timestamp, SbsProgression)>, StoreError>;
        fn author_sbs(&self, at: Timestamp, sbs: &SbsProgression) -> Result<(), StoreError>;
    }
}

spanned! {
    GenerationParameterStore {
        fn scales_on(&self, date: jiff::civil::Date) -> Result<Scales, StoreError>;
        fn bands_on(&self, specific: &domain::gym::equipment::Specific, date: jiff::civil::Date) -> Result<Option<domain::gym::equipment::BandRange>, StoreError>;
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
    PlanStore {
        fn windows(&self) -> Result<Vec<PlanWindow>, StoreError>;
        fn named(&self, name: &PlanName) -> Result<Option<Plan>, StoreError>;
        fn author(&self, plan: &Plan) -> Result<PlanId, StoreError>;
        fn commit(&self, plan: &PlanName, gym: &[GymMesocycle], cycling: &[CyclingMesocycle]) -> Result<(), StoreError>;
        fn completed(&self, plan: &PlanName) -> Result<Option<Completed>, StoreError>;
        fn complete(&self, plan: &PlanName, completed: &Completed) -> Result<(), StoreError>;
    }
}

spanned! {
    FtpHistory {
        fn in_force_on(&self, date: Date) -> Result<Option<Ftp>, StoreError>;
    }
}

spanned! {
    RiddenVenues {
        fn ridden(&self) -> Result<BTreeSet<RideVenue>, StoreError>;
    }
}

spanned! {
    CyclingMesocycleStore {
        fn on(&self, date: Date) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError>;
        fn following(&self, date: Date) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError>;
    }
}

spanned! {
    CyclingDeliveryStore {
        fn delivered_for(&self, date: Date, destination: &DestinationName) -> Result<Option<DeliveredRide>, StoreError>;
        fn record(&self, delivered: &DeliveredRide) -> Result<(), StoreError>;
    }
}

spanned! {
    PrescribedWorkoutStore {
        fn issue(&self, workout: &PrescribedWorkout) -> Result<PrescribedWorkoutId, StoreError>;
        fn issued_for(&self, date: Date) -> Result<Option<(PrescribedWorkoutId, PrescribedWorkout)>, StoreError>;
        fn ordinal_in(&self, plan: &PlanName, date: Date) -> Result<Option<SessionOrdinal>, StoreError>;
    }
}

spanned! {
    PrescriptionLifecycle {
        fn state_of(&self, prescription: PrescribedWorkoutId) -> Result<PrescriptionState, StoreError>;
    }
}

spanned! {
    PrescriptionDeliveryStore {
        fn reference_for(&self, prescription: PrescribedWorkoutId, destination: &DestinationName) -> Result<Option<DeliveryReference>, StoreError>;
        fn occupying(&self, date: Date, destination: &DestinationName) -> Result<Option<Occupant>, StoreError>;
        fn record(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn record_rendering(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn hand_over(&self, from: PrescribedWorkoutId, to: PrescribedWorkoutId, destination: &DestinationName, reference: &DeliveryReference, rendering: &RenderingDigest, at: Timestamp) -> Result<(), StoreError>;
        fn record_reply(&self, prescription: PrescribedWorkoutId, destination: &DestinationName, reply: &DestinationReply, at: Timestamp) -> Result<(), StoreError>;
    }
}

spanned! {
    HolidayCalendar {
        fn holidays(&self) -> Result<Holidays, SourceError>;
    }
}

spanned! {
    CalendarStore {
        fn kept(&self, calendar: Calendar) -> Result<Option<Kept>, StoreError>;
        fn keep(&self, calendar: Calendar, served: &Served, asked: Timestamp) -> Result<(), StoreError>;
        fn unchanged(&self, calendar: Calendar, asked: Timestamp) -> Result<(), StoreError>;
    }
}

spanned! {
    DiaryStore {
        fn diary(&self) -> Result<Diary, StoreError>;
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
    RecordedDevices {
        fn sightings(&self) -> Result<Vec<domain::device::Sighting>, StoreError>;
    }
}

spanned! {
    WearRecord {
        fn wear(&self) -> Result<domain::device::Wear, StoreError>;
    }
}

spanned! {
    DerivationLedger {
        fn normalised(&self, stream: &LandingStream) -> Result<Option<DerivedFrom>, StoreError>;
        fn record_normalised(&self, run: NormalisationRunId, from: &DerivedFrom) -> Result<(), StoreError>;
        fn canonical(&self, layer: CanonicalLayer) -> Result<Option<DerivedFrom>, StoreError>;
        fn record_canonical(&self, layer: CanonicalLayer, from: &DerivedFrom) -> Result<(), StoreError>;
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
    PublishedProgrammes {
        fn read(&self, which: PublishedCycling) -> Result<PublishedRead, SourceError>;
    }
    at_once {
            fn which(&self, programme: &ExternalProgramme) -> Option<PublishedCycling>;
    }
}
