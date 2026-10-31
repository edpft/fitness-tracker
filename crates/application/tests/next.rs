//! `next` as a use case (#397): what it reports, and what it carries on past.
//!
//! **Every port is a test adapter.** What a walk lands, what a derivation
//! writes and which state a session is in have suites of their own; here the
//! question is the run: which steps it reports, in what order, and that a
//! source it cannot reach costs that step and nothing else (§ 36).

use std::{
    collections::{BTreeMap, BTreeSet},
    future::poll_fn,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
};

use application::{
    CanonicalLayer, ClassSource, ClassesHeld, ClassesListed, ClassesRead, Clock, DerivationLedger,
    DerivedFrom, DiaryAuthor, DiaryStore, ExtractionError, FetchLog, Fetched, HolidayCalendar,
    NewestObservation, NextLock, NormalisationError, NormalisationSummary, NormalisedLayers, Part,
    PlanStore, PrescriptionError, Rederivation, RunLockError, RunSummary, SourceError, StoreError,
    Taken,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    commit::{CommitError, Due},
    cycling::RidingError,
    microcycle::{Session, Standing},
    next::{
        Awaits, Canonicalising, CyclingPrescribed, CyclingSessions, Daily, Done, Expects, Found,
        GymPrescribed, GymSessions, IfGoing, MesocycleCommitter, Next, NextError, NextPorts,
        NoAccess, NotDue, Prescribed, Progress, Records, Refetch, Report, Scope, SkipRefused,
        StandingReader, Step, StepFailure, Walks, WhatIsNext,
    },
    reschedule::Reschedule,
};
use domain::{
    body::WeighInCount,
    canonical::SessionCount,
    cycling::CyclingMesocycle,
    landing::{FetchedAt, LandingStream, RecordCount, RunId},
    normalised::{NormalisationRunId, OperatorZone, RefusalCount, WorkoutCount},
    plan::{Plan, PlanId, PlanName, PlanWindow},
    planner::SessionState,
    prescription::GymMesocycle,
    schedule::{
        Allocation, DayPart, Diary, Discipline, Holidays, PartOfDay, Relative, ScheduledSlot,
        SessionRole, Skip, TrainingPattern, TrainingSlot,
    },
};
use jiff::civil::{Date, Weekday};

const HARDER: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);

fn stream(name: &str) -> Result<LandingStream, Box<dyn std::error::Error>> {
    Ok(LandingStream::try_from(name)?)
}

/// Walks that answer as they were told to, in order.
///
/// **A walk that is met waits for the others** behind the same meeting point,
/// and fails if they never arrive: fetched one after another, the first would
/// wait alone.
///
/// **Each walk expects what it was told to**, and anything at all where it was
/// told nothing.
struct Walking(
    Vec<(LandingStream, Result<RunSummary, ExtractionError>)>,
    Option<Arc<AtomicUsize>>,
    Vec<Expects>,
);

impl Walking {
    fn landing(stream: LandingStream, records: usize) -> Self {
        Self(vec![(stream, Ok(landed(records)))], None, Vec::new())
    }

    /// Each stream landing one record, expecting what it is paired with.
    fn expecting(walks: Vec<(LandingStream, Expects)>) -> Self {
        let (streams, expects): (Vec<_>, Vec<_>) = walks.into_iter().unzip();
        Self(
            streams
                .into_iter()
                .map(|stream| (stream, Ok(landed(1))))
                .collect(),
            None,
            expects,
        )
    }
}

/// How many walks meet, and how long one waits for the rest.
const MEETING: usize = 4;
const PATIENCE: usize = 100;

impl Walks for Walking {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn stream(&self, at: usize) -> Option<&LandingStream> {
        self.0.get(at).map(|(stream, _)| stream)
    }

    async fn walk(&self, at: usize) -> Option<Result<RunSummary, ExtractionError>> {
        if let Some(met) = &self.1 {
            met.fetch_add(1, Ordering::SeqCst);
            let mut waited = 0;
            let together = poll_fn(|context| {
                if met.load(Ordering::SeqCst) >= MEETING {
                    Poll::Ready(true)
                } else if waited >= PATIENCE {
                    Poll::Ready(false)
                } else {
                    waited += 1;
                    context.waker().wake_by_ref();
                    Poll::Pending
                }
            })
            .await;
            if !together {
                return Some(Err(ExtractionError::Source(SourceError::Unavailable {
                    detail: "fetched alone".to_owned(),
                })));
            }
        }
        self.0.get(at).map(|(_, result)| result.clone())
    }

    fn expects(&self, at: usize) -> Expects {
        self.2.get(at).cloned().unwrap_or(Expects::Anytime)
    }
}

fn landed(records: usize) -> RunSummary {
    RunSummary {
        run_id: RunId::from(1),
        events_seen: records.into(),
        records_landed: RecordCount::from(records),
        resumption_point: None,
        resumption_point_moved: false,
    }
}

/// The newest day a stream holds, as the test sets it.
struct Newest(Option<Date>);

impl NewestObservation for Newest {
    async fn newest_day(&self) -> Result<Option<Date>, StoreError> {
        Ok(self.0)
    }
}

/// The derived layers, in memory: how far each stream's inputs reach, which
/// run last derived each, and what the ledger says each was derived from.
///
/// **One graph behind every port that touches it**, so a canonical layer's
/// inputs move exactly when a normalisation it reads from runs, as the store's
/// do.
#[derive(Default)]
struct Graph(Mutex<Layered>);

#[derive(Default)]
struct Layered {
    /// What each stream's normalised layer would be derived from now.
    inputs: BTreeMap<String, u32>,
    /// The run that last derived each stream.
    runs: BTreeMap<String, u64>,
    last_run: u64,
    /// Streams that will not derive.
    failing: Vec<String>,
    normalised: Vec<String>,
    canonicalised: Vec<CanonicalLayer>,
    /// The canonical layers rebuilt only where something changed (#610).
    reached: Vec<CanonicalLayer>,
    derived_from: BTreeMap<u64, DerivedFrom>,
    canonical: Vec<(CanonicalLayer, DerivedFrom)>,
}

const STREAMS: [&str; 4] = [
    "hevy.workouts",
    "peloton.rides",
    "withings.measurements",
    "garmin.activities",
];

impl Graph {
    fn with<T>(&self, change: impl FnOnce(&mut Layered) -> T) -> Result<T, StoreError> {
        let mut layered = self.0.lock().map_err(|_| StoreError::Unavailable {
            detail: "poisoned".to_owned(),
        })?;
        Ok(change(&mut layered))
    }

    /// Something new reaches a stream: a record landed, or a correction.
    fn reach(&self, stream: &str) {
        let _ = self.with(|layered| *layered.inputs.entry(stream.to_owned()).or_default() += 1);
    }

    fn normalised(&self) -> Vec<String> {
        self.with(|layered| layered.normalised.clone())
            .unwrap_or_default()
    }

    fn canonicalised(&self) -> Vec<CanonicalLayer> {
        self.with(|layered| layered.canonicalised.clone())
            .unwrap_or_default()
    }

    fn reached(&self) -> Vec<CanonicalLayer> {
        self.with(|layered| layered.reached.clone())
            .unwrap_or_default()
    }

    const fn reads(layer: CanonicalLayer) -> &'static [&'static str] {
        match layer {
            CanonicalLayer::Gym => &["hevy.workouts", "garmin.activities"],
            CanonicalLayer::Cycling => &["peloton.rides"],
            CanonicalLayer::WeighIns => &["withings.measurements"],
        }
    }
}

/// The normalised layers over the graph.
#[derive(Clone)]
struct Layers(Arc<Graph>);

impl NormalisedLayers for Layers {
    fn streams(&self) -> Vec<LandingStream> {
        STREAMS
            .iter()
            .filter_map(|name| LandingStream::try_from(*name).ok())
            .collect()
    }

    async fn inputs(&self, stream: &LandingStream) -> Result<DerivedFrom, StoreError> {
        self.0.with(|layered| {
            let reach = layered
                .inputs
                .get(&stream.to_string())
                .copied()
                .unwrap_or(0);
            DerivedFrom::from(format!("{stream} {reach}"))
        })
    }

    async fn normalise(
        &self,
        stream: &LandingStream,
    ) -> Result<NormalisationSummary, NormalisationError> {
        let run = self
            .0
            .with(|layered| {
                if layered.failing.contains(&stream.to_string()) {
                    return None;
                }
                layered.last_run += 1;
                layered.runs.insert(stream.to_string(), layered.last_run);
                layered.normalised.push(stream.to_string());
                Some(layered.last_run)
            })
            .map_err(NormalisationError::Store)?
            .ok_or(NormalisationError::MissingTimeZone)?;
        Ok(NormalisationSummary {
            run_id: NormalisationRunId::from(run),
            records_read: RecordCount::from(0),
            workouts_written: WorkoutCount::from(1),
            records_composed: RecordCount::from(0),
            records_superseded: RecordCount::from(0),
            workouts_retracted: WorkoutCount::from(0),
            retractions_read: RecordCount::from(0),
            records_refused: RecordCount::from(0),
            refusals_recorded: RefusalCount::from(0),
            records_covered: RecordCount::from(0),
        })
    }
}

/// The canonical layers over the graph, each derived from the runs of the
/// streams it reads.
struct Canonical(Arc<Graph>);

impl Canonical {
    fn derive(&self, layer: CanonicalLayer, rederivation: Rederivation) -> Result<(), StoreError> {
        self.0.with(|layered| {
            layered.canonicalised.push(layer);
            if rederivation == Rederivation::Incremental {
                layered.reached.push(layer);
            }
        })
    }
}

impl Canonicalising for Canonical {
    async fn gym(&self, rederivation: Rederivation) -> Result<Canonicalised, StoreError> {
        self.derive(CanonicalLayer::Gym, rederivation)?;
        Ok(Canonicalised {
            read: SessionCount::from(0),
            written: SessionCount::from(0),
        })
    }

    async fn cycling(&self, rederivation: Rederivation) -> Result<Canonicalised, StoreError> {
        self.derive(CanonicalLayer::Cycling, rederivation)?;
        Ok(Canonicalised {
            read: SessionCount::from(0),
            written: SessionCount::from(0),
        })
    }

    async fn weigh_ins(
        &self,
        rederivation: Rederivation,
    ) -> Result<WeighInsCanonicalised, StoreError> {
        self.derive(CanonicalLayer::WeighIns, rederivation)?;
        Ok(WeighInsCanonicalised {
            read: WeighInCount::from(0),
            written: WeighInCount::from(0),
            left_out: Vec::new(),
        })
    }

    async fn inputs(&self, layer: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
        self.0.with(|layered| {
            let runs: Vec<String> = Graph::reads(layer)
                .iter()
                .map(|stream| format!("{stream} {:?}", layered.runs.get(*stream)))
                .collect();
            DerivedFrom::from(runs.join(", "))
        })
    }
}

/// The ledger over the graph.
struct Ledger(Arc<Graph>);

impl DerivationLedger for Ledger {
    async fn normalised(&self, stream: &LandingStream) -> Result<Option<DerivedFrom>, StoreError> {
        self.0.with(|layered| {
            layered
                .runs
                .get(&stream.to_string())
                .and_then(|run| layered.derived_from.get(run))
                .cloned()
        })
    }

    async fn record_normalised(
        &self,
        run: NormalisationRunId,
        from: &DerivedFrom,
    ) -> Result<(), StoreError> {
        self.0.with(|layered| {
            layered.derived_from.insert(run.as_u64(), from.clone());
        })
    }

    async fn canonical(&self, layer: CanonicalLayer) -> Result<Option<DerivedFrom>, StoreError> {
        self.0.with(|layered| {
            layered
                .canonical
                .iter()
                .find(|(held, _)| *held == layer)
                .map(|(_, from)| from.clone())
        })
    }

    async fn record_canonical(
        &self,
        layer: CanonicalLayer,
        from: &DerivedFrom,
    ) -> Result<(), StoreError> {
        self.0.with(|layered| {
            layered.canonical.retain(|(held, _)| *held != layer);
            layered.canonical.push((layer, from.clone()));
        })
    }
}

/// A catalogue with nothing new to list or read, held as given.
struct Converged(ClassesHeld);

impl ClassSource for Converged {
    async fn list(&self) -> Result<ClassesListed, SourceError> {
        Ok(ClassesListed {
            listed: 3,
            added: 0,
            outstanding: 0,
            reading_takes: std::time::Duration::ZERO,
        })
    }

    async fn read(&self) -> Result<ClassesRead, SourceError> {
        Ok(ClassesRead {
            held: 3,
            read: 0,
            not_served: 0,
            outstanding: 0,
            stopped: None,
        })
    }
    async fn held(&self) -> Result<ClassesHeld, StoreError> {
        Ok(self.0)
    }
}

/// When each stream was last fetched, and a clock stopped at [`AT`].
#[derive(Default)]
struct Fetches(BTreeMap<LandingStream, Fetched>);

impl Fetches {
    /// Each stream last fetched so many minutes before [`AT`], and last
    /// landing something so many minutes before it, where it ever did.
    fn minutes(streams: &[(&str, i64, Option<i64>)]) -> Result<Self, Box<dyn std::error::Error>> {
        let ago = |minutes: i64| -> Result<FetchedAt, jiff::Error> {
            Ok(FetchedAt::from(
                AT.checked_sub(jiff::SignedDuration::from_mins(minutes))?,
            ))
        };
        let mut fetched = BTreeMap::new();
        for (name, succeeded, landed) in streams {
            fetched.insert(
                stream(name)?,
                Fetched {
                    succeeded: Some(ago(*succeeded)?),
                    landed: landed.map(ago).transpose()?,
                },
            );
        }
        Ok(Self(fetched))
    }
}

impl FetchLog for Fetches {
    async fn fetched(&self, stream: &LandingStream) -> Result<Fetched, StoreError> {
        Ok(self.0.get(stream).copied().unwrap_or_default())
    }
}

impl Clock for Fetches {
    fn now(&self) -> FetchedAt {
        FetchedAt::from(AT)
    }
}

/// The moment the runs here are, inside [`now`]'s afternoon in London.
const AT: jiff::Timestamp = jiff::Timestamp::constant(1_791_813_600, 0);

/// A school calendar that will not answer.
struct Unreachable;

impl HolidayCalendar for Unreachable {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        Err(SourceError::Unavailable {
            detail: "the feed did not answer".to_owned(),
        })
    }
}

/// A calendar that answers, with nothing in it.
struct Answering;

impl HolidayCalendar for Answering {
    async fn holidays(&self) -> Result<Holidays, SourceError> {
        Ok(Holidays::default())
    }
}

/// A diary with one week recorded, or none, and the skips written to it.
struct Recorded {
    diary: Diary,
    skips: Mutex<Vec<Skip>>,
}

impl Recorded {
    fn week() -> Result<Self, Box<dyn std::error::Error>> {
        let zone = OperatorZone::try_from("Europe/London".to_owned())?;
        let slots = BTreeMap::from([
            (
                TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
                Allocation::new(Discipline::Gym, HARDER),
            ),
            (
                TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
                Allocation::new(Discipline::Cycling, HARDER),
            ),
        ]);
        Ok(Self {
            diary: Diary::new(
                vec![TrainingPattern::new(
                    Date::constant(2026, 9, 7),
                    zone,
                    slots,
                )],
                Vec::new(),
            ),
            skips: Mutex::new(Vec::new()),
        })
    }

    fn nothing() -> Self {
        Self {
            diary: Diary::new(Vec::new(), Vec::new()),
            skips: Mutex::new(Vec::new()),
        }
    }
}

impl DiaryStore for Recorded {
    async fn diary(&self) -> Result<Diary, StoreError> {
        Ok(self.diary.clone())
    }
}

impl DiaryAuthor for Recorded {
    async fn record_pattern(&self, _schedule: &TrainingPattern) -> Result<(), StoreError> {
        Ok(())
    }

    async fn record_alteration(
        &self,
        _alteration: &domain::schedule::Alteration,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    async fn record_skip(&self, skip: &Skip) -> Result<(), StoreError> {
        self.skips
            .lock()
            .map_err(|_| StoreError::Unavailable {
                detail: "poisoned".to_owned(),
            })?
            .push(skip.clone());
        Ok(())
    }
}

/// No plan at all.
#[derive(Clone)]
struct NoPlans;

impl PlanStore for NoPlans {
    async fn windows(&self) -> Result<Vec<PlanWindow>, StoreError> {
        Ok(Vec::new())
    }

    async fn named(&self, _name: &PlanName) -> Result<Option<Plan>, StoreError> {
        Ok(None)
    }

    async fn author(&self, _plan: &Plan) -> Result<PlanId, StoreError> {
        Err(StoreError::Unavailable {
            detail: "nothing is authored here".to_owned(),
        })
    }

    async fn commit(
        &self,
        _plan: &PlanName,
        _gym: &[GymMesocycle],
        _cycling: &[CyclingMesocycle],
    ) -> Result<(), StoreError> {
        Ok(())
    }
}

/// The week of Monday 12 October 2026: the gym on the Monday evening and a ride
/// on the Wednesday evening, each in the state given.
struct Week(Vec<SessionState>);

fn slot(date: Date, discipline: Discipline) -> ScheduledSlot {
    ScheduledSlot {
        date,
        slot: TrainingSlot::new(date.weekday(), PartOfDay::Evening),
        discipline,
        role: HARDER,
    }
}

impl StandingReader for Week {
    async fn standing(&self, _now: DayPart) -> Result<Standing, StoreError> {
        let monday = Date::constant(2026, 10, 12);
        let wednesday = Date::constant(2026, 10, 14);
        let slots = [
            (
                slot(monday, Discipline::Gym),
                Some(DayPart::new(wednesday, PartOfDay::Evening)),
            ),
            (slot(wednesday, Discipline::Cycling), None),
        ];
        Ok(Standing {
            commencing: Some(monday),
            sessions: slots
                .into_iter()
                .zip(self.0.iter())
                .map(|((slot, closes), state)| Session {
                    slot,
                    number: 1,
                    programmed: true,
                    test: false,
                    state: state.clone(),
                    closes,
                })
                .collect(),
            ..Standing::default()
        })
    }
}

/// Nothing is due, or committing what is due fails.
struct Committer(Option<CommitError>);

impl MesocycleCommitter for Committer {
    async fn due(
        &self,
        _now: DayPart,
        _holidays: &Holidays,
        _standing: &Standing,
        _diary: &Diary,
    ) -> Result<Option<Due>, CommitError> {
        self.0.clone().map_or(Ok(None), Err)
    }

    async fn commit(&self, _due: &Due) -> Result<(), CommitError> {
        Ok(())
    }
}

/// A gym with nothing planned at or after the slot.
struct Unplanned;

impl GymSessions for Unplanned {
    async fn prescribed(
        &self,
        from: Date,
        _reschedule: &Reschedule,
    ) -> Result<GymPrescribed, PrescriptionError> {
        Ok(GymPrescribed::NothingPlanned { from, last: None })
    }
}

/// Cycling with no day to ride on.
struct Dayless;

impl CyclingSessions for Dayless {
    async fn prescribed(
        &self,
        from: Date,
        _reschedule: &Reschedule,
        _diary: &Diary,
    ) -> Result<CyclingPrescribed, RidingError> {
        Err(RidingError::NoDay { from })
    }
}

/// Every event, in the order it arrived.
#[derive(Default)]
struct Heard(Mutex<Vec<String>>);

impl Heard {
    fn push(&self, event: String) {
        if let Ok(mut heard) = self.0.lock() {
            heard.push(event);
        }
    }

    fn events(&self) -> Vec<String> {
        self.0.lock().map(|heard| heard.clone()).unwrap_or_default()
    }
}

impl Progress for Heard {
    fn started(&self, step: &Step) {
        self.push(format!("started {step:?}"));
    }

    fn estimated(&self, step: &Step, outstanding: u64, _takes: std::time::Duration) {
        self.push(format!("estimated {step:?} {outstanding}"));
    }

    fn finished(&self, done: &Done) {
        self.push(format!("finished {:?}", done.step));
    }
}

type Ports<H = Unreachable> = NextPorts<
    Walking,
    Walking,
    Daily<Walking, Newest>,
    Walking,
    Daily<Walking, Newest>,
    Layers,
    Canonical,
    Ledger,
    Converged,
    H,
    Recorded,
    NoPlans,
    Week,
    Committer,
    Unplanned,
    Dayless,
    Lock,
    Fetches,
>;

/// The parts other runs of `next` hold, each let go once waited on.
#[derive(Default)]
pub struct Lock {
    held: Mutex<BTreeSet<Part>>,
}

impl Lock {
    fn holding(parts: impl IntoIterator<Item = Part>) -> Self {
        Self {
            held: Mutex::new(parts.into_iter().collect()),
        }
    }
}

impl NextLock for Lock {
    type Guard = ();

    fn try_acquire(&self, parts: &BTreeSet<Part>) -> Result<Taken<()>, RunLockError> {
        let contended: BTreeSet<Part> = self
            .held
            .lock()
            .map_err(|_| RunLockError::Unavailable {
                detail: "poisoned".to_owned(),
            })?
            .intersection(parts)
            .cloned()
            .collect();
        Ok(if contended.is_empty() {
            Taken::All(())
        } else {
            Taken::Held(contended)
        })
    }

    async fn released(&self, parts: &BTreeSet<Part>) -> Result<(), RunLockError> {
        if let Ok(mut held) = self.held.lock() {
            held.retain(|part| !parts.contains(part));
        }
        Ok(())
    }
}

/// The gym walked and landed two; Peloton has no credential; the watch's
/// activities landed three and its days four; the school calendar does not
/// answer.
fn ports(
    diary: Recorded,
    week: Week,
    committer: Committer,
) -> Result<Ports, Box<dyn std::error::Error>> {
    calendar(diary, week, committer, Unreachable)
}

/// As [`ports`], over a calendar of the test's choosing.
fn calendar<H>(
    diary: Recorded,
    week: Week,
    committer: Committer,
    holidays: H,
) -> Result<Ports<H>, Box<dyn std::error::Error>> {
    over(Arc::default(), diary, week, committer, holidays)
}

/// As [`calendar`], over a graph the test keeps hold of.
fn over<H>(
    graph: Arc<Graph>,
    diary: Recorded,
    week: Week,
    committer: Committer,
    holidays: H,
) -> Result<Ports<H>, Box<dyn std::error::Error>> {
    Ok(NextPorts {
        gym_records: Records {
            entry: stream("hevy.workouts")?,
            walks: Ok(Walking::landing(stream("hevy.workouts")?, 2)),
        },
        cycling_records: Records {
            entry: stream("peloton.rides")?,
            walks: Err(NoAccess {
                detail: "no peloton credential".to_owned(),
            }),
        },
        weigh_in_records: Records {
            entry: stream("withings.measurements")?,
            walks: Ok(Daily {
                walks: Walking::landing(stream("withings.measurements")?, 1),
                newest: Newest(None),
                awaits: Awaits::Today,
            }),
        },
        watch_records: Records {
            entry: stream("garmin.activities")?,
            walks: Ok(Walking::landing(stream("garmin.activities")?, 3)),
        },
        watch_days: Records {
            entry: stream("garmin.daily_heart_rate")?,
            walks: Ok(Daily {
                walks: Walking::landing(stream("garmin.daily_heart_rate")?, 4),
                newest: Newest(None),
                awaits: Awaits::Yesterday,
            }),
        },
        normalised: Layers(Arc::clone(&graph)),
        canonical: Canonical(Arc::clone(&graph)),
        ledger: Ledger(graph),
        classes: Ok(Converged(ClassesHeld::default())),
        holidays,
        diary,
        plans: NoPlans,
        microcycle: week,
        committing: committer,
        gym: Unplanned,
        cycling: Dayless,
        lock: Lock::default(),
        fetches: Fetches::default(),
        zone: OperatorZone::try_from("Europe/London")?,
        refetch: Refetch::WhenDue,
    })
}

const fn now() -> DayPart {
    DayPart::new(Date::constant(2026, 10, 12), PartOfDay::Afternoon)
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().build()
}

/// **A source that cannot be reached is a failed step, and the run carries
/// on** — through the derivations, the standing and the next session (§ 36).
#[test]
fn a_source_that_cannot_be_fetched_is_a_failed_step_and_the_run_carries_on() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    let steps: Vec<(&Step, bool)> = report
        .run
        .iter()
        .map(|done| (&done.step, done.outcome.is_ok()))
        .collect();
    let hevy = stream("hevy.workouts").expect("a stream");
    let peloton = stream("peloton.rides").expect("a stream");
    let withings = stream("withings.measurements").expect("a stream");
    let garmin = stream("garmin.activities").expect("a stream");
    let garmin_days = stream("garmin.daily_heart_rate").expect("a stream");
    assert_eq!(
        steps,
        vec![
            (&Step::Fetching(hevy.clone()), true),
            (&Step::Fetching(peloton.clone()), false),
            (&Step::RefreshingClasses, true),
            (&Step::Fetching(withings.clone()), true),
            (&Step::Fetching(garmin.clone()), true),
            (&Step::Fetching(garmin_days), true),
            (&Step::Normalising(hevy), true),
            (&Step::Normalising(peloton), true),
            (&Step::Normalising(withings), true),
            (&Step::Normalising(garmin), true),
            (&Step::Canonicalising(Discipline::Gym), true),
            (&Step::Canonicalising(Discipline::Cycling), true),
            (&Step::CanonicalisingWeighIns, true),
        ]
    );
    assert!(matches!(
        report.run.get(1).map(|done| &done.outcome),
        Some(Err(StepFailure::NoAccess(_)))
    ));
    assert!(matches!(
        report.next,
        WhatIsNext::Session {
            prescribed: Prescribed::Gym(Ok(GymPrescribed::NothingPlanned { .. })),
            ..
        }
    ));
}

/// **Each walk behind one entry reports its own outcome** (#367): the ride
/// list landing and the graphs being refused are two lines, not one failure.
#[test]
fn each_walk_behind_an_entry_reports_its_own_outcome() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    ports.cycling_records.walks = Ok(Walking(
        vec![
            (stream("peloton.rides").expect("a stream"), Ok(landed(50))),
            (
                stream("peloton.ride_samples").expect("a stream"),
                Err(ExtractionError::Source(SourceError::Unauthorised)),
            ),
        ],
        None,
        Vec::new(),
    ));
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    let fetched: Vec<(String, Option<Found>)> = report
        .run
        .iter()
        .filter_map(|done| match &done.step {
            Step::Fetching(stream) => Some((stream.to_string(), done.outcome.clone().ok())),
            _ => None,
        })
        .collect();
    assert_eq!(
        fetched,
        vec![
            (
                "hevy.workouts".to_owned(),
                Some(Found::Landed(RecordCount::from(2)))
            ),
            (
                "peloton.rides".to_owned(),
                Some(Found::Landed(RecordCount::from(50)))
            ),
            ("peloton.ride_samples".to_owned(), None),
            (
                "withings.measurements".to_owned(),
                Some(Found::Landed(RecordCount::from(1)))
            ),
            (
                "garmin.activities".to_owned(),
                Some(Found::Landed(RecordCount::from(3)))
            ),
            (
                "garmin.daily_heart_rate".to_owned(),
                Some(Found::Landed(RecordCount::from(4)))
            ),
        ]
    );
}

/// **With no Garmin credential, the run says the watch was not collected and
/// carries on** (#432): every other stream is fetched, every layer derived,
/// and the next session prescribed (§ 36).
#[test]
fn with_no_garmin_credential_the_watch_is_not_collected_and_the_run_carries_on() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    let no_garmin = NoAccess {
        detail: "no garmin credential".to_owned(),
    };
    ports.watch_records.walks = Err(no_garmin.clone());
    ports.watch_days.walks = Err(no_garmin.clone());
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    let garmin = stream("garmin.activities").expect("a stream");
    for entry in [
        garmin.clone(),
        stream("garmin.daily_heart_rate").expect("a stream"),
    ] {
        let watch = report
            .run
            .iter()
            .find(|done| done.step == Step::Fetching(entry.clone()))
            .expect("the watch is reported");
        assert_eq!(watch.outcome, Err(StepFailure::NoAccess(no_garmin.clone())));
    }
    let landed = |name: &str| {
        report.run.iter().any(|done| {
            done.step == Step::Fetching(stream(name).expect("a stream"))
                && matches!(done.outcome, Ok(Found::Landed(_)))
        })
    };
    assert!(landed("hevy.workouts"));
    assert!(landed("withings.measurements"));
    assert!(
        report
            .run
            .iter()
            .any(|done| done.step == Step::Normalising(garmin.clone()) && done.outcome.is_ok()),
        "what is already landed is still derived"
    );
    assert!(matches!(report.next, WhatIsNext::Session { .. }));
}

/// **Every step is reported as it runs**, started then finished, in the order
/// the report keeps them — so a frontend following progress and one reading
/// the report cannot disagree.
#[test]
fn every_step_is_reported_as_it_ends_in_the_order_the_report_keeps() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    let heard = Heard::default();
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &heard))
        .expect("the run answers");

    let expected: Vec<String> = report
        .run
        .iter()
        .flat_map(|done| {
            [
                format!("started {:?}", done.step),
                format!("finished {:?}", done.step),
            ]
        })
        .collect();
    assert_eq!(heard.events(), expected);
}

/// **A mesocycle that will not commit is a failed step**, and the run still
/// says where the plan stands and what is next.
#[test]
fn a_mesocycle_that_cannot_be_committed_is_a_failed_step() {
    // Committing reads the calendar, so this one answers.
    let ports = calendar(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(Some(CommitError::NotDesigned {
            planned_at: jiff::Timestamp::UNIX_EPOCH,
        })),
        Answering,
    )
    .expect("the ports build");
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    assert!(report.run.iter().any(|done| done.step == Step::Committing
        && done.outcome
            == Err(StepFailure::Commit(CommitError::NotDesigned {
                planned_at: jiff::Timestamp::UNIX_EPOCH,
            }))));
    assert!(matches!(report.next, WhatIsNext::Session { .. }));
}

/// **The session after a skip is the one prescribed**: the skip is recorded
/// from now to the current session's window closing, and the standing read
/// again.
#[test]
fn a_skip_takes_the_rest_of_the_current_window() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    let next = Next::new(ports);
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(next.run(now(), Some("ill"), IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    let skipped = report.run.iter().find_map(|done| match &done.outcome {
        Ok(Found::Skipped { skip, .. }) => Some(skip.clone()),
        _ => None,
    });
    let skip = skipped.expect("a skip is reported");
    assert_eq!(skip.from(), now());
    assert_eq!(
        skip.until(),
        DayPart::new(Date::constant(2026, 10, 14), PartOfDay::Evening)
    );
}

/// **A skip with nothing left to skip is refused**, rather than delivering the
/// session the operator asked to skip.
#[test]
fn a_skip_with_nothing_to_skip_is_refused() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::Performed, SessionState::Performed]),
        Committer(None),
    )
    .expect("the ports build");
    let refused = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), Some("ill"), IfGoing::Refuse, &Heard::default()));

    assert_eq!(
        refused.err(),
        Some(NextError::Skip(SkipRefused::NothingToSkip))
    );
}

/// **Nothing is collected without a recorded week**, because nothing could say
/// what is next.
#[test]
fn no_recorded_week_is_refused_before_anything_runs() {
    let ports =
        ports(Recorded::nothing(), Week(Vec::new()), Committer(None)).expect("the ports build");
    let heard = Heard::default();
    let refused = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &heard));

    assert_eq!(refused.err(), Some(NextError::NoWeek));
    assert!(heard.events().is_empty());
}

/// **A microcycle with nothing left to perform says so**, and names the slot
/// the next one opens with rather than prescribing it.
#[test]
fn a_settled_microcycle_prescribes_nothing() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::Performed, SessionState::Performed]),
        Committer(None),
    )
    .expect("the ports build");
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    assert!(matches!(report.next, WhatIsNext::MicrocycleComplete { .. }));
}

/// **A second run is refused before it reads anything**, whichever frontend
/// started the first (#398).
#[test]
fn a_run_while_another_is_going_is_refused_before_anything_runs() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::ToBePrescribed]),
        Committer(None),
    )
    .expect("the ports build");
    ports.lock = Lock::holding([Part::Delivering]);
    let heard = Heard::default();
    let refused = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &heard));

    assert_eq!(refused.err(), Some(NextError::AlreadyRunning));
    assert!(heard.events().is_empty());
}

/// **A page's run of the whole of `next` joins one going** (#559): it waits
/// for it, uses what it collected, and delivers nothing itself.
#[test]
fn a_whole_run_asked_to_join_waits_and_delivers_nothing() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::ToBePrescribed]),
        Committer(None),
    )
    .expect("the ports build");
    ports.lock = Lock::holding([Part::Delivering]);
    let heard = Heard::default();
    let joined = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Join, &heard));

    assert_eq!(joined.err(), Some(NextError::Joined));
    assert_eq!(
        heard.events(),
        vec![
            "started Joining({Delivering})".to_owned(),
            "finished Joining({Delivering})".to_owned(),
        ]
    );
}

/// **A chart's run collects only what it reads** (#559): its walks and its
/// layers, and no class refresh, commit or delivery.
#[test]
fn a_collection_fetches_and_derives_only_its_scope() {
    let ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::ToBePrescribed]),
        Committer(None),
    )
    .expect("the ports build");
    let hevy = stream("hevy.workouts").expect("a stream");
    let scope = Scope::new([
        Part::Fetching(hevy.clone()),
        Part::Normalising(hevy),
        Part::Canonicalising(CanonicalLayer::Gym),
    ]);
    let run = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).collect(&scope, now(), &Heard::default()))
        .expect("the run collects");

    let steps: Vec<String> = run.iter().map(|done| format!("{:?}", done.step)).collect();
    assert_eq!(
        steps,
        vec![
            "Fetching(LandingStream { source: SourceName(\"hevy\"), entity: EntityKind(\"workouts\") })",
            "Normalising(LandingStream { source: SourceName(\"hevy\"), entity: EntityKind(\"workouts\") })",
            "Canonicalising(Gym)",
        ]
    );
}

/// **A part another run holds is joined, not collected again**, and the rest
/// is collected: the watch's page joins the gym's, and fetches its own
/// stream once the gym's run has ended.
#[test]
fn a_part_another_run_holds_is_joined_and_the_rest_collected() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![SessionState::ToBePrescribed]),
        Committer(None),
    )
    .expect("the ports build");
    let hevy = stream("hevy.workouts").expect("a stream");
    let watch = stream("garmin.activities").expect("a stream");
    ports.lock = Lock::holding([Part::Normalising(hevy.clone())]);
    let scope = Scope::new([
        Part::Fetching(watch.clone()),
        Part::Normalising(watch.clone()),
        Part::Normalising(hevy.clone()),
    ]);
    let run = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).collect(&scope, now(), &Heard::default()))
        .expect("the run collects");

    let steps: Vec<&Step> = run.iter().map(|done| &done.step).collect();
    assert_eq!(
        steps,
        vec![
            &Step::Joining(BTreeSet::from([Part::Normalising(hevy)])),
            &Step::Fetching(watch.clone()),
            &Step::Normalising(watch),
        ]
    );
}

// --- Deriving only what something new reached (#468) ------------------------

/// A run over the graph.
fn run_over(graph: &Arc<Graph>) -> Result<Report, Box<dyn std::error::Error>> {
    let ports = over(
        Arc::clone(graph),
        Recorded::week()?,
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
        Unreachable,
    )?;
    Ok(runtime()?.block_on(Next::new(ports).run(
        now(),
        None,
        IfGoing::Refuse,
        &Heard::default(),
    ))?)
}

/// Each derivation step, named, and whether it said its layer was unchanged.
fn derived(report: &Report) -> Vec<(String, bool)> {
    report
        .run
        .iter()
        .filter_map(|done| {
            let name = match &done.step {
                Step::Normalising(stream) => stream.to_string(),
                Step::Canonicalising(discipline) => discipline.as_str().to_owned(),
                Step::CanonicalisingWeighIns => "weigh-ins".to_owned(),
                _ => return None,
            };
            Some((name, done.outcome == Ok(Found::Unchanged)))
        })
        .collect()
}

fn unchanged(names: &[&str], changed: &[&str]) -> Vec<(String, bool)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), !changed.contains(name)))
        .collect()
}

const LAYERS: [&str; 7] = [
    "hevy.workouts",
    "peloton.rides",
    "withings.measurements",
    "garmin.activities",
    "gym",
    "cycling",
    "weigh-ins",
];

/// **A second run with nothing new derives nothing**, and says of each layer
/// that it was unchanged.
#[test]
fn a_second_run_with_nothing_new_derives_nothing() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");
    let derived_before = (graph.normalised(), graph.canonicalised());

    let second = run_over(&graph).expect("the run answers");

    assert_eq!(derived(&second), unchanged(&LAYERS, &[]));
    assert_eq!((graph.normalised(), graph.canonicalised()), derived_before);
}

/// **Something new reaching Hevy derives the gym again, and nothing that does
/// not read from it**: its normalised layer, then the canonical gym layer that
/// reads it, while cycling and the weigh-ins stand.
#[test]
fn something_new_reaching_a_stream_derives_what_reads_from_it_and_nothing_else() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");

    graph.reach("hevy.workouts");
    let report = run_over(&graph).expect("the run answers");

    assert_eq!(
        derived(&report),
        unchanged(&LAYERS, &["hevy.workouts", "gym"])
    );
}

/// **A canonical layer is built whole once and rebuilt only where something
/// reached it after** (#610): the first run has no derivation to rebuild part
/// of, and the second has one, for the layers that read what changed.
#[test]
fn a_canonical_layer_is_built_whole_once_and_then_only_where_reached() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");
    assert_eq!(graph.reached(), Vec::new(), "built whole");

    graph.reach("hevy.workouts");
    graph.reach("peloton.rides");
    graph.reach("withings.measurements");
    run_over(&graph).expect("the run answers");
    assert_eq!(
        graph.reached(),
        vec![
            CanonicalLayer::Gym,
            CanonicalLayer::Cycling,
            CanonicalLayer::WeighIns
        ]
    );
}

/// **A correction derives again what reads from it**: the overlay is one of a
/// normalised layer's inputs, so a correction to a Garmin set derives Garmin's
/// layer and the gym's, which reads it.
#[test]
fn a_correction_derives_again_what_reads_from_it() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");

    graph.reach("garmin.activities");
    let report = run_over(&graph).expect("the run answers");

    assert_eq!(
        derived(&report),
        unchanged(&LAYERS, &["garmin.activities", "gym"])
    );
}

/// **A layer that will not derive is a failed step**, the run carries on, and
/// it is derived again next run rather than passed over.
#[test]
fn a_layer_that_will_not_derive_is_a_failed_step_and_tried_again() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");
    graph
        .with(|layered| layered.failing.push("garmin.activities".to_owned()))
        .expect("the graph");
    graph.reach("garmin.activities");

    let failed = run_over(&graph).expect("the run answers");
    let garmin = Step::Normalising(stream("garmin.activities").expect("a stream"));
    assert!(
        failed.run.iter().any(|done| done.step == garmin
            && matches!(done.outcome, Err(StepFailure::Normalisation(_))))
    );
    assert!(matches!(failed.next, WhatIsNext::Session { .. }));

    graph
        .with(|layered| layered.failing.clear())
        .expect("the graph");
    let retried = run_over(&graph).expect("the run answers");
    assert_eq!(
        derived(&retried),
        unchanged(&LAYERS, &["garmin.activities", "gym"])
    );
}

/// The Withings step of a run whose newest weigh-in is on `newest`.
type Outcomes = Vec<(Step, Result<Found, StepFailure>)>;

fn withings_on(newest: Option<Date>) -> Result<Outcomes, Box<dyn std::error::Error>> {
    let mut ports = ports(
        Recorded::week()?,
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )?;
    if let Ok(daily) = &mut ports.weigh_in_records.walks {
        daily.newest = Newest(newest);
    }
    let report = runtime()?.block_on(Next::new(ports).run(
        now(),
        None,
        IfGoing::Refuse,
        &Heard::default(),
    ))?;
    let withings = stream("withings.measurements")?;
    Ok(report
        .run
        .into_iter()
        .filter(|done| done.step == Step::Fetching(withings.clone()))
        .map(|done| (done.step, done.outcome))
        .collect())
}

/// **Once today's weigh-in has been collected, Withings is not fetched again
/// until tomorrow**; a weigh-in from yesterday leaves it due.
#[test]
fn a_daily_stream_holding_today_is_not_fetched_until_tomorrow() {
    let withings = Step::Fetching(stream("withings.measurements").expect("a stream"));

    assert_eq!(
        withings_on(Some(now().date)).expect("the run answers"),
        vec![(withings.clone(), Ok(Found::NotDue(NotDue::UntilTomorrow)))]
    );
    assert_eq!(
        withings_on(now().date.yesterday().ok()).expect("the run answers"),
        vec![(withings, Ok(Found::Landed(RecordCount::from(1))))]
    );
}

/// **The streams are fetched at once**: each walk here waits for the other
/// three to start, which they never would one after another.
#[test]
fn the_streams_are_fetched_at_once() {
    let mut ports = ports(
        Recorded::week().expect("a week"),
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )
    .expect("the ports build");
    let met = Some(Arc::new(AtomicUsize::new(0)));
    let walking = |name: &str| {
        Walking(
            vec![(stream(name).expect("a stream"), Ok(landed(1)))],
            met.clone(),
            Vec::new(),
        )
    };
    ports.gym_records.walks = Ok(walking("hevy.workouts"));
    ports.cycling_records.walks = Ok(walking("peloton.rides"));
    if let Ok(daily) = &mut ports.weigh_in_records.walks {
        daily.walks = walking("withings.measurements");
    }
    ports.watch_records.walks = Ok(walking("garmin.activities"));
    // After the watch's activities, on the same sign-in, so it meets nobody.
    if let Ok(daily) = &mut ports.watch_days.walks {
        daily.walks = Walking::landing(stream("garmin.daily_heart_rate").expect("a stream"), 1);
    }
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
        .expect("the run answers");

    let fetched: Vec<bool> = report
        .run
        .iter()
        .filter(|done| matches!(done.step, Step::Fetching(_)))
        .map(|done| done.outcome == Ok(Found::Landed(RecordCount::from(1))))
        .collect();
    assert_eq!(fetched, vec![true, true, true, true, true]);
}

/// The watch's steps of a run whose day walks are whole through `whole`.
fn watch_on(whole: Option<Date>) -> Result<Outcomes, Box<dyn std::error::Error>> {
    let mut ports = ports(
        Recorded::week()?,
        Week(vec![
            SessionState::ToBePrescribed,
            SessionState::ToBePrescribed,
        ]),
        Committer(None),
    )?;
    if let Ok(daily) = &mut ports.watch_days.walks {
        daily.newest = Newest(whole);
    }
    let report = runtime()?.block_on(Next::new(ports).run(
        now(),
        None,
        IfGoing::Refuse,
        &Heard::default(),
    ))?;
    let watch = [
        stream("garmin.activities")?,
        stream("garmin.daily_heart_rate")?,
    ];
    Ok(report
        .run
        .into_iter()
        .filter(|done| {
            watch
                .iter()
                .any(|entry| done.step == Step::Fetching(entry.clone()))
        })
        .map(|done| (done.step, done.outcome))
        .collect())
}

/// **Once yesterday is whole, the watch's days are not fetched again until
/// tomorrow; its activities are fetched every run** (#432). The operator,
/// 2026-10-22: *"we don't need to fetch daily data if we've already got data
/// for today, in the case of Withings, or yesterday, in the case of daily
/// Garmin metrics."*
#[test]
fn the_watch_s_days_wait_for_yesterday_and_its_activities_do_not() {
    let activities = Step::Fetching(stream("garmin.activities").expect("a stream"));
    let days = Step::Fetching(stream("garmin.daily_heart_rate").expect("a stream"));
    let yesterday = now().date.yesterday().ok();
    let day_before = yesterday.and_then(|day| day.yesterday().ok());

    assert_eq!(
        watch_on(yesterday).expect("the run answers"),
        vec![
            (activities.clone(), Ok(Found::Landed(RecordCount::from(3)))),
            (days.clone(), Ok(Found::NotDue(NotDue::UntilTomorrow))),
        ]
    );
    assert_eq!(
        watch_on(day_before).expect("the run answers"),
        vec![
            (activities, Ok(Found::Landed(RecordCount::from(3)))),
            (days, Ok(Found::Landed(RecordCount::from(4)))),
        ]
    );
}

// --- Fetching only what could hold something new (#612) ---------------------

/// Each fetching step of `run`, and how it ended.
fn fetching(run: Vec<Done>) -> Outcomes {
    run.into_iter()
        .filter(|done| matches!(done.step, Step::Fetching(_)))
        .map(|done| (done.step, done.outcome))
        .collect()
}

/// What a page's run over Hevy fetches, with the gym's Monday session in
/// `state` and Hevy last fetched `ago` minutes before [`AT`].
fn hevy_fetched(
    state: SessionState,
    ago: i64,
    refetch: Refetch,
) -> Result<Outcomes, Box<dyn std::error::Error>> {
    let mut ports = ports(
        Recorded::week()?,
        Week(vec![state, SessionState::ToBePrescribed]),
        Committer(None),
    )?;
    let hevy = stream("hevy.workouts")?;
    ports.gym_records.walks = Ok(Walking::expecting(vec![(
        hevy.clone(),
        Expects::Sessions(Discipline::Gym),
    )]));
    ports.fetches = Fetches::minutes(&[("hevy.workouts", ago, None)])?;
    ports.refetch = refetch;
    let scope = Scope::new([Part::Fetching(hevy)]);
    Ok(fetching(runtime()?.block_on(Next::new(ports).collect(
        &scope,
        now(),
        &Heard::default(),
    ))?))
}

/// **Hevy is fetched only while a gym session could have been performed**
/// (#612): prescribed and not yet performed or skipped, or its window closed
/// since Hevy was last fetched, or before this microcycle began.
/// The operator, 2026-10-30, on the issue.
#[test]
fn a_session_stream_is_fetched_only_while_a_session_could_have_been_performed() {
    let hevy = Step::Fetching(stream("hevy.workouts").expect("a stream"));
    let landed = Ok(Found::Landed(RecordCount::from(1)));
    let no_session = Ok(Found::NotDue(NotDue::NoSession(Discipline::Gym)));
    let two_hours = 120;
    let last_week = 8 * 24 * 60;
    let fetched =
        |state, ago| hevy_fetched(state, ago, Refetch::WhenDue).expect("the run collects");

    assert_eq!(
        fetched(SessionState::Prescribed, two_hours),
        vec![(hevy.clone(), landed.clone())],
        "prescribed and not yet performed",
    );
    assert_eq!(
        fetched(SessionState::NotPerformed { absence: None }, two_hours),
        vec![(hevy.clone(), landed.clone())],
        "its window closes after the last fetch",
    );
    assert_eq!(
        fetched(SessionState::NotPrescribed, two_hours),
        vec![(hevy.clone(), landed.clone())],
        "its window closes after the last fetch with nothing prescribed",
    );
    assert_eq!(
        fetched(SessionState::ToBePrescribed, last_week),
        vec![(hevy.clone(), landed)],
        "last fetched before the microcycle began",
    );
    assert_eq!(
        fetched(SessionState::ToBePrescribed, two_hours),
        vec![(hevy.clone(), no_session.clone())],
        "nothing prescribed",
    );
    assert_eq!(
        fetched(SessionState::Performed, two_hours),
        vec![(hevy.clone(), no_session.clone())],
        "already performed",
    );
    assert_eq!(
        fetched(
            SessionState::Skipped {
                absence: domain::schedule::AbsenceKind::Illness,
            },
            two_hours,
        ),
        vec![(hevy, no_session)],
        "skipped",
    );
}

/// **A page does not fetch again what was fetched under 45 minutes ago**, the
/// shortest visit there can be; the terminal does whenever it is run.
#[test]
fn a_page_fetches_nothing_fetched_within_the_shortest_visit() {
    let hevy = Step::Fetching(stream("hevy.workouts").expect("a stream"));
    let landed = Ok(Found::Landed(RecordCount::from(1)));
    let fetched = |ago, refetch| {
        hevy_fetched(SessionState::Prescribed, ago, refetch).expect("the run collects")
    };

    assert_eq!(
        fetched(10, Refetch::SHORTEST_VISIT),
        vec![(
            hevy.clone(),
            Ok(Found::NotDue(NotDue::Fetched {
                within: std::time::Duration::from_mins(45),
            })),
        )]
    );
    assert_eq!(
        fetched(50, Refetch::SHORTEST_VISIT),
        vec![(hevy.clone(), landed.clone())]
    );
    assert_eq!(fetched(10, Refetch::WhenDue), vec![(hevy, landed)]);
}

/// **A ride's samples are fetched only once the ride list has landed
/// something since they last were**, and Peloton's account once a day.
#[test]
fn a_stream_that_follows_another_waits_for_it_and_an_account_for_tomorrow() {
    let fetched = |samples: i64, account: i64| -> Outcomes {
        let mut ports = ports(
            Recorded::week().expect("a week"),
            Week(vec![SessionState::Prescribed, SessionState::Prescribed]),
            Committer(None),
        )
        .expect("the ports build");
        let rides = stream("peloton.rides").expect("a stream");
        ports.cycling_records.walks = Ok(Walking::expecting(vec![
            (rides.clone(), Expects::Sessions(Discipline::Cycling)),
            (
                stream("peloton.ride_samples").expect("a stream"),
                Expects::After(rides),
            ),
            (
                stream("peloton.account").expect("a stream"),
                Expects::OnceADay,
            ),
        ]));
        ports.fetches = Fetches::minutes(&[
            ("peloton.rides", 120, Some(120)),
            ("peloton.ride_samples", samples, None),
            ("peloton.account", account, None),
        ])
        .expect("the fetches");
        let scope = Scope::new(
            ["peloton.rides", "peloton.ride_samples", "peloton.account"]
                .map(|name| Part::Fetching(stream(name).expect("a stream"))),
        );
        fetching(
            runtime()
                .expect("a current-thread runtime builds")
                .block_on(Next::new(ports).collect(&scope, now(), &Heard::default()))
                .expect("the run collects"),
        )
    };
    let outcomes = |run: Outcomes| {
        run.into_iter()
            .map(|(_, outcome)| outcome)
            .collect::<Vec<_>>()
    };
    let landed = Ok(Found::Landed(RecordCount::from(1)));
    let rides = stream("peloton.rides").expect("a stream");

    // Samples fetched before the rides landed, the account yesterday.
    assert_eq!(
        outcomes(fetched(180, 20 * 60)),
        vec![landed.clone(), landed.clone(), landed.clone()]
    );
    // Samples fetched since, the account this morning.
    assert_eq!(
        outcomes(fetched(60, 3 * 60)),
        vec![
            landed,
            Ok(Found::NotDue(NotDue::Follows(rides))),
            Ok(Found::NotDue(NotDue::UntilTomorrow)),
        ]
    );
}

/// **Peloton's classes are refreshed once a day**, unless details are still
/// to be read.
#[test]
fn the_classes_are_refreshed_once_a_day_unless_some_are_unread() {
    let refreshed = |listed_ago: i64, outstanding: u64| -> Result<Found, StepFailure> {
        let mut ports = ports(
            Recorded::week().expect("a week"),
            Week(vec![
                SessionState::ToBePrescribed,
                SessionState::ToBePrescribed,
            ]),
            Committer(None),
        )
        .expect("the ports build");
        let listed = AT
            .checked_sub(jiff::SignedDuration::from_mins(listed_ago))
            .expect("a moment");
        ports.classes = Ok(Converged(ClassesHeld {
            listed: Some(FetchedAt::from(listed)),
            outstanding,
        }));
        let report = runtime()
            .expect("a current-thread runtime builds")
            .block_on(Next::new(ports).run(now(), None, IfGoing::Refuse, &Heard::default()))
            .expect("the run answers");
        report
            .run
            .into_iter()
            .find(|done| done.step == Step::RefreshingClasses)
            .map(|done| done.outcome)
            .expect("a refresh step")
    };

    assert_eq!(refreshed(60, 0), Ok(Found::NotDue(NotDue::UntilTomorrow)));
    assert!(
        matches!(refreshed(60, 5), Ok(Found::Classes { .. })),
        "unread"
    );
    assert!(
        matches!(refreshed(20 * 60, 0), Ok(Found::Classes { .. })),
        "yesterday"
    );
}

/// **A chart page opened just after a run over its streams wants no run**
/// (#612), so it starts none and joins none: nothing it reads could hold
/// anything new, and every layer it reads is current. Later than the shortest
/// visit, or with something new to derive, it wants one.
#[test]
fn a_chart_page_opened_just_after_a_run_over_its_streams_wants_no_run() {
    let graph = Arc::new(Graph::default());
    run_over(&graph).expect("the run answers");
    let hevy = stream("hevy.workouts").expect("a stream");
    let scope = Scope::new([
        Part::Fetching(hevy.clone()),
        Part::Normalising(hevy),
        Part::Canonicalising(CanonicalLayer::Gym),
    ]);
    let wanted = |minutes: i64| -> bool {
        let mut ports = over(
            Arc::clone(&graph),
            Recorded::week().expect("a week"),
            Week(vec![SessionState::Prescribed, SessionState::ToBePrescribed]),
            Committer(None),
            Unreachable,
        )
        .expect("the ports build");
        ports.fetches =
            Fetches::minutes(&[("hevy.workouts", minutes, Some(minutes))]).expect("the fetches");
        ports.refetch = Refetch::SHORTEST_VISIT;
        runtime()
            .expect("a current-thread runtime builds")
            .block_on(Next::new(ports).wanted(&scope, now()))
            .expect("the store answers")
    };

    assert!(!wanted(1), "a minute after");
    assert!(wanted(50), "after the shortest visit");
    graph.reach("hevy.workouts");
    assert!(wanted(1), "with something new to derive");
}
