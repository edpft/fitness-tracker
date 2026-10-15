//! `next` as a use case (#397): what it reports, and what it carries on past.
//!
//! **Every port is a test adapter.** What a walk lands, what a derivation
//! writes and which state a session is in have suites of their own; here the
//! question is the run: which steps it reports, in what order, and that a
//! source it cannot reach costs that step and nothing else (§ 36).

use std::{collections::BTreeMap, sync::Mutex};

use application::{
    CanonicalCyclingSessionStore, CanonicalGymSessionStore, CanonicalWeighInStore, ClassSource,
    ClassesListed, ClassesRead, DiaryAuthor, DiaryStore, ExtractionError, ExtractionRunLog,
    HolidayCalendar, NextLock, NormalisationError, NormalisationSummary,
    NormalisedCyclingSessionReader, NormalisedGymSessionReader, NormalisedWeighInReader, PlanStore,
    PrescriptionError, RunLockError, RunSummary, SourceError, StoreError, WorkoutNormaliser,
    commit::{CommitError, Due},
    cycling::RidingError,
    microcycle::{Session, Standing},
    next::{
        CanonicalPorts, CyclingPrescribed, CyclingSessions, Done, Found, GymPrescribed,
        GymSessions, MesocycleCommitter, Next, NextError, NextPorts, NoAccess, Prescribed,
        Progress, Records, SkipRefused, StandingReader, Step, StepFailure, Uncollected, Walks,
        WhatIsNext, uncollected,
    },
    reschedule::Reschedule,
};
use domain::{
    body::{CanonicalWeighIn, WeighInAccount, WeighInCount},
    canonical::SessionCount,
    cycling::{CanonicalCyclingSession, CyclingMesocycle},
    gym::{CanonicalGymSession, NormalisedGymSession},
    landing::{
        EventCount, ExtractionRun, FetchedAt, LandingStream, RecordCount, RunId, RunOutcome,
    },
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
struct Walking(Vec<(LandingStream, Result<RunSummary, ExtractionError>)>);

impl Walks for Walking {
    fn count(&self) -> usize {
        self.0.len()
    }

    fn stream(&self, at: usize) -> Option<&LandingStream> {
        self.0.get(at).map(|(stream, _)| stream)
    }

    async fn walk(&self, at: usize) -> Option<Result<RunSummary, ExtractionError>> {
        self.0.get(at).map(|(_, result)| result.clone())
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

/// A derivation that writes this many sessions.
struct Deriving(usize);

impl WorkoutNormaliser for Deriving {
    async fn normalise(&self) -> Result<NormalisationSummary, NormalisationError> {
        Ok(NormalisationSummary {
            run_id: NormalisationRunId::from(1),
            records_read: RecordCount::from(0),
            workouts_written: WorkoutCount::from(self.0),
            records_composed: RecordCount::from(0),
            records_superseded: RecordCount::from(0),
            workouts_retracted: WorkoutCount::from(0),
            retractions_read: RecordCount::from(0),
            records_refused: RecordCount::from(0),
            refusals_recorded: RefusalCount::from(0),
        })
    }
}

/// An empty record, canonicalised to nothing.
struct Empty;

impl NormalisedGymSessionReader for Empty {
    async fn all(&self) -> Result<Vec<NormalisedGymSession>, StoreError> {
        Ok(Vec::new())
    }
}

impl NormalisedCyclingSessionReader for Empty {
    async fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
        Ok(Vec::new())
    }
}

impl CanonicalCyclingSessionStore for Empty {
    async fn replace(
        &self,
        sessions: Vec<CanonicalCyclingSession>,
    ) -> Result<SessionCount, StoreError> {
        Ok(SessionCount::from(sessions.len()))
    }

    async fn count(&self) -> Result<SessionCount, StoreError> {
        Ok(SessionCount::from(0))
    }
}

impl NormalisedWeighInReader for Empty {
    async fn all(&self) -> Result<Vec<WeighInAccount>, StoreError> {
        Ok(Vec::new())
    }
}

impl CanonicalWeighInStore for Empty {
    async fn replace(&self, weigh_ins: Vec<CanonicalWeighIn>) -> Result<WeighInCount, StoreError> {
        Ok(WeighInCount::from(weigh_ins.len()))
    }

    async fn count(&self) -> Result<WeighInCount, StoreError> {
        Ok(WeighInCount::from(0))
    }
}

impl CanonicalGymSessionStore for Empty {
    async fn replace(
        &self,
        sessions: Vec<CanonicalGymSession>,
    ) -> Result<SessionCount, StoreError> {
        Ok(SessionCount::from(sessions.len()))
    }

    async fn all(&self) -> Result<Vec<CanonicalGymSession>, StoreError> {
        Ok(Vec::new())
    }

    async fn count(&self) -> Result<SessionCount, StoreError> {
        Ok(SessionCount::from(0))
    }
}

/// A catalogue with nothing new to list or read.
struct Converged;

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
}

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
    Deriving,
    Walking,
    Deriving,
    Walking,
    Deriving,
    CanonicalPorts<Empty, Empty, Empty, Empty, Empty, Empty>,
    Converged,
    H,
    Recorded,
    NoPlans,
    Week,
    Committer,
    Unplanned,
    Dayless,
    Lock,
>;

/// Whether another run of `next` holds the lock.
pub struct Lock {
    held: bool,
}

impl NextLock for Lock {
    type Guard = ();

    fn try_acquire(&self) -> Result<(), RunLockError> {
        if self.held {
            Err(RunLockError::Held)
        } else {
            Ok(())
        }
    }
}

/// The gym walked and landed two; Peloton has no credential; the school
/// calendar does not answer.
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
    Ok(NextPorts {
        gym_records: Records {
            entry: stream("hevy.workouts")?,
            walks: Ok(Walking(vec![(stream("hevy.workouts")?, Ok(landed(2)))])),
            normaliser: Deriving(146),
        },
        cycling_records: Records {
            entry: stream("peloton.rides")?,
            walks: Err(NoAccess {
                detail: "no peloton credential".to_owned(),
            }),
            normaliser: Deriving(156),
        },
        weigh_in_records: Records {
            entry: stream("withings.measurements")?,
            walks: Ok(Walking(vec![(
                stream("withings.measurements")?,
                Ok(landed(1)),
            )])),
            normaliser: Deriving(227),
        },
        canonical: CanonicalPorts {
            normalised_gym: Empty,
            canonical_gym: Empty,
            normalised_cycling: Empty,
            canonical_cycling: Empty,
            normalised_weigh_ins: Empty,
            canonical_weigh_ins: Empty,
        },
        classes: Ok(Converged),
        holidays,
        diary,
        plans: NoPlans,
        microcycle: week,
        committing: committer,
        gym: Unplanned,
        cycling: Dayless,
        lock: Lock { held: false },
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
        .block_on(Next::new(ports).run(now(), None, &Heard::default()))
        .expect("the run answers");

    let steps: Vec<(&Step, bool)> = report
        .run
        .iter()
        .map(|done| (&done.step, done.outcome.is_ok()))
        .collect();
    let hevy = stream("hevy.workouts").expect("a stream");
    let peloton = stream("peloton.rides").expect("a stream");
    let withings = stream("withings.measurements").expect("a stream");
    assert_eq!(
        steps,
        vec![
            (&Step::Fetching(hevy.clone()), true),
            (&Step::Normalising(hevy), true),
            (&Step::Canonicalising(Discipline::Gym), true),
            (&Step::Fetching(peloton.clone()), false),
            (&Step::Normalising(peloton), true),
            (&Step::Canonicalising(Discipline::Cycling), true),
            (&Step::Fetching(withings.clone()), true),
            (&Step::Normalising(withings), true),
            (&Step::CanonicalisingWeighIns, true),
            (&Step::RefreshingClasses, true),
        ]
    );
    assert!(matches!(
        report.run.get(3).map(|done| &done.outcome),
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
    ports.cycling_records.walks = Ok(Walking(vec![
        (stream("peloton.rides").expect("a stream"), Ok(landed(50))),
        (
            stream("peloton.ride_samples").expect("a stream"),
            Err(ExtractionError::Source(SourceError::Unauthorised)),
        ),
    ]));
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, &Heard::default()))
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
        ]
    );
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
        .block_on(Next::new(ports).run(now(), None, &heard))
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
        Committer(Some(CommitError::NoParameters)),
        Answering,
    )
    .expect("the ports build");
    let report = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, &Heard::default()))
        .expect("the run answers");

    assert!(report.run.iter().any(|done| done.step == Step::Committing
        && done.outcome == Err(StepFailure::Commit(CommitError::NoParameters))));
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
        .block_on(next.run(now(), Some("ill"), &Heard::default()))
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
        .block_on(Next::new(ports).run(now(), Some("ill"), &Heard::default()));

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
        .block_on(Next::new(ports).run(now(), None, &heard));

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
        .block_on(Next::new(ports).run(now(), None, &Heard::default()))
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
    ports.lock = Lock { held: true };
    let heard = Heard::default();
    let refused = runtime()
        .expect("a current-thread runtime builds")
        .block_on(Next::new(ports).run(now(), None, &heard));

    assert_eq!(refused.err(), Some(NextError::AlreadyRunning));
    assert!(heard.events().is_empty());
}

/// When each stream last succeeded, as the run log would say.
struct Collected(Vec<(LandingStream, &'static str)>);

impl ExtractionRunLog for Collected {
    async fn begin(&self, _stream: &LandingStream, _at: FetchedAt) -> Result<RunId, StoreError> {
        Ok(RunId::from(1))
    }

    async fn finish(&self, _run: RunId, _outcome: RunOutcome) -> Result<(), StoreError> {
        Ok(())
    }

    async fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> Result<Option<ExtractionRun>, StoreError> {
        let Some((_, at)) = self.0.iter().find(|(collected, _)| collected == stream) else {
            return Ok(None);
        };
        let at = FetchedAt::try_from(*at).map_err(|error| StoreError::Corrupt {
            detail: error.to_string(),
        })?;
        Ok(Some(ExtractionRun::new(
            RunId::from(1),
            stream.clone(),
            at,
            RunOutcome::Succeeded {
                finished_at: at,
                events_seen: EventCount::from(0),
                records_landed: RecordCount::from(0),
            },
        )))
    }
}

/// **#434: a window that closed after its discipline was last collected is
/// what a scheduled run is for.** Monday evening's gym closes on the Tuesday
/// morning; Hevy was last collected during it, Peloton after the previous
/// Wednesday's ride had closed.
#[test]
fn a_window_closed_since_its_discipline_was_collected_is_uncollected() {
    let diary = Recorded::week().expect("a week");
    let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
    let hevy = stream("hevy.workouts").expect("a stream");
    let peloton = stream("peloton.rides").expect("a stream");
    let collects = [
        (Discipline::Gym, hevy.clone()),
        (Discipline::Cycling, peloton.clone()),
    ];
    let tuesday = DayPart::new(Date::constant(2026, 10, 13), PartOfDay::Morning);
    let runtime = runtime().expect("a current-thread runtime builds");

    let before = Collected(vec![
        (hevy.clone(), "2026-10-12T19:00:00Z"),
        (peloton.clone(), "2026-10-08T08:00:00Z"),
    ]);
    assert_eq!(
        runtime
            .block_on(uncollected(&diary, &before, &collects, &zone, tuesday))
            .expect("the store answers"),
        vec![Uncollected {
            discipline: Discipline::Gym,
            closed: tuesday,
            collected: Some(DayPart::new(
                Date::constant(2026, 10, 12),
                PartOfDay::Evening
            )),
        }],
    );

    let after = Collected(vec![
        (hevy, "2026-10-13T05:00:30Z"),
        (peloton, "2026-10-08T08:00:00Z"),
    ]);
    assert_eq!(
        runtime
            .block_on(uncollected(&diary, &after, &collects, &zone, tuesday))
            .expect("the store answers"),
        Vec::new(),
        "a run since the close leaves nothing for the timer"
    );
}

/// **A source never collected is uncollected once a window closes**, which is
/// how a missing credential is said on every scheduled run rather than once.
#[test]
fn a_discipline_never_collected_is_uncollected() {
    let diary = Recorded::week().expect("a week");
    let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
    let peloton = stream("peloton.rides").expect("a stream");
    let thursday = DayPart::new(Date::constant(2026, 10, 15), PartOfDay::Morning);

    let found = runtime()
        .expect("a current-thread runtime builds")
        .block_on(uncollected(
            &diary,
            &Collected(Vec::new()),
            &[(Discipline::Cycling, peloton)],
            &zone,
            thursday,
        ))
        .expect("the store answers");
    assert_eq!(
        found,
        vec![Uncollected {
            discipline: Discipline::Cycling,
            closed: thursday,
            collected: None,
        }],
    );
}
