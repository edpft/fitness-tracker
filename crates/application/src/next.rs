//! `next`: collect everything, say where the plan stands, and deliver the
//! session that is next (#397).
//!
//! **A use case, so that any frontend can offer it.** It was orchestrated in
//! `cli`, which printed as it went; a browser could not have offered it without
//! reimplementing every step (§ 19). Now it reports progress through a port as
//! each step runs and returns a [`Report`], and a frontend renders both.
//!
//! The operator, 2026-09-19: *"This command is all about what's next, based on
//! what has happened and what should happen."* (#137)
//!
//! **Every source is collected first, all at once.** Deciding what to collect
//! from what the plan holds is how the bike went unasked: the store said
//! nothing was outstanding because nothing had been prescribed. So whether to
//! fetch is a question for each stream alone (#468) — the operator,
//! 2026-10-16: *"Once a daily Withings weigh in has been collected, we don't
//! need to collect another one until the next day."* Hevy and Peloton are
//! always fetched, because a session could have been completed in the last
//! hour. A source that cannot be reached is a failed step, and the run carries
//! on with what is already landed (§ 36).
//!
//! **Then every derived layer is brought current, and only what moved is
//! derived.** The layers are a graph: a normalised layer reads its stream's
//! raw, its corrections and the zone, and a canonical layer reads the
//! normalised layers. Each records what it was derived from, and one whose
//! inputs read the same is left as it stands. The build is not a node in that
//! graph: a build the store has not seen derives every layer once.
//!
//! **Then each session of the microcycle gets a state**, and the first one
//! that can still be performed is the one delivered — again, if it already was
//! (#390).

use std::{
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};

use domain::{
    body::{LeftOut, WeighInCount},
    canonical::SessionCount,
    landing::{LandingStream, RecordCount},
    macrocycle::{Concurrent, Macrocycle},
    normalised::WorkoutCount,
    plan::{MesocycleOrdinal, Plan, PlanName, Span},
    planner::{self, RerunOrdinal},
    prescription::{GymMesocycle, schedule::WeekKind},
    schedule::{DayPart, Diary, Discipline, Holidays, ScheduledSlot, Skip},
};
use jiff::civil::Date;

use crate::{
    CanonicalCyclingSessionStore, CanonicalGymSessionStore, CanonicalWeighInStore, ClassSource,
    CyclingMesocycleStore, Delivery, DeliveryError, DiaryAuthor, DiaryStore, ExerciseHistory,
    ExtractionError, GenerationParameterStore, HolidayCalendar, MesocycleStore, NextLock,
    NormalisationError, NormalisedCyclingSessionReader, NormalisedGymSessionReader,
    NormalisedWeighInReader, PlanStore, PrescribedWorkoutStore, Prescription,
    PrescriptionDeliverer as _, PrescriptionDeliveryStore, PrescriptionDestination,
    PrescriptionError, PrescriptionLifecycle, RideDestination, RidesPlaced, RunLockError,
    RunSummary, SourceError, StoreError, WorkoutExtractor, WorkoutPrescriber as _,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    commit::{CommitError, Due},
    cycling::{PrescribedRide, RideDeliveryError, RidingError},
    deliver::{Delivering, DeliveryPorts},
    microcycle::{Session, Standing},
    ports::{
        Build, CanonicalLayer, DerivationLedger, DerivedFrom, NewestObservation, NormalisedLayers,
    },
    prescribe::{NextSession, Prescribing, PrescriptionPorts},
    reschedule::{Reschedule, Rescheduled, ReschedulingPlans},
};

/// Why a source could not even be asked: no credential, or the wrong kind.
///
/// **Not a [`SourceError`]**, because nothing was sent. What a frontend needs
/// to say is how to give it one, and the composition root that found it
/// missing is the one that knows.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct NoAccess {
    pub detail: String,
}

/// One walk of a source: the stream it lands in, and the extraction that
/// walks it.
pub struct Walk<E> {
    pub stream: LandingStream,
    pub extraction: E,
}

/// The walks behind one catalogue entry, run in order.
///
/// **Each walk reports its own outcome** (#367): Peloton's rides are two walks
/// against two endpoints, and one failing must not hide the other.
pub trait Walks {
    /// How many walks there are.
    fn count(&self) -> usize;

    /// The stream walk `at` lands in.
    fn stream(&self, at: usize) -> Option<&LandingStream>;

    /// Run walk `at`.
    fn walk(
        &self,
        at: usize,
    ) -> impl Future<Output = Option<Result<RunSummary, ExtractionError>>> + Send;

    /// Whether to fetch at all on `today`. Always, unless the stream says
    /// otherwise, as [`Daily`] does.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if what the stream holds cannot be read.
    fn due(&self, today: Date) -> impl Future<Output = Result<bool, StoreError>> + Send {
        let _ = today;
        std::future::ready(Ok(true))
    }
}

/// A stream fetched once a day: once it holds an observation for today, it is
/// not fetched again until tomorrow (#468).
pub struct Daily<W, N> {
    pub walks: W,
    pub newest: N,
}

impl<W: Walks + Sync, N: NewestObservation + Sync> Walks for Daily<W, N> {
    fn count(&self) -> usize {
        self.walks.count()
    }

    fn stream(&self, at: usize) -> Option<&LandingStream> {
        self.walks.stream(at)
    }

    async fn walk(&self, at: usize) -> Option<Result<RunSummary, ExtractionError>> {
        self.walks.walk(at).await
    }

    async fn due(&self, today: Date) -> Result<bool, StoreError> {
        Ok(self
            .newest
            .newest_day()
            .await?
            .is_none_or(|newest| newest < today))
    }
}

impl<E: WorkoutExtractor + Sync> Walks for Walk<E> {
    fn count(&self) -> usize {
        1
    }

    fn stream(&self, at: usize) -> Option<&LandingStream> {
        (at == 0).then_some(&self.stream)
    }

    async fn walk(&self, at: usize) -> Option<Result<RunSummary, ExtractionError>> {
        if at != 0 {
            return None;
        }
        Some(self.extraction.extract().await)
    }
}

impl<A: Walks + Sync, B: Walks + Sync> Walks for (A, B) {
    fn count(&self) -> usize {
        self.0.count().saturating_add(self.1.count())
    }

    fn stream(&self, at: usize) -> Option<&LandingStream> {
        let first = self.0.count();
        if at < first {
            self.0.stream(at)
        } else {
            self.1.stream(at - first)
        }
    }

    async fn walk(&self, at: usize) -> Option<Result<RunSummary, ExtractionError>> {
        let first = self.0.count();
        if at < first {
            self.0.walk(at).await
        } else {
            self.1.walk(at - first).await
        }
    }
}

/// The walks that collect one catalogue entry.
///
/// **Not its derivation**, which is a layer of the graph every run brings
/// current whether or not this entry was fetched ([`NormalisedLayers`]).
pub struct Records<W> {
    /// The catalogue's name for what is collected, which a walk with no access
    /// is reported under.
    pub entry: LandingStream,
    pub walks: Result<W, NoAccess>,
}

/// A step of the run, as it is reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Fetching(LandingStream),
    Normalising(LandingStream),
    /// Rebuilding a discipline's canonical layer from the normalised one.
    Canonicalising(Discipline),
    /// Rebuilding the canonical weigh-ins (#409). Not a discipline: body
    /// weight has a source and no session to prescribe.
    CanonicalisingWeighIns,
    /// Keeping the local class catalogue current (#246).
    RefreshingClasses,
    /// Committing the next concurrent mesocycle (#222).
    Committing,
    /// Taking what is left of the current session's window (#391).
    Skipping,
}

/// What a step found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// Records landed by a walk.
    Landed(RecordCount),
    /// A stream that holds today's observation, so is not fetched until
    /// tomorrow.
    NotDue,
    /// Entities a normalised layer's derivation wrote.
    Written(WorkoutCount),
    /// A layer nothing new reached, left as it stands.
    Unchanged,
    /// Canonical sessions the record now holds.
    Canonical(SessionCount),
    /// Canonical weigh-ins the record now holds, and the weighings left out.
    CanonicalWeighIns {
        written: WeighInCount,
        left_out: Vec<LeftOut>,
    },
    Classes {
        /// Classes the source published since the last refresh.
        added: u64,
        /// Details read this run.
        read: u64,
        /// Details still to read.
        outstanding: u64,
        /// Why reading stopped early, where it did.
        stopped: Option<SourceError>,
    },
    Committed {
        concurrent: Concurrent,
        span: Span,
    },
    Skipped {
        session: Box<Session>,
        skip: Skip,
    },
}

/// Why a step failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StepFailure {
    #[error(transparent)]
    NoAccess(#[from] NoAccess),
    #[error(transparent)]
    Extraction(#[from] ExtractionError),
    /// **A layer that will not derive is a failed step**, and the derivation
    /// before it stands: it writes nothing when it fails.
    #[error(transparent)]
    Normalisation(#[from] NormalisationError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Commit(#[from] CommitError),
}

/// One step, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    pub step: Step,
    pub outcome: Result<Found, StepFailure>,
}

/// Where the run has got to, as it gets there.
///
/// **A port, because a run is long** (the first class refresh is ten minutes)
/// and a frontend has to be able to say what it is doing before the report
/// exists. A terminal prints a line; a browser pushes an event.
pub trait Progress {
    fn started(&self, step: &Step);
    /// How long the step now running will take, where that is worth saying.
    fn estimated(&self, step: &Step, outstanding: u64, takes: std::time::Duration);
    fn finished(&self, done: &Done);
}

/// Why `next` could not answer at all.
///
/// **Only what leaves nothing to report.** A source that cannot be reached or
/// a layer that will not derive is a failed step, and a session that cannot be
/// prescribed or delivered is in the report; this is the store failing, or a
/// skip that cannot be taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NextError {
    /// Another run of `next` is going, from this frontend or the other.
    #[error("a run of next is already going")]
    AlreadyRunning,
    /// No week has been recorded, so nothing says what is next.
    #[error("no week has been recorded, so nothing says what is next")]
    NoWeek,
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("{0}")]
    Skip(#[from] SkipRefused),
}

/// Why a skip could not be taken.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkipRefused {
    #[error("nothing in this microcycle can still be performed, so there is nothing to skip")]
    NothingToSkip,
    #[error("nothing is slotted after {discipline} {number}, so its window has no end to skip to")]
    NoEnd { discipline: Discipline, number: u8 },
    #[error("{detail}")]
    Invalid { detail: String },
}

/// Which week of the macrocycle this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacrocycleStanding {
    Week {
        macrocycle: Macrocycle,
        /// Counting from one.
        week: u32,
        of: u32,
    },
    /// Past the school calendar's reach: no data, which is not no term.
    Unknown { published_to: Option<Date> },
    /// The calendar could not be read.
    Unreadable(SourceError),
}

/// Where one discipline's mesocycle stands this microcycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MesocyclePosition {
    pub mesocycle: MesocycleOrdinal,
    pub week: WeekPosition,
}

/// Which week of its mesocycle a discipline is in.
///
/// **A line per discipline, because one can hold while the other reruns**
/// (#190), even though the two usually move in lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeekPosition {
    Microcycle {
        number: u32,
        of: u32,
        /// Which attempt at it this is, where the week before was lost.
        rerun: Option<RerunOrdinal>,
    },
    /// A week the discipline holds while the other reruns its test.
    Holding,
}

/// Where the plan stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStanding {
    pub macrocycle: MacrocycleStanding,
    /// One per discipline, in the order the week runs them. `None` where no
    /// mesocycle of it covers this microcycle.
    pub disciplines: Vec<(Discipline, Option<MesocyclePosition>)>,
    /// The Monday the microcycle commences, and every session of it.
    pub microcycle: Option<(Date, Vec<Session>)>,
}

/// A delivery that did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Undelivered<E> {
    #[error(transparent)]
    NoAccess(NoAccess),
    #[error(transparent)]
    Failed(E),
}

/// The gym's next session, prescribed and delivered.
#[derive(Debug)]
pub enum GymPrescribed {
    Issued {
        issued: Box<Prescription>,
        delivery: Result<Box<Delivery>, Undelivered<DeliveryError>>,
    },
    /// Nothing is programmed at or after the slot.
    NothingPlanned {
        from: Date,
        last: Option<(PlanName, Date)>,
    },
}

/// The cycling session that is next, prescribed and delivered.
#[derive(Debug)]
pub struct CyclingPrescribed {
    pub ride: Box<PrescribedRide>,
    pub delivery: Result<RidesPlaced, Undelivered<RideDeliveryError>>,
}

/// The prescription, whichever discipline the session is.
///
/// **Shown because it and what reaches the destination can diverge**, so it
/// is reported whether or not it was delivered.
#[derive(Debug)]
pub enum Prescribed {
    Gym(Result<GymPrescribed, PrescriptionError>),
    Cycling(Result<CyclingPrescribed, RidingError>),
}

/// What is next.
#[derive(Debug)]
pub enum WhatIsNext {
    Session {
        session: Box<Session>,
        prescribed: Prescribed,
    },
    /// Every session of the microcycle is settled.
    MicrocycleComplete { after: Option<ScheduledSlot> },
    /// The week has slots and no plan covers it.
    NoPlanCovers { week_of: Date },
    /// The schedule holds no slot on or after the date.
    NoSlot { from: Date },
}

/// What the run did, where the plan stands, and what is next.
#[derive(Debug)]
pub struct Report {
    pub run: Vec<Done>,
    pub plan: PlanStanding,
    pub next: WhatIsNext,
}

/// Where the plan stands at a moment, read from the store.
pub trait StandingReader {
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    fn standing(&self, now: DayPart) -> impl Future<Output = Result<Standing, StoreError>> + Send;
}

impl<D, P, G, C, Y, L> StandingReader for crate::microcycle::Microcycle<D, P, G, C, Y, L>
where
    Self: Sync,
    D: DiaryStore + Sync,
    P: PlanStore + Sync,
    G: PrescriptionDeliveryStore + Sync,
    C: crate::CyclingDeliveryStore + Sync,
    Y: crate::PerformedSessionLog + Sync,
    L: crate::RiddenSessionLog + Sync,
{
    async fn standing(&self, now: DayPart) -> Result<Standing, StoreError> {
        Self::standing(self, now).await
    }
}

/// Committing the next concurrent mesocycle.
pub trait MesocycleCommitter {
    /// # Errors
    ///
    /// [`CommitError`] if the plan cannot be read.
    fn due(
        &self,
        now: DayPart,
        holidays: &Holidays,
        standing: &Standing,
        diary: &Diary,
    ) -> impl Future<Output = Result<Option<Due>, CommitError>> + Send;

    /// # Errors
    ///
    /// [`CommitError`] if what is due will not build or cannot be written.
    fn commit(&self, due: &Due) -> impl Future<Output = Result<(), CommitError>> + Send;
}

impl<P, G, H, R, B> MesocycleCommitter for crate::commit::Committing<P, G, H, R, B>
where
    Self: Sync,
    P: PlanStore + Clone + Send + Sync,
    G: GenerationParameterStore + Sync,
    H: crate::HoldingRides + Sync,
    R: crate::RiddenVenues + Sync,
    B: crate::PublishedProgrammes + Sync,
{
    async fn due(
        &self,
        now: DayPart,
        holidays: &Holidays,
        standing: &Standing,
        diary: &Diary,
    ) -> Result<Option<Due>, CommitError> {
        Self::due(self, now, holidays, standing, diary).await
    }

    async fn commit(&self, due: &Due) -> Result<(), CommitError> {
        Self::commit(self, due).await
    }
}

/// The gym's sessions: which is next, prescribed, and delivered.
pub trait GymSessions {
    fn prescribed(
        &self,
        from: Date,
        reschedule: &Reschedule,
    ) -> impl Future<Output = Result<GymPrescribed, PrescriptionError>> + Send;
}

/// Everything the gym's next session is read from and written to.
pub struct GymPorts<X, M, P, G, W, L, V, T> {
    pub history: X,
    /// The gym's mesocycles as authored; read through the reschedule each call.
    pub mesocycles: M,
    pub plans: P,
    pub parameters: G,
    pub prescriptions: W,
    pub lifecycle: L,
    pub deliveries: V,
    pub destination: Result<T, NoAccess>,
}

impl<X, M, P, G, W, L, V, T> GymSessions for GymPorts<X, M, P, G, W, L, V, T>
where
    X: ExerciseHistory + Clone + Send + Sync,
    M: MesocycleStore + Clone + Send + Sync,
    P: PlanStore + Clone + Send + Sync,
    G: GenerationParameterStore + Clone + Send + Sync,
    W: PrescribedWorkoutStore + Clone + Send + Sync,
    L: PrescriptionLifecycle + Clone + Send + Sync,
    V: PrescriptionDeliveryStore + Clone + Send + Sync,
    T: PrescriptionDestination + Sync,
{
    /// **The first session at or after the slot, found once** (#122), and
    /// handed to both steps, so they cannot disagree about which session they
    /// mean. **Delivery reads what prescribing issued** rather than deriving
    /// again, so a destination being unreachable costs a retry rather than a
    /// ladder position (§ 36).
    async fn prescribed(
        &self,
        from: Date,
        reschedule: &Reschedule,
    ) -> Result<GymPrescribed, PrescriptionError> {
        let programmes = || {
            Rescheduled::new(
                self.mesocycles.clone(),
                self.plans.clone(),
                reschedule.clone(),
            )
        };
        let date = match crate::prescribe::next_session(&programmes(), from).await? {
            NextSession::On(date) => date,
            NextSession::NothingPlanned { last } => {
                return Ok(GymPrescribed::NothingPlanned { from, last });
            }
        };
        let issued = Prescribing::new(PrescriptionPorts {
            history: self.history.clone(),
            programmes: programmes(),
            parameters: self.parameters.clone(),
            prescriptions: self.prescriptions.clone(),
            lifecycle: self.lifecycle.clone(),
        })
        .prescribe(date)
        .await?;

        let delivery = match &self.destination {
            Err(why) => Err(Undelivered::NoAccess(why.clone())),
            Ok(destination) => Delivering::new(DeliveryPorts {
                prescriptions: self.prescriptions.clone(),
                programmes: programmes(),
                deliveries: self.deliveries.clone(),
                destination,
            })
            .deliver(date)
            .await
            .map(Box::new)
            .map_err(Undelivered::Failed),
        };
        Ok(GymPrescribed::Issued {
            issued: Box::new(issued),
            delivery,
        })
    }
}

/// Cycling's sessions: which ride is next, and delivered.
pub trait CyclingSessions {
    fn prescribed(
        &self,
        from: Date,
        reschedule: &Reschedule,
        diary: &Diary,
    ) -> impl Future<Output = Result<CyclingPrescribed, RidingError>> + Send;
}

/// The cycling use case and where its rides go.
pub struct CyclingPorts<U, T> {
    pub riding: U,
    pub destination: Result<T, NoAccess>,
}

impl<S, P, H, R, F, Y, T> CyclingSessions
    for CyclingPorts<crate::cycling::Riding<S, P, H, R, F, Y>, T>
where
    S: CyclingMesocycleStore + Clone + Send + Sync,
    P: PlanStore + Clone + Send + Sync,
    H: crate::HoldingRides + Sync,
    R: crate::RiddenVenues + Sync,
    F: crate::FtpHistory + Sync,
    Y: crate::CyclingDeliveryStore + Sync,
    T: RideDestination + Sync,
{
    /// **`next` replaces whatever the stack holds** (the operator, 2026-09-21:
    /// *"The default behaviour should be replace"*). What it replaced is said.
    async fn prescribed(
        &self,
        from: Date,
        reschedule: &Reschedule,
        diary: &Diary,
    ) -> Result<CyclingPrescribed, RidingError> {
        let ride = self
            .riding
            .next(from, reschedule.clone(), diary, None)
            .await?;
        let delivery = match &self.destination {
            Err(why) => Err(Undelivered::NoAccess(why.clone())),
            Ok(destination) => self
                .riding
                .deliver(&ride, destination, true)
                .await
                .map_err(Undelivered::Failed),
        };
        Ok(CyclingPrescribed {
            ride: Box::new(ride),
            delivery,
        })
    }
}

/// Rebuilding the canonical layers.
pub trait Canonicalising {
    /// # Errors
    ///
    /// [`StoreError`] if either layer cannot be read or written.
    fn gym(&self) -> impl Future<Output = Result<Canonicalised, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if either layer cannot be read or written.
    fn cycling(&self) -> impl Future<Output = Result<Canonicalised, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if either layer cannot be read or written.
    fn weigh_ins(&self) -> impl Future<Output = Result<WeighInsCanonicalised, StoreError>> + Send;

    /// What the layer would be derived from now: the derivations that wrote
    /// the normalised layers it reads.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the normalised layer cannot be read.
    fn inputs(
        &self,
        layer: CanonicalLayer,
    ) -> impl Future<Output = Result<DerivedFrom, StoreError>> + Send;
}

/// Each normalised layer, and the canonical layer built from it.
pub struct CanonicalPorts<GR, GC, CR, CC, WR, WC> {
    pub normalised_gym: GR,
    pub canonical_gym: GC,
    pub normalised_cycling: CR,
    pub canonical_cycling: CC,
    pub normalised_weigh_ins: WR,
    pub canonical_weigh_ins: WC,
}

impl<GR, GC, CR, CC, WR, WC> Canonicalising for CanonicalPorts<GR, GC, CR, CC, WR, WC>
where
    GR: NormalisedGymSessionReader + Sync,
    GC: CanonicalGymSessionStore + Sync,
    CR: NormalisedCyclingSessionReader + Sync,
    CC: CanonicalCyclingSessionStore + Sync,
    WR: NormalisedWeighInReader + Sync,
    WC: CanonicalWeighInStore + Sync,
{
    async fn gym(&self) -> Result<Canonicalised, StoreError> {
        crate::canonicalise::gym_sessions(&self.normalised_gym, &self.canonical_gym).await
    }

    async fn cycling(&self) -> Result<Canonicalised, StoreError> {
        crate::canonicalise::cycling_sessions(&self.normalised_cycling, &self.canonical_cycling)
            .await
    }

    async fn weigh_ins(&self) -> Result<WeighInsCanonicalised, StoreError> {
        crate::canonicalise::weigh_ins(&self.normalised_weigh_ins, &self.canonical_weigh_ins).await
    }

    async fn inputs(&self, layer: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
        match layer {
            CanonicalLayer::Gym => self.normalised_gym.written_by().await,
            CanonicalLayer::Cycling => self.normalised_cycling.written_by().await,
            CanonicalLayer::WeighIns => self.normalised_weigh_ins.written_by().await,
        }
    }
}

/// Everything `next` reads, writes and drives.
pub struct NextPorts<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L> {
    pub gym_records: Records<GW>,
    pub cycling_records: Records<CW>,
    /// Body weight, which no discipline collects (#409, #357).
    pub weigh_in_records: Records<BW>,
    /// Every normalised layer, fetched this run or not.
    pub normalised: NL,
    /// Each canonical layer.
    pub canonical: CL,
    /// What each layer standing was derived from.
    pub ledger: DL,
    /// This build of the derivation code. `None` for a build that cannot say
    /// which commit it is, which therefore derives every layer every run.
    pub build: Option<Build>,
    pub classes: Result<K, NoAccess>,
    pub holidays: HC,
    pub diary: D,
    /// The plan as authored, read through the reschedule.
    pub plans: P,
    pub microcycle: M,
    pub committing: C,
    pub gym: GY,
    pub cycling: CY,
    /// One run at a time, whichever frontend starts it.
    pub lock: L,
}

/// The use case.
#[expect(
    clippy::type_complexity,
    reason = "one parameter per port the run drives; each stage is already behind a trait"
)]
pub struct Next<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L> {
    ports: NextPorts<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L>,
}

impl<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L>
    Next<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L>
where
    GW: Walks + Sync,
    CW: Walks + Sync,
    BW: Walks + Sync,
    NL: NormalisedLayers + Sync,
    CL: Canonicalising + Sync,
    DL: DerivationLedger + Sync,
    K: ClassSource + Sync,
    HC: HolidayCalendar + Sync,
    D: DiaryStore + DiaryAuthor + Sync,
    P: PlanStore + Clone + Send + Sync,
    M: StandingReader + Sync,
    C: MesocycleCommitter + Sync,
    GY: GymSessions + Sync,
    CY: CyclingSessions + Sync,
    L: NextLock + Sync,
{
    #[expect(
        clippy::type_complexity,
        reason = "one parameter per port the run drives; each stage is already behind a trait"
    )]
    pub const fn new(
        ports: NextPorts<GW, CW, BW, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L>,
    ) -> Self {
        Self { ports }
    }

    /// Collect everything, say where the plan stands, and deliver what is next.
    ///
    /// `skip` takes what is left of the current session's window first, for the
    /// reason given (#391).
    ///
    /// # Errors
    ///
    /// [`NextError`] if no week has been recorded, if the store or a derivation
    /// fails, or if a skip cannot be taken.
    pub async fn run(
        &self,
        now: DayPart,
        skip: Option<&str>,
        progress: &(impl Progress + Sync),
    ) -> Result<Report, NextError> {
        // **Held for the whole run**, and taken before anything is read: a
        // second run that got as far as delivering would put a session at the
        // destination the first is about to replace.
        let _guard = self.ports.lock.try_acquire().map_err(|error| match error {
            RunLockError::Held => NextError::AlreadyRunning,
            RunLockError::Unavailable { detail } => {
                NextError::Store(StoreError::Unavailable { detail })
            }
        })?;
        if self.ports.diary.diary().await?.patterns().is_empty() {
            return Err(NextError::NoWeek);
        }
        let mut run = Vec::new();

        // 1. Every source at once, before anything is judged. The operator,
        // 2026-10-18: *"We should pull the new data in parallel if we can."*
        // Peloton's catalogue is refreshed after its rides, on the same login.
        let (gym, cycling, weigh_ins) = all(
            self.fetch(&self.ports.gym_records, now.date, progress),
            async {
                let mut done = self
                    .fetch(&self.ports.cycling_records, now.date, progress)
                    .await?;
                let (step, outcome) = self.refresh(progress).await;
                finish(progress, &mut done, step, outcome);
                Ok::<_, NextError>(done)
            },
            self.fetch(&self.ports.weigh_in_records, now.date, progress),
        )
        .await;
        run.extend(gym?);
        run.extend(cycling?);
        run.extend(weigh_ins?);

        // **Every layer is current before the standing is read**, and for the
        // gym that is the whole of #361: deriving a stream deletes its rows,
        // and a standing read inside that window once told the operator he had
        // lost every week.
        self.derive(progress, &mut run).await?;

        // 2. The next concurrent mesocycle, if the one before it has ended.
        let holidays = self.ports.holidays.holidays().await;
        let standing = self.ports.microcycle.standing(now).await?;
        let diary = self.ports.diary.diary().await?;
        if let Ok(holidays) = &holidays
            && let Some((step, outcome)) = self
                .commit(now, holidays, &standing, &diary, progress)
                .await
        {
            finish(progress, &mut run, step, outcome);
        }

        // 3. Where the plan stands, after taking a skip if one was asked for.
        let mut standing = self.ports.microcycle.standing(now).await?;
        if let Some(reason) = skip {
            progress.started(&Step::Skipping);
            let found = self.skip(&standing, now, reason).await?;
            finish(progress, &mut run, Step::Skipping, Ok(found));
            standing = self.ports.microcycle.standing(now).await?;
        }
        let diary = self.ports.diary.diary().await?;
        let reschedule = Reschedule::new(
            standing.reruns.clone(),
            diary.clone(),
            standing.missed.clone(),
        );
        let plan = self.plan(now, holidays, &standing, &reschedule).await?;

        // 4. The first session that can still be performed, prescribed or not.
        let next = self.next(now, &standing, &diary, &reschedule).await;
        Ok(Report { run, plan, next })
    }

    /// Fetch one catalogue entry, if it is due today.
    ///
    /// **Each walk is reported as it ends**, so a long one says it is done
    /// while the others are still going.
    async fn fetch<W: Walks + Sync>(
        &self,
        records: &Records<W>,
        today: Date,
        progress: &(impl Progress + Sync),
    ) -> Result<Vec<Done>, NextError> {
        let mut run = Vec::new();
        let step = Step::Fetching(records.entry.clone());
        let walks = match &records.walks {
            Err(why) => {
                progress.started(&step);
                finish(
                    progress,
                    &mut run,
                    step,
                    Err(StepFailure::NoAccess(why.clone())),
                );
                return Ok(run);
            }
            Ok(walks) => walks,
        };
        if !walks.due(today).await? {
            progress.started(&step);
            finish(progress, &mut run, step, Ok(Found::NotDue));
            return Ok(run);
        }
        for at in 0..walks.count() {
            let Some(stream) = walks.stream(at) else {
                continue;
            };
            let step = Step::Fetching(stream.clone());
            progress.started(&step);
            let outcome = match walks.walk(at).await {
                Some(Ok(summary)) => Ok(Found::Landed(summary.records_landed)),
                Some(Err(error)) => Err(StepFailure::Extraction(error)),
                None => continue,
            };
            finish(progress, &mut run, step, outcome);
        }
        Ok(run)
    }

    /// Bring every derived layer current, deriving only what something new
    /// reached (#468).
    ///
    /// **Normalised layers first, then canonical**, because a canonical layer's
    /// inputs are the derivations that wrote the normalised layers it reads.
    ///
    /// **The build is recorded only once every layer has derived under it**, so
    /// a layer that failed is derived again by the next run rather than passed
    /// over as current.
    async fn derive(
        &self,
        progress: &(impl Progress + Sync),
        run: &mut Vec<Done>,
    ) -> Result<(), NextError> {
        let ledger = &self.ports.ledger;
        let every = match &self.ports.build {
            None => true,
            Some(build) => ledger.build().await?.as_ref() != Some(build),
        };
        let mut failed = false;

        for stream in self.ports.normalised.streams() {
            let step = Step::Normalising(stream.clone());
            progress.started(&step);
            let inputs = self.ports.normalised.inputs(&stream).await?;
            let outcome = if !every && ledger.normalised(&stream).await?.as_ref() == Some(&inputs) {
                Ok(Found::Unchanged)
            } else {
                match self.ports.normalised.normalise(&stream).await {
                    Ok(summary) => {
                        ledger.record_normalised(summary.run_id, &inputs).await?;
                        Ok(Found::Written(summary.workouts_written))
                    }
                    Err(error) => {
                        failed = true;
                        Err(StepFailure::Normalisation(error))
                    }
                }
            };
            finish(progress, run, step, outcome);
        }

        for layer in [
            CanonicalLayer::Gym,
            CanonicalLayer::Cycling,
            CanonicalLayer::WeighIns,
        ] {
            let step = match layer {
                CanonicalLayer::Gym => Step::Canonicalising(Discipline::Gym),
                CanonicalLayer::Cycling => Step::Canonicalising(Discipline::Cycling),
                CanonicalLayer::WeighIns => Step::CanonicalisingWeighIns,
            };
            progress.started(&step);
            let inputs = self.ports.canonical.inputs(layer).await?;
            let found = if !every && ledger.canonical(layer).await?.as_ref() == Some(&inputs) {
                Found::Unchanged
            } else {
                let found = match layer {
                    CanonicalLayer::Gym => {
                        Found::Canonical(self.ports.canonical.gym().await?.written)
                    }
                    CanonicalLayer::Cycling => {
                        Found::Canonical(self.ports.canonical.cycling().await?.written)
                    }
                    CanonicalLayer::WeighIns => {
                        let canonicalised = self.ports.canonical.weigh_ins().await?;
                        Found::CanonicalWeighIns {
                            written: canonicalised.written,
                            left_out: canonicalised.left_out,
                        }
                    }
                };
                ledger.record_canonical(layer, &inputs).await?;
                found
            };
            finish(progress, run, step, Ok(found));
        }

        if let Some(build) = &self.ports.build
            && every
            && !failed
        {
            ledger.record_build(build).await?;
        }
        Ok(())
    }

    /// Keep the local class catalogue current (#246).
    ///
    /// **Stepped past, never fatal.** A catalogue one run out of date still
    /// answers every question asked of it, so a source that will not talk costs
    /// this week's new classes and nothing else.
    async fn refresh(
        &self,
        progress: &(impl Progress + Sync),
    ) -> (Step, Result<Found, StepFailure>) {
        let step = Step::RefreshingClasses;
        progress.started(&step);
        let source = match &self.ports.classes {
            Err(why) => return (step, Err(StepFailure::NoAccess(why.clone()))),
            Ok(source) => source,
        };
        let listed = match source.list().await {
            Ok(listed) => listed,
            Err(error) => return (step, Err(StepFailure::Source(error))),
        };
        if listed.outstanding > 0 {
            progress.estimated(&step, listed.outstanding, listed.reading_takes);
        }
        let outcome = source
            .read()
            .await
            .map(|read| Found::Classes {
                added: listed.added,
                read: read.read,
                outstanding: read.outstanding,
                stopped: read.stopped,
            })
            .map_err(StepFailure::Source);
        (step, outcome)
    }

    /// Commit the next concurrent mesocycle, where one is due.
    ///
    /// `None` where nothing is due, which is not a step worth a line.
    async fn commit(
        &self,
        now: DayPart,
        holidays: &Holidays,
        standing: &Standing,
        diary: &Diary,
        progress: &(impl Progress + Sync),
    ) -> Option<(Step, Result<Found, StepFailure>)> {
        let step = Step::Committing;
        let due = match self
            .ports
            .committing
            .due(now, holidays, standing, diary)
            .await
        {
            Ok(Some(due)) => due,
            Ok(None) => return None,
            Err(error) => return Some((step, Err(StepFailure::Commit(error)))),
        };
        progress.started(&step);
        let outcome = self
            .ports
            .committing
            .commit(&due)
            .await
            .map(|()| Found::Committed {
                concurrent: due.concurrent,
                span: due.span,
            })
            .map_err(StepFailure::Commit);
        Some((step, outcome))
    }

    /// Take what is left of the current session's window (#391).
    ///
    /// **From now, not from the slot.** The operator, 2026-10-10: on a Monday
    /// afternoon with the gym that evening and a ride on the Wednesday evening,
    /// a skip takes every part from the Monday afternoon to the Wednesday
    /// afternoon.
    async fn skip(
        &self,
        standing: &Standing,
        now: DayPart,
        reason: &str,
    ) -> Result<Found, NextError> {
        let current = standing.current().ok_or(SkipRefused::NothingToSkip)?;
        let until = current.closes.ok_or(SkipRefused::NoEnd {
            discipline: current.slot.discipline,
            number: current.number,
        })?;
        let skip = Skip::new(now, until, reason).map_err(|error| SkipRefused::Invalid {
            detail: error.to_string(),
        })?;
        self.ports.diary.record_skip(&skip).await?;
        Ok(Found::Skipped {
            session: Box::new(current.clone()),
            skip,
        })
    }

    /// Where the plan stands: the macrocycle's week, each discipline's
    /// mesocycle, and the microcycle's sessions.
    async fn plan(
        &self,
        now: DayPart,
        holidays: Result<Holidays, SourceError>,
        standing: &Standing,
        reschedule: &Reschedule,
    ) -> Result<PlanStanding, NextError> {
        let macrocycle = match holidays {
            Err(error) => MacrocycleStanding::Unreadable(error),
            Ok(holidays) => Macrocycle::on(now.date, &holidays).map_or_else(
                || MacrocycleStanding::Unknown {
                    published_to: holidays.school_published_to(),
                },
                |macrocycle| MacrocycleStanding::Week {
                    week: weeks_between(macrocycle.start(), now.date).saturating_add(1),
                    of: weeks_between(macrocycle.start(), macrocycle.last()).saturating_add(1),
                    macrocycle,
                },
            ),
        };

        let plans = ReschedulingPlans::new(self.ports.plans.clone(), reschedule.clone());
        let plan = match standing.commencing {
            Some(monday) => covering(&plans, monday).await?,
            None => None,
        };
        let disciplines = [Discipline::Gym, Discipline::Cycling]
            .into_iter()
            .map(|discipline| {
                let position = plan.as_ref().and_then(|plan| {
                    let date = standing
                        .sessions
                        .iter()
                        .find(|session| session.slot.discipline == discipline)
                        .map(|session| session.slot.date)?;
                    position(plan, discipline, date, &standing.reruns)
                });
                (discipline, position)
            })
            .collect();

        Ok(PlanStanding {
            macrocycle,
            disciplines,
            microcycle: standing
                .commencing
                .filter(|_| !standing.sessions.is_empty())
                .map(|monday| (monday, standing.sessions.clone())),
        })
    }

    /// What is next, prescribed and delivered where it can be.
    async fn next(
        &self,
        now: DayPart,
        standing: &Standing,
        diary: &Diary,
        reschedule: &Reschedule,
    ) -> WhatIsNext {
        if standing.sessions.is_empty() {
            // **Two empty answers, and they are different facts.** A week the
            // diary holds nothing for is a week off; a week it holds slots for
            // that no plan covers is a plan to author.
            let monday = standing
                .commencing
                .unwrap_or_else(|| planner::commencing(now.date));
            let has_slots = (0..7)
                .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
                .any(|date| !diary.ordinary_slots_of(date).is_empty());
            return if has_slots {
                WhatIsNext::NoPlanCovers { week_of: monday }
            } else {
                WhatIsNext::NoSlot { from: now.date }
            };
        }
        let Some(session) = standing.current() else {
            let after = standing
                .sessions
                .last()
                .and_then(|last| diary.first_ordinary_after(last.slot.date));
            return WhatIsNext::MicrocycleComplete { after };
        };

        let from = session.slot.date;
        let prescribed = match session.slot.discipline {
            Discipline::Gym => Prescribed::Gym(self.ports.gym.prescribed(from, reschedule).await),
            Discipline::Cycling => {
                Prescribed::Cycling(self.ports.cycling.prescribed(from, reschedule, diary).await)
            }
        };
        WhatIsNext::Session {
            session: Box::new(session.clone()),
            prescribed,
        }
    }
}

/// Report a step's end, and keep it for the report.
fn finish(
    progress: &impl Progress,
    run: &mut Vec<Done>,
    step: Step,
    outcome: Result<Found, StepFailure>,
) {
    let done = Done { step, outcome };
    progress.finished(&done);
    run.push(done);
}

/// Three futures run at once on the caller's task, and all three outcomes.
///
/// **By hand, because the core depends on no runtime.** Each is polled until it
/// is ready and never after, which is all a join is.
async fn all<A: Future, B: Future, C: Future>(
    a: A,
    b: B,
    c: C,
) -> (A::Output, B::Output, C::Output) {
    let (mut a, mut b, mut c) = (pin!(a), pin!(b), pin!(c));
    let (mut first, mut second, mut third) = (None, None, None);
    poll_fn(|context| {
        if first.is_none()
            && let Poll::Ready(output) = a.as_mut().poll(context)
        {
            first = Some(output);
        }
        if second.is_none()
            && let Poll::Ready(output) = b.as_mut().poll(context)
        {
            second = Some(output);
        }
        if third.is_none()
            && let Poll::Ready(output) = c.as_mut().poll(context)
        {
            third = Some(output);
        }
        match (first.take(), second.take(), third.take()) {
            (Some(a), Some(b), Some(c)) => Poll::Ready((a, b, c)),
            (a, b, c) => {
                (first, second, third) = (a, b, c);
                Poll::Pending
            }
        }
    })
    .await
}

/// The plan covering a date, as it now stands.
async fn covering<P: PlanStore + Sync>(
    plans: &ReschedulingPlans<P>,
    date: Date,
) -> Result<Option<Plan>, StoreError> {
    let windows = plans.windows().await?;
    let Some(window) = windows.iter().find(|window| window.span().covers(date)) else {
        return Ok(None);
    };
    plans.named(window.name()).await
}

/// Which mesocycle of the plan a discipline is in on a date, and which week of
/// it.
///
/// **The mesocycle is the plan's number** ([`MesocycleOrdinal`]), and **the
/// week is the discipline's own calendar's**: a week lost or held is not a
/// microcycle of the mesocycle, so a rerun reads as the same microcycle again.
fn position(
    plan: &Plan,
    discipline: Discipline,
    date: Date,
    reruns: &[planner::Rerun],
) -> Option<MesocyclePosition> {
    let rerun = planner::rerun_of(reruns, discipline, date);
    let (index, week) = match discipline {
        Discipline::Gym => {
            let (index, mesocycle) = plan.gym()?.on(date)?;
            (index, gym_week(mesocycle, date, rerun)?)
        }
        Discipline::Cycling => {
            let (index, mesocycle) = plan.cycling()?.on(date)?;
            let week = if mesocycle.holds_on(date) {
                WeekPosition::Holding
            } else {
                WeekPosition::Microcycle {
                    number: u32::try_from(mesocycle.microcycle_of(date)?).ok()?,
                    of: u32::try_from(mesocycle.duration_weeks()).ok()?,
                    rerun,
                }
            };
            (index, week)
        }
    };
    let mesocycle = MesocycleOrdinal::new(u32::try_from(index).ok()?.checked_add(1)?).ok()?;
    Some(MesocyclePosition { mesocycle, week })
}

fn gym_week(
    mesocycle: &GymMesocycle,
    date: Date,
    rerun: Option<RerunOrdinal>,
) -> Option<WeekPosition> {
    let calendar = mesocycle.calendar();
    let (kind, _) = calendar.place(date).ok()?;
    if kind == WeekKind::Holding {
        return Some(WeekPosition::Holding);
    }
    Some(WeekPosition::Microcycle {
        number: calendar.microcycle(kind)?.as_u32(),
        of: calendar.duration_weeks(),
        rerun,
    })
}

/// Whole weeks from one Monday to the week a date falls in.
fn weeks_between(start: Date, date: Date) -> u32 {
    let days = (date - start).get_days().max(0);
    u32::try_from(days / 7).unwrap_or(u32::MAX)
}
