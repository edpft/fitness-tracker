//! Recording the ordinary week, altering it, and illness, with no frontend
//! (#670).
//!
//! Each refusal is the use case's, so each is pinned here against a diary held
//! in memory: a page offering the same thing gets the same answers.

use std::{collections::BTreeMap, num::NonZeroU8, sync::Mutex};

use application::{
    DiaryAuthor, DiaryStore, ScheduleError, StoreError,
    schedule::{Scheduling, weekdays},
};
use domain::{
    normalised::OperatorZone,
    schedule::{
        Alteration, Diary, Illness, InvalidIllness, PartOfDay, Skip, SlotLength, TrainingPattern,
        TrainingSlot,
    },
};
use jiff::civil::{Date, Weekday};

/// What was recorded, in the order it was recorded.
#[derive(Default)]
struct Recorded {
    patterns: Vec<TrainingPattern>,
    alterations: Vec<Alteration>,
    illnesses: Vec<Illness>,
}

#[derive(Default)]
struct InMemoryDiary(Mutex<Recorded>);

impl InMemoryDiary {
    fn with<T>(&self, read: impl FnOnce(&mut Recorded) -> T) -> Result<T, StoreError> {
        let mut recorded = self.0.lock().map_err(|_| StoreError::Corrupt {
            detail: "a poisoned lock".to_owned(),
        })?;
        Ok(read(&mut recorded))
    }
}

impl DiaryStore for InMemoryDiary {
    async fn diary(&self) -> Result<Diary, StoreError> {
        self.with(|recorded| {
            // Keyed on the day each began, as the store keys them.
            let mut illnesses: BTreeMap<Date, Illness> = BTreeMap::new();
            for illness in &recorded.illnesses {
                illnesses.insert(illness.from(), *illness);
            }
            Diary::new(recorded.patterns.clone(), recorded.alterations.clone())
                .with_illnesses(illnesses.into_values().collect())
        })
    }
}

impl DiaryAuthor for InMemoryDiary {
    async fn record_pattern(&self, pattern: &TrainingPattern) -> Result<(), StoreError> {
        self.with(|recorded| recorded.patterns.push(pattern.clone()))
    }

    async fn record_alteration(&self, alteration: &Alteration) -> Result<(), StoreError> {
        self.with(|recorded| recorded.alterations.push(alteration.clone()))
    }

    async fn record_illness(&self, illness: &Illness) -> Result<(), StoreError> {
        self.with(|recorded| recorded.illnesses.push(*illness))
    }

    async fn record_skip(&self, _: &Skip) -> Result<(), StoreError> {
        Ok(())
    }
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().build()
}

fn london() -> Result<OperatorZone, String> {
    OperatorZone::try_from("Europe/London".to_owned()).map_err(|error| error.to_string())
}

const FRIDAY_18_DECEMBER: Date = Date::constant(2026, 12, 18);

const fn three() -> NonZeroU8 {
    NonZeroU8::MIN.saturating_add(2)
}

fn one_slot(weekday: Weekday) -> BTreeMap<TrainingSlot, SlotLength> {
    BTreeMap::from([(
        TrainingSlot::new(weekday, PartOfDay::Evening),
        SlotLength::Shorter,
    )])
}

/// **The weekdays to ask about are the run's, in the order it meets them.**
#[test]
fn a_run_asks_about_the_weekdays_it_meets_in_run_order() {
    assert_eq!(
        weekdays(FRIDAY_18_DECEMBER, three()),
        vec![Weekday::Friday, Weekday::Saturday, Weekday::Sunday],
    );
}

/// **A slot on a weekday the run does not meet is refused**, and nothing is
/// recorded: Friday 18 December for 3 days has no Tuesday in it.
#[test]
fn a_slot_outside_the_run_is_refused() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());
    let zone = london().expect("a real zone");

    let refused = runtime.block_on(scheduling.alter(
        FRIDAY_18_DECEMBER,
        three(),
        None,
        Some(one_slot(Weekday::Tuesday)),
        "Christmas at my parents'",
    ));
    assert!(
        matches!(refused, Err(ScheduleError::SlotOutsideRun { .. })),
        "a Tuesday evening is refused, got {refused:?}"
    );

    let recorded = runtime.block_on(scheduling.alter(
        FRIDAY_18_DECEMBER,
        three(),
        Some(zone),
        Some(one_slot(Weekday::Saturday)),
        "Christmas at my parents'",
    ));
    assert!(
        recorded.is_ok(),
        "a Saturday evening is in it: {recorded:?}"
    );
    assert_eq!(
        runtime
            .block_on(scheduling.diary())
            .expect("the diary reads")
            .alterations()
            .len(),
        1,
        "only the alteration that was not refused is recorded"
    );
}

/// **An alteration with no reason is refused**, a blank one included.
#[test]
fn an_alteration_with_no_reason_is_refused() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());

    for reason in ["", "   "] {
        let refused = runtime.block_on(scheduling.alter(
            FRIDAY_18_DECEMBER,
            three(),
            None,
            Some(BTreeMap::new()),
            reason,
        ));
        assert_eq!(
            refused,
            Err(ScheduleError::NoReason),
            "{reason:?} is no reason"
        );
    }
    assert!(
        runtime
            .block_on(scheduling.diary())
            .expect("the diary reads")
            .alterations()
            .is_empty(),
        "nothing is recorded"
    );
}

/// **The week recorded is the week stated**: its date, its zone and each
/// slot's length, and nothing else.
#[test]
fn the_ordinary_week_is_recorded_as_stated() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());
    let zone = london().expect("a real zone");
    let slots = BTreeMap::from([
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            SlotLength::Shorter,
        ),
        (
            TrainingSlot::new(Weekday::Saturday, PartOfDay::Morning),
            SlotLength::Longer,
        ),
    ]);
    let from = Date::constant(2026, 11, 2);

    let pattern = runtime
        .block_on(scheduling.add(from, zone.clone(), slots.clone()))
        .expect("it records");

    let stated = TrainingPattern::new(from, zone, slots);
    assert_eq!(pattern, stated);
    assert_eq!(
        runtime
            .block_on(scheduling.diary())
            .expect("the diary reads")
            .patterns(),
        &[stated],
    );
}

/// **Ending an illness that is not open is refused**, whether there has never
/// been one or the last has already ended.
#[test]
fn better_with_no_illness_open_is_refused() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());
    let from = Date::constant(2026, 11, 1);

    assert_eq!(
        runtime.block_on(scheduling.better(from)),
        Err(ScheduleError::NoIllnessOpen),
        "never ill"
    );

    runtime.block_on(scheduling.ill(from)).expect("ill records");
    let ended = runtime
        .block_on(scheduling.better(Date::constant(2026, 11, 4)))
        .expect("better records");
    assert_eq!(ended.ended_on(), Some(Date::constant(2026, 11, 4)));

    assert_eq!(
        runtime.block_on(scheduling.better(Date::constant(2026, 11, 5))),
        Err(ScheduleError::NoIllnessOpen),
        "already better"
    );
}

/// **Better before the illness began is refused**, and the illness stays open.
#[test]
fn better_before_ill_is_refused() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());
    let from = Date::constant(2026, 11, 1);
    runtime.block_on(scheduling.ill(from)).expect("ill records");

    assert_eq!(
        runtime.block_on(scheduling.better(Date::constant(2026, 10, 30))),
        Err(ScheduleError::Illness(InvalidIllness::EndsBeforeItBegins {
            from
        })),
    );
    assert_eq!(
        runtime.block_on(scheduling.open_illness()),
        Ok(Illness::new(from)),
        "still open"
    );
}
