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
//! need to collect another one until the next day."* Each stream says what
//! it [`Expects`] (#612): Hevy, Peloton's rides and the watch's activities are
//! fetched while a session could have been performed, what is enumerated from
//! another stream once that one has landed something, and the watch's days
//! once yesterday is whole (#432). A page leaves alone anything fetched within
//! the shortest visit. A source that cannot be reached is a failed step, and
//! the run carries on with what is already landed (§ 36).
//!
//! **Then every derived layer is brought current, and only what moved is
//! derived.** The layers are a graph: a normalised layer reads its stream's
//! raw, its corrections and the zone, and a canonical layer reads the
//! normalised layers. Each records what it was derived from, and one whose
//! inputs read the same is left as it stands. The build is not a node in that
//! graph, but each layer's version is one of its inputs: a new build derives
//! only the layers whose translation it changed (#606).
//!
//! **Then each session of the microcycle gets a state**, and the first one
//! that can still be performed is the one delivered — again, if it already was
//! (#390).

use std::{
    collections::BTreeSet,
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
    time::{Duration, Instant},
};

use domain::{
    body::{LeftOut, WeighInCount},
    canonical::SessionCount,
    landing::{FetchedAt, LandingStream, RecordCount},
    macrocycle::{Concurrent, Macrocycle},
    normalised::{OperatorZone, WorkoutCount},
    plan::{MesocycleOrdinal, Plan, PlanName, Span},
    planner::{self, RerunOrdinal, SessionState},
    prescription::{GymMesocycle, schedule::WeekKind},
    schedule::{DayPart, Diary, Discipline, Holidays, ScheduledSlot, Skip},
};
use jiff::civil::Date;

use crate::{
    CanonicalCyclingSessionStore, CanonicalGymSessionStore, CanonicalWeighInStore, ClassSource,
    Clock, CyclingMesocycleStore, Delivery, DeliveryError, DiaryAuthor, DiaryStore,
    ExerciseHistory, ExtractionError, FetchLog, GenerationParameterStore, HolidayCalendar,
    MesocycleStore, NextLock, NormalisationError, NormalisedCyclingSessionReader,
    NormalisedGymSessionReader, NormalisedWeighInReader, Part, PlanStore, PrescribedWorkoutStore,
    Prescription, PrescriptionDeliverer as _, PrescriptionDeliveryStore, PrescriptionDestination,
    PrescriptionError, PrescriptionLifecycle, ProgressionStore, RideDestination, RidesPlaced,
    RunLockError, RunSummary, SourceError, StoreError, Taken, WorkoutExtractor,
    WorkoutPrescriber as _, WorkoutStore,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    commit::{CommitError, Due},
    cycling::{PrescribedRide, RideDeliveryError, RidingError},
    deliver::{Delivering, DeliveryPorts},
    microcycle::{Session, Standing},
    ports::{CanonicalLayer, DerivationLedger, DerivedFrom, NewestObservation, NormalisedLayers},
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

/// One walk of a source: the stream it lands in, the extraction that walks
/// it, and why it could hold something new.
pub struct Walk<E> {
    pub stream: LandingStream,
    pub extraction: E,
    pub expects: Expects,
}

/// Why a stream could hold something new, and so when a run fetches it
/// (#612).
///
/// The operator, 2026-10-29: *"a run is needed only where there is reason to
/// think a source has something new."* Of 185 fetches in the day before this,
/// 18 landed anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expects {
    /// Something new can arrive at any moment: a weigh-in, or the watch
    /// syncing a day. What waits for a whole day is a [`Daily`] around it.
    Anytime,
    /// What changes rarely, so is asked once a day: the account a source
    /// keeps.
    OnceADay,
    /// A session of the discipline performed: only while one is prescribed
    /// and not yet performed or skipped, or one could have been performed
    /// since the stream was last fetched.
    Sessions(Discipline),
    /// What is enumerated from another stream's records, so only once that
    /// stream has landed something since this one was last fetched.
    After(LandingStream),
}

/// Why a stream was not fetched this run (#612).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotDue {
    /// It holds the day it waits for, or was fetched today and is asked once
    /// a day.
    UntilTomorrow,
    /// Fetched less than [`Refetch::After`] ago, sooner than anything new can
    /// have been performed.
    Fetched { within: Duration },
    /// The stream it is enumerated from has landed nothing since.
    Follows(LandingStream),
    /// No session of the discipline could have been performed since it was
    /// last fetched.
    NoSession(Discipline),
}

impl std::fmt::Display for NotDue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UntilTomorrow => f.write_str("not due until tomorrow"),
            Self::Fetched { within } => {
                write!(f, "fetched in the last {} minutes", within.as_secs() / 60)
            }
            Self::Follows(stream) => write!(f, "nothing new from {stream}"),
            Self::NoSession(discipline) => write!(f, "no {discipline} session prescribed"),
        }
    }
}

/// How soon a run fetches again a stream that was fetched a moment ago.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refetch {
    /// Whenever it is due, as `fitness next` in the terminal does: running it
    /// is asking.
    WhenDue,
    /// Not within this long of the last fetch, as a page does.
    After(Duration),
}

impl Refetch {
    /// The shortest visit there can be: the operator, 2026-10-30, *"The
    /// shortest session I could do is a 30 Peloton class, followed by a 5
    /// minute cool down ride, with, say, 5 minutes each way to get to the
    /// bike."*
    pub const SHORTEST_VISIT: Self = Self::After(Duration::from_mins(45));
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

    /// Why walk `at` could hold something new.
    fn expects(&self, at: usize) -> Expects {
        let _ = at;
        Expects::Anytime
    }

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

/// A stream fetched once a day: once it holds the day it waits for, it is not
/// fetched again until tomorrow (#468, #432).
///
/// The operator, 2026-10-22: *"we don't need to fetch daily data if we've
/// already got data for today, in the case of Withings, or yesterday, in the
/// case of daily Garmin metrics."*
pub struct Daily<W, N> {
    pub walks: W,
    pub newest: N,
    pub awaits: Awaits,
}

/// Which day's observation a [`Daily`] stream waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Awaits {
    /// A weigh-in is whole the moment it is taken.
    Today,
    /// A watch's day is whole only once the next has begun and the watch has
    /// synced past it (#528).
    Yesterday,
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

    fn expects(&self, at: usize) -> Expects {
        self.walks.expects(at)
    }

    async fn due(&self, today: Date) -> Result<bool, StoreError> {
        let awaited = match self.awaits {
            Awaits::Today => Some(today),
            Awaits::Yesterday => today.yesterday().ok(),
        };
        // A day before the calendar's first is none to wait for.
        let Some(awaited) = awaited else {
            return Ok(true);
        };
        Ok(self
            .newest
            .newest_day()
            .await?
            .is_none_or(|newest| newest < awaited))
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

    fn expects(&self, _at: usize) -> Expects {
        self.expects.clone()
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

    fn expects(&self, at: usize) -> Expects {
        let first = self.0.count();
        if at < first {
            self.0.expects(at)
        } else {
            self.1.expects(at - first)
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

/// What a run collects and derives (#559).
///
/// **A page collects only what its charts read**, and a run of the whole of
/// `next` collects every part there is. The operator, 2026-10-23: *"Opening a
/// visualisation page should trigger a collection run for the metrics needed
/// for its visualisations."*
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scope {
    parts: BTreeSet<Part>,
}

impl Scope {
    pub fn new(parts: impl IntoIterator<Item = Part>) -> Self {
        Self {
            parts: parts.into_iter().collect(),
        }
    }

    pub const fn parts(&self) -> &BTreeSet<Part> {
        &self.parts
    }

    pub fn has(&self, part: &Part) -> bool {
        self.parts.contains(part)
    }

    /// Whether a run over `self` collects everything a run over `other` would.
    pub fn covers(&self, other: &Self) -> bool {
        other.parts.is_subset(&self.parts)
    }

    fn without(&self, joined: &BTreeSet<Part>) -> Self {
        Self {
            parts: self.parts.difference(joined).cloned().collect(),
        }
    }

    /// Whether an entry with no access is reported: where it is fetched, or
    /// where something else from its source is and no other entry of that
    /// source says so.
    ///
    /// **Once per source a page reads**, so a page reading Garmin's
    /// activities is not told the watch's days could not be fetched too.
    fn reports(&self, entry: &LandingStream, entries: &[&LandingStream]) -> bool {
        let fetched = |stream: &LandingStream| self.has(&Part::Fetching(stream.clone()));
        let from_source = self.parts.iter().any(
            |part| matches!(part, Part::Fetching(stream) if stream.source() == entry.source()),
        );
        let said_elsewhere = entries
            .iter()
            .any(|other| *other != entry && other.source() == entry.source() && fetched(other));
        fetched(entry) || (from_source && !said_elsewhere)
    }
}

/// What a run of the whole of `next` does when another is going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfGoing {
    /// Say so at once, as the terminal does.
    Refuse,
    /// Wait for it, as a page does: the operator, 2026-10-23, *"unless there
    /// is already one in progress, in which case, it should join and use
    /// that."*
    Join,
}

/// A step of the run, as it is reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Waiting for the runs collecting these parts to end, and using what they
    /// collected (#559).
    Joining(BTreeSet<Part>),
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
    /// The runs joined have ended.
    Joined,
    /// Records landed by a walk.
    Landed(RecordCount),
    /// A stream not fetched, and why.
    NotDue(NotDue),
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
    /// From the step starting to its ending, so the slowest is visible (#560).
    pub took: Duration,
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
    /// Another run of `next` delivered while this one waited for it, so this
    /// one used what it collected and delivered nothing.
    #[error("joined the run of next that was going, which has ended")]
    Joined,
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
    G: GenerationParameterStore + ProgressionStore + Sync,
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
    G: GenerationParameterStore + WorkoutStore + ProgressionStore + Clone + Send + Sync,
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
        let mesocycles = || {
            Rescheduled::new(
                self.mesocycles.clone(),
                self.plans.clone(),
                reschedule.clone(),
            )
        };
        let date = match crate::prescribe::next_session(&mesocycles(), from).await? {
            NextSession::On(date) => date,
            NextSession::NothingPlanned { last } => {
                return Ok(GymPrescribed::NothingPlanned { from, last });
            }
        };
        let issued = Prescribing::new(PrescriptionPorts {
            history: self.history.clone(),
            mesocycles: mesocycles(),
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
                mesocycles: mesocycles(),
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
        let written_by = match layer {
            CanonicalLayer::Gym => self.normalised_gym.written_by().await,
            CanonicalLayer::Cycling => self.normalised_cycling.written_by().await,
            CanonicalLayer::WeighIns => self.normalised_weigh_ins.written_by().await,
        }?;
        Ok(DerivedFrom::from(format!(
            "version {}; {}",
            layer.version(),
            written_by.as_str()
        )))
    }
}

/// Everything `next` reads, writes and drives.
pub struct NextPorts<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F> {
    pub gym_records: Records<GW>,
    pub cycling_records: Records<CW>,
    /// Body weight, which no discipline collects (#409, #357).
    pub weigh_in_records: Records<BW>,
    /// The activities the watch records, which no discipline collects either
    /// (#432), fetched while a gym session could have been performed (#612).
    pub watch_records: Records<WW>,
    /// The watch's days and nights, fetched until yesterday is whole (#432),
    /// after its activities and on the same sign-in.
    pub watch_days: Records<WD>,
    /// Every normalised layer, fetched this run or not.
    pub normalised: NL,
    /// Each canonical layer.
    pub canonical: CL,
    /// What each layer standing was derived from.
    pub ledger: DL,
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
    /// When each stream was last fetched, and the time now.
    pub fetches: F,
    /// Where a fetch's moment falls in the operator's day.
    pub zone: OperatorZone,
    /// How soon a stream fetched a moment ago is fetched again.
    pub refetch: Refetch,
}

/// The use case.
#[expect(
    clippy::type_complexity,
    reason = "one parameter per port the run drives; each stage is already behind a trait"
)]
pub struct Next<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F> {
    ports: NextPorts<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F>,
}

impl<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F>
    Next<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F>
where
    GW: Walks + Sync,
    CW: Walks + Sync,
    BW: Walks + Sync,
    WW: Walks + Sync,
    WD: Walks + Sync,
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
    F: FetchLog + Clock + Sync,
{
    #[expect(
        clippy::type_complexity,
        reason = "one parameter per port the run drives; each stage is already behind a trait"
    )]
    pub const fn new(
        ports: NextPorts<GW, CW, BW, WW, WD, NL, CL, DL, K, HC, D, P, M, C, GY, CY, L, F>,
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
        if_going: IfGoing,
        progress: &(impl Progress + Sync),
    ) -> Result<Report, NextError> {
        let mut run = Vec::new();
        // **Held for the whole run**, and taken before anything is read: a
        // second run that got as far as delivering would put a session at the
        // destination the first is about to replace.
        let everything = self.everything();
        let (_guard, joined) = self.take(&everything, if_going, progress, &mut run).await?;
        if joined.contains(&Part::Delivering) {
            return Err(NextError::Joined);
        }
        if self.ports.diary.diary().await?.patterns().is_empty() {
            return Err(NextError::NoWeek);
        }

        // 1. Every source at once, before anything is judged, and every layer
        // brought current.
        let judge = self.judge(now).await?;
        self.gather(&everything.without(&joined), &judge, progress, &mut run)
            .await?;

        // 2. The next concurrent mesocycle, if the one before it has ended.
        let holidays = self.ports.holidays.holidays().await;
        let standing = self.ports.microcycle.standing(now).await?;
        let diary = self.ports.diary.diary().await?;
        if let Ok(holidays) = &holidays
            && let Some((started, outcome)) = self
                .commit(now, holidays, &standing, &diary, progress)
                .await
        {
            finish(progress, &mut run, started, outcome);
        }

        // 3. Where the plan stands, after taking a skip if one was asked for.
        let mut standing = self.ports.microcycle.standing(now).await?;
        if let Some(reason) = skip {
            let started = start(progress, Step::Skipping);
            let found = self.skip(&standing, now, reason).await?;
            finish(progress, &mut run, started, Ok(found));
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

    /// Collect and derive only `scope`, joining any run collecting part of
    /// it, and deliver nothing (#559).
    ///
    /// The operator, 2026-10-23: *"A visualisation page is a visualisation
    /// page."*
    ///
    /// # Errors
    ///
    /// [`NextError`] if the store or a lock fails.
    pub async fn collect(
        &self,
        scope: &Scope,
        now: DayPart,
        progress: &(impl Progress + Sync),
    ) -> Result<Vec<Done>, NextError> {
        let mut run = Vec::new();
        let (_guard, joined) = self.take(scope, IfGoing::Join, progress, &mut run).await?;
        let judge = self.judge(now).await?;
        self.gather(&scope.without(&joined), &judge, progress, &mut run)
            .await?;
        Ok(run)
    }

    /// Whether a run over `scope` would do anything: fetch a stream that
    /// could hold something new, or bring a layer current (#612).
    ///
    /// **A page asks before it starts a run**, so one opened just after
    /// another run collected what it reads starts nothing and joins nothing.
    /// A stream enumerated from another is judged on what that one has
    /// landed so far, since nothing is fetched in asking.
    ///
    /// # Errors
    ///
    /// [`NextError`] if the store fails.
    pub async fn wanted(&self, scope: &Scope, now: DayPart) -> Result<bool, NextError> {
        let judge = self.judge(now).await?;
        for wanted in [
            self.due_in(&self.ports.gym_records, scope, &judge).await?,
            self.due_in(&self.ports.cycling_records, scope, &judge)
                .await?,
            self.due_in(&self.ports.weigh_in_records, scope, &judge)
                .await?,
            self.due_in(&self.ports.watch_records, scope, &judge)
                .await?,
            self.due_in(&self.ports.watch_days, scope, &judge).await?,
        ] {
            if wanted {
                return Ok(true);
            }
        }
        if scope.has(&Part::Delivering) {
            return Ok(true);
        }
        self.stale(scope).await
    }

    /// Whether any walk of one entry in `scope` is due.
    ///
    /// **An entry with no access is not**: asking would only say again that
    /// it cannot be fetched.
    async fn due_in<W: Walks + Sync>(
        &self,
        records: &Records<W>,
        scope: &Scope,
        judge: &Judge,
    ) -> Result<bool, NextError> {
        let Ok(walks) = &records.walks else {
            return Ok(false);
        };
        let wanted = wanted(walks, scope);
        if wanted.is_empty() || !walks.due(judge.now.date).await? {
            return Ok(false);
        }
        for at in wanted {
            if let Some(stream) = walks.stream(at)
                && self.not_due(walks, at, stream, judge).await?.is_none()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether a layer in `scope` would be derived again: from inputs that
    /// moved since it last was, its version among them.
    async fn stale(&self, scope: &Scope) -> Result<bool, NextError> {
        let ledger = &self.ports.ledger;
        for stream in self.ports.normalised.streams() {
            if scope.has(&Part::Normalising(stream.clone()))
                && ledger.normalised(&stream).await?.as_ref()
                    != Some(&self.ports.normalised.inputs(&stream).await?)
            {
                return Ok(true);
            }
        }
        for layer in CANONICAL {
            if scope.has(&Part::Canonicalising(layer))
                && ledger.canonical(layer).await?.as_ref()
                    != Some(&self.ports.canonical.inputs(layer).await?)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// What every stream is judged against this run: where the plan stands
    /// before anything is fetched, and the time now.
    async fn judge(&self, now: DayPart) -> Result<Judge, NextError> {
        Ok(Judge {
            standing: self.ports.microcycle.standing(now).await?,
            now,
            at: self.ports.fetches.now(),
        })
    }

    /// Why walk `at` of `walks`, landing in `stream`, is not fetched this run,
    /// or `None` where it is (#612).
    async fn not_due<W: Walks + Sync>(
        &self,
        walks: &W,
        at: usize,
        stream: &LandingStream,
        judge: &Judge,
    ) -> Result<Option<NotDue>, StoreError> {
        let log = &self.ports.fetches;
        let fetched = log.fetched(stream).await?;
        let expects = walks.expects(at);
        // **What follows another is fetched whenever that one has landed
        // something since**, however recently it was fetched itself: a new
        // ride's samples are not worth waiting for.
        if let Expects::After(parent) = &expects {
            let landed = log.fetched(parent).await?.landed;
            return Ok(match (landed, fetched.succeeded) {
                (Some(landed), Some(succeeded)) if landed <= succeeded => {
                    Some(NotDue::Follows(parent.clone()))
                }
                (None, Some(_)) => Some(NotDue::Follows(parent.clone())),
                _ => None,
            });
        }
        if let (Refetch::After(within), Some(succeeded)) = (self.ports.refetch, fetched.succeeded)
            && judge
                .at
                .as_timestamp()
                .duration_since(succeeded.as_timestamp())
                .unsigned_abs()
                < within
        {
            return Ok(Some(NotDue::Fetched { within }));
        }
        Ok(match expects {
            Expects::Anytime | Expects::After(_) => None,
            Expects::OnceADay => fetched
                .succeeded
                .filter(|succeeded| self.part(*succeeded).date >= judge.now.date)
                .map(|_| NotDue::UntilTomorrow),
            Expects::Sessions(discipline) => (!could_have_performed(
                &judge.standing,
                discipline,
                fetched.succeeded.map(|succeeded| self.part(succeeded)),
            ))
            .then_some(NotDue::NoSession(discipline)),
        })
    }

    /// The part of the operator's day a fetch fell in.
    fn part(&self, at: FetchedAt) -> DayPart {
        DayPart::containing(
            at.as_timestamp()
                .to_zoned(self.ports.zone.as_time_zone())
                .datetime(),
        )
    }

    /// Every part this build collects: each walk, each normalised and
    /// canonical layer, and delivering.
    ///
    /// **Each catalogue entry as well as its walks**, so an entry with no
    /// access, whose walks are unknown, is still reported.
    pub fn everything(&self) -> Scope {
        fn walks_of<W: Walks>(records: &Records<W>, parts: &mut BTreeSet<Part>) {
            parts.insert(Part::Fetching(records.entry.clone()));
            if let Ok(walks) = &records.walks {
                parts.extend(
                    (0..walks.count())
                        .filter_map(|at| walks.stream(at))
                        .map(|stream| Part::Fetching(stream.clone())),
                );
            }
        }
        let mut parts = BTreeSet::from([Part::Delivering]);
        walks_of(&self.ports.gym_records, &mut parts);
        walks_of(&self.ports.cycling_records, &mut parts);
        walks_of(&self.ports.weigh_in_records, &mut parts);
        walks_of(&self.ports.watch_records, &mut parts);
        walks_of(&self.ports.watch_days, &mut parts);
        parts.extend(
            self.ports
                .normalised
                .streams()
                .into_iter()
                .map(Part::Normalising),
        );
        parts.extend(CANONICAL.map(Part::Canonicalising));
        Scope { parts }
    }

    /// The name of each catalogue entry `next` fetches.
    const fn entries(&self) -> [&LandingStream; 5] {
        [
            &self.ports.gym_records.entry,
            &self.ports.cycling_records.entry,
            &self.ports.weigh_in_records.entry,
            &self.ports.watch_records.entry,
            &self.ports.watch_days.entry,
        ]
    }

    /// Take every part of `scope`, joining the runs that hold any of it.
    ///
    /// **All or none, then wait holding nothing**, so two runs waiting on each
    /// other cannot deadlock. Once the runs joined have ended the whole scope
    /// is taken again, the parts joined included: a canonical layer is not
    /// derived while another run re-derives a layer it reads.
    ///
    /// Returns the guard and the parts joined, which another run collected.
    async fn take(
        &self,
        scope: &Scope,
        if_going: IfGoing,
        progress: &(impl Progress + Sync),
        run: &mut Vec<Done>,
    ) -> Result<(L::Guard, BTreeSet<Part>), NextError> {
        let unavailable = |error| match error {
            RunLockError::Held => NextError::AlreadyRunning,
            RunLockError::Unavailable { detail } => {
                NextError::Store(StoreError::Unavailable { detail })
            }
        };
        let mut joined = BTreeSet::new();
        loop {
            match self
                .ports
                .lock
                .try_acquire(scope.parts())
                .map_err(unavailable)?
            {
                Taken::All(guard) => return Ok((guard, joined)),
                Taken::Held(held) => {
                    if if_going == IfGoing::Refuse && held.contains(&Part::Delivering) {
                        return Err(NextError::AlreadyRunning);
                    }
                    let started = start(progress, Step::Joining(held.clone()));
                    self.ports.lock.released(&held).await.map_err(unavailable)?;
                    finish(progress, run, started, Ok(Found::Joined));
                    joined.extend(held);
                }
            }
        }
    }

    /// Fetch every walk in `scope`, all sources at once, then bring the layers
    /// in it current.
    async fn gather(
        &self,
        scope: &Scope,
        judge: &Judge,
        progress: &(impl Progress + Sync),
        run: &mut Vec<Done>,
    ) -> Result<(), NextError> {
        // The operator, 2026-10-18: *"We should pull the new data in parallel
        // if we can."* Peloton's catalogue is refreshed after its rides, on
        // the same login, and only by a run that delivers.
        let (gym, cycling, weigh_ins, watch) = all(
            self.fetch(&self.ports.gym_records, scope, judge, progress),
            async {
                let mut done = self
                    .fetch(&self.ports.cycling_records, scope, judge, progress)
                    .await?;
                if scope.has(&Part::Delivering) {
                    let (started, outcome) = self.refresh(judge, progress).await;
                    finish(progress, &mut done, started, outcome);
                }
                Ok::<_, NextError>(done)
            },
            self.fetch(&self.ports.weigh_in_records, scope, judge, progress),
            // **The watch's days after its activities**, on the one sign-in:
            // Garmin is asked one request at a time.
            async {
                let mut done = self
                    .fetch(&self.ports.watch_records, scope, judge, progress)
                    .await?;
                done.extend(
                    self.fetch(&self.ports.watch_days, scope, judge, progress)
                        .await?,
                );
                Ok::<_, NextError>(done)
            },
        )
        .await;
        run.extend(gym?);
        run.extend(cycling?);
        run.extend(weigh_ins?);
        run.extend(watch?);

        // **Every layer is current before the standing is read**, and for the
        // gym that is the whole of #361: deriving a stream deletes its rows,
        // and a standing read inside that window once told the operator he had
        // lost every week.
        self.derive(scope, progress, run).await
    }

    /// Fetch the walks of one catalogue entry that are in `scope`, if it is
    /// due today.
    ///
    /// **Each walk is reported as it ends**, so a long one says it is done
    /// while the others are still going.
    async fn fetch<W: Walks + Sync>(
        &self,
        records: &Records<W>,
        scope: &Scope,
        judge: &Judge,
        progress: &(impl Progress + Sync),
    ) -> Result<Vec<Done>, NextError> {
        let mut run = Vec::new();
        let step = Step::Fetching(records.entry.clone());
        let walks = match &records.walks {
            Err(_) if !scope.reports(&records.entry, &self.entries()) => return Ok(run),
            Err(why) => {
                let started = start(progress, step);
                finish(
                    progress,
                    &mut run,
                    started,
                    Err(StepFailure::NoAccess(why.clone())),
                );
                return Ok(run);
            }
            Ok(walks) => walks,
        };
        let wanted = wanted(walks, scope);
        if wanted.is_empty() {
            return Ok(run);
        }
        if !walks.due(judge.now.date).await? {
            let started = start(progress, step);
            finish(
                progress,
                &mut run,
                started,
                Ok(Found::NotDue(NotDue::UntilTomorrow)),
            );
            return Ok(run);
        }
        for at in wanted {
            let Some(stream) = walks.stream(at) else {
                continue;
            };
            let started = start(progress, Step::Fetching(stream.clone()));
            if let Some(why) = self.not_due(walks, at, stream, judge).await? {
                finish(progress, &mut run, started, Ok(Found::NotDue(why)));
                continue;
            }
            let outcome = match walks.walk(at).await {
                Some(Ok(summary)) => Ok(Found::Landed(summary.records_landed)),
                Some(Err(error)) => Err(StepFailure::Extraction(error)),
                None => continue,
            };
            finish(progress, &mut run, started, outcome);
        }
        Ok(run)
    }

    /// Bring every derived layer current, deriving only what something new
    /// reached (#468) or a new version reads differently (#606).
    ///
    /// **Normalised layers first, then canonical**, because a canonical layer's
    /// inputs are the derivations that wrote the normalised layers it reads.
    ///
    /// **A layer that failed records nothing**, so the next run derives it
    /// again rather than passing it over as current.
    async fn derive(
        &self,
        scope: &Scope,
        progress: &(impl Progress + Sync),
        run: &mut Vec<Done>,
    ) -> Result<(), NextError> {
        let ledger = &self.ports.ledger;
        for stream in self.ports.normalised.streams() {
            if !scope.has(&Part::Normalising(stream.clone())) {
                continue;
            }
            let started = start(progress, Step::Normalising(stream.clone()));
            let inputs = self.ports.normalised.inputs(&stream).await?;
            let outcome = if ledger.normalised(&stream).await?.as_ref() == Some(&inputs) {
                Ok(Found::Unchanged)
            } else {
                match self.ports.normalised.normalise(&stream).await {
                    Ok(summary) => {
                        ledger.record_normalised(summary.run_id, &inputs).await?;
                        Ok(Found::Written(summary.workouts_written))
                    }
                    Err(error) => Err(StepFailure::Normalisation(error)),
                }
            };
            finish(progress, run, started, outcome);
        }

        for layer in CANONICAL {
            if !scope.has(&Part::Canonicalising(layer)) {
                continue;
            }
            let step = match layer {
                CanonicalLayer::Gym => Step::Canonicalising(Discipline::Gym),
                CanonicalLayer::Cycling => Step::Canonicalising(Discipline::Cycling),
                CanonicalLayer::WeighIns => Step::CanonicalisingWeighIns,
            };
            let started = start(progress, step);
            let inputs = self.ports.canonical.inputs(layer).await?;
            let found = if ledger.canonical(layer).await?.as_ref() == Some(&inputs) {
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
            finish(progress, run, started, Ok(found));
        }

        Ok(())
    }

    /// Keep the local class catalogue current (#246).
    ///
    /// **Stepped past, never fatal.** A catalogue one run out of date still
    /// answers every question asked of it, so a source that will not talk costs
    /// this week's new classes and nothing else.
    ///
    /// **Once a day** (#612), unless details are still to be read: the
    /// operator, 2026-10-30, lists Peloton's classes among what *"should only
    /// be fetched once a day"*.
    async fn refresh(
        &self,
        judge: &Judge,
        progress: &(impl Progress + Sync),
    ) -> (Started, Result<Found, StepFailure>) {
        let started = start(progress, Step::RefreshingClasses);
        let source = match &self.ports.classes {
            Err(why) => return (started, Err(StepFailure::NoAccess(why.clone()))),
            Ok(source) => source,
        };
        match source.held().await {
            Ok(held)
                if held.outstanding == 0
                    && held
                        .listed
                        .is_some_and(|listed| self.part(listed).date >= judge.now.date) =>
            {
                return (started, Ok(Found::NotDue(NotDue::UntilTomorrow)));
            }
            Ok(_) => {}
            Err(error) => {
                return (
                    started,
                    Err(StepFailure::Source(SourceError::Malformed {
                        detail: format!("the class catalogue could not be read: {error}"),
                    })),
                );
            }
        }
        let listed = match source.list().await {
            Ok(listed) => listed,
            Err(error) => return (started, Err(StepFailure::Source(error))),
        };
        if listed.outstanding > 0 {
            progress.estimated(&started.step, listed.outstanding, listed.reading_takes);
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
        (started, outcome)
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
    ) -> Option<(Started, Result<Found, StepFailure>)> {
        let due = match self
            .ports
            .committing
            .due(now, holidays, standing, diary)
            .await
        {
            Ok(Some(due)) => due,
            Ok(None) => return None,
            Err(error) => {
                let started = start(progress, Step::Committing);
                return Some((started, Err(StepFailure::Commit(error))));
            }
        };
        let started = start(progress, Step::Committing);
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
        Some((started, outcome))
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

/// Every canonical layer, in the order they are derived.
/// What every stream is judged against in one run (#612).
struct Judge {
    /// Where the plan stood before anything was fetched.
    standing: Standing,
    now: DayPart,
    at: FetchedAt,
}

/// The walks of `walks` that land in a stream `scope` fetches.
fn wanted<W: Walks>(walks: &W, scope: &Scope) -> Vec<usize> {
    (0..walks.count())
        .filter(|&at| {
            walks
                .stream(at)
                .is_some_and(|stream| scope.has(&Part::Fetching(stream.clone())))
        })
        .collect()
}

/// Whether a session of `discipline` could have been performed since a
/// stream was last fetched, in the part of the day `since` (#612).
///
/// The operator, 2026-10-30: *"hevy.workouts and garmin.activities only need
/// to be fetched if we've prescribed a gym session that hasn't been performed
/// or skipped yet."* **Also a session whose window closed after the last
/// fetch**, since one performed late in its window is read as not performed
/// until it is fetched, and the next session would be prescribed without it.
/// That includes a window that closed with nothing prescribed: collecting only
/// what had been prescribed is how a ride went unasked in September (#195).
/// **And anything before this microcycle began**, for the last session of the
/// week before, which the standing no longer shows.
///
/// Where there is no microcycle to read, or the stream was never fetched,
/// nothing says it could not have been.
fn could_have_performed(
    standing: &Standing,
    discipline: Discipline,
    since: Option<DayPart>,
) -> bool {
    let (Some(commencing), Some(since)) = (standing.commencing, since) else {
        return true;
    };
    since.date < commencing
        || standing
            .sessions
            .iter()
            .filter(|session| session.slot.discipline == discipline)
            .any(|session| match session.state {
                SessionState::Prescribed => true,
                SessionState::NotPerformed { .. } | SessionState::NotPrescribed => {
                    session.closes.is_none_or(|closes| since < closes)
                }
                SessionState::ToBePrescribed
                | SessionState::Performed
                | SessionState::Skipped { .. } => false,
            })
}

const CANONICAL: [CanonicalLayer; 3] = [
    CanonicalLayer::Gym,
    CanonicalLayer::Cycling,
    CanonicalLayer::WeighIns,
];

/// A step that has started, and when.
struct Started {
    step: Step,
    at: Instant,
}

/// Report a step's start.
fn start(progress: &impl Progress, step: Step) -> Started {
    progress.started(&step);
    Started {
        step,
        at: Instant::now(),
    }
}

/// Report a step's end, and keep it for the report.
fn finish(
    progress: &impl Progress,
    run: &mut Vec<Done>,
    started: Started,
    outcome: Result<Found, StepFailure>,
) {
    let done = Done {
        step: started.step,
        outcome,
        took: started.at.elapsed(),
    };
    progress.finished(&done);
    run.push(done);
}

/// Four futures run at once on the caller's task, and all four outcomes.
///
/// **By hand, because the core depends on no runtime.** Each is polled until it
/// is ready and never after, which is all a join is.
async fn all<A: Future, B: Future, C: Future, D: Future>(
    a: A,
    b: B,
    c: C,
    d: D,
) -> (A::Output, B::Output, C::Output, D::Output) {
    let (mut a, mut b, mut c, mut d) = (pin!(a), pin!(b), pin!(c), pin!(d));
    let (mut first, mut second, mut third, mut fourth) = (None, None, None, None);
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
        if fourth.is_none()
            && let Poll::Ready(output) = d.as_mut().poll(context)
        {
            fourth = Some(output);
        }
        match (first.take(), second.take(), third.take(), fourth.take()) {
            (Some(a), Some(b), Some(c), Some(d)) => Poll::Ready((a, b, c, d)),
            (a, b, c, d) => {
                (first, second, third, fourth) = (a, b, c, d);
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
