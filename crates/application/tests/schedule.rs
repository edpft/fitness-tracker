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

/// The operator's weekly availability since 25 August: three shorter
/// evenings and a longer Sunday morning.
fn ordinary() -> BTreeMap<TrainingSlot, SlotLength> {
    BTreeMap::from([
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            SlotLength::Shorter,
        ),
        (
            TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            SlotLength::Shorter,
        ),
        (
            TrainingSlot::new(Weekday::Friday, PartOfDay::Evening),
            SlotLength::Shorter,
        ),
        (
            TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            SlotLength::Longer,
        ),
    ])
}

const MONDAY_14_DECEMBER: Date = Date::constant(2026, 12, 14);

fn recorded_since_august() -> Result<Scheduling<InMemoryDiary>, String> {
    let diary = InMemoryDiary::default();
    diary
        .with(|recorded| {
            recorded.patterns.push(TrainingPattern::new(
                Date::constant(2026, 8, 25),
                london()?,
                ordinary(),
            ));
            Ok::<_, String>(())
        })
        .map_err(|error| error.to_string())??;
    Ok(Scheduling::new(diary))
}

/// **A week tapped is the run from its first changed day to its last**, as
/// `alter` would have been asked for it: Friday evening and Sunday morning
/// off the week commencing 14 December is 18 December for 3 days, with no
/// room to train.
#[test]
fn a_week_as_tapped_is_one_alteration_over_its_changed_days() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = recorded_since_august().expect("the diary holds the week");
    let mut tapped = ordinary();
    tapped.remove(&TrainingSlot::new(Weekday::Friday, PartOfDay::Evening));
    tapped.remove(&TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning));

    let recorded = runtime
        .block_on(scheduling.alter_week(Date::constant(2026, 12, 16), &tapped, "Christmas"))
        .expect("it records");

    assert_eq!(
        recorded,
        Alteration::new(
            FRIDAY_18_DECEMBER,
            three(),
            None,
            Some(BTreeMap::new()),
            "Christmas".to_owned()
        )
    );
}

/// **Changed from the week as it stands, not the ordinary week**: putting
/// back what an alteration took is a change, and leaving it off is not.
#[test]
fn a_week_is_changed_from_the_week_as_it_stands() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = recorded_since_august().expect("the diary holds the week");
    let mut tapped = ordinary();
    tapped.remove(&TrainingSlot::new(Weekday::Friday, PartOfDay::Evening));
    runtime
        .block_on(scheduling.alter_week(MONDAY_14_DECEMBER, &tapped, "Christmas"))
        .expect("it records");

    assert_eq!(
        runtime.block_on(scheduling.alter_week(MONDAY_14_DECEMBER, &tapped, "again")),
        Err(ScheduleError::Unchanged {
            what: "week as it stands"
        }),
    );
    let reverted = runtime
        .block_on(scheduling.alter_week(MONDAY_14_DECEMBER, &ordinary(), "home after all"))
        .expect("it records");
    assert_eq!(reverted.start(), FRIDAY_18_DECEMBER);
    assert_eq!(reverted.days(), NonZeroU8::MIN);
    assert_eq!(reverted.slots(), Some(&one_slot(Weekday::Friday)));
}

/// **A week saved with no reason is refused**, before anything is compared.
#[test]
fn a_week_with_no_reason_is_refused() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = recorded_since_august().expect("the diary holds the week");

    assert_eq!(
        runtime.block_on(scheduling.alter_week(MONDAY_14_DECEMBER, &BTreeMap::new(), " ")),
        Err(ScheduleError::NoReason),
    );
}

/// **From now on is from the week's Monday**, in the zone of the weekly
/// availability it replaces, and the same week again is refused.
#[test]
fn a_week_from_now_on_is_weekly_availability_from_its_monday() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = recorded_since_august().expect("the diary holds the week");
    let mut tapped = ordinary();
    tapped.insert(
        TrainingSlot::new(Weekday::Tuesday, PartOfDay::Morning),
        SlotLength::Shorter,
    );

    let recorded = runtime
        .block_on(scheduling.from_week(Date::constant(2026, 12, 17), tapped.clone()))
        .expect("it records");
    assert_eq!(
        recorded,
        TrainingPattern::new(
            MONDAY_14_DECEMBER,
            london().expect("a real zone"),
            tapped.clone()
        )
    );

    assert_eq!(
        runtime.block_on(scheduling.from_week(MONDAY_14_DECEMBER, tapped)),
        Err(ScheduleError::Unchanged {
            what: "weekly availability"
        }),
    );
}

/// **With no weekly availability there is no zone to take**, so it is
/// refused rather than assumed.
#[test]
fn a_week_from_now_on_needs_a_weekly_availability_first() {
    let runtime = runtime().expect("a current-thread runtime builds");
    let scheduling = Scheduling::new(InMemoryDiary::default());

    assert_eq!(
        runtime.block_on(scheduling.from_week(MONDAY_14_DECEMBER, ordinary())),
        Err(ScheduleError::NoWeeklyAvailability),
    );
}
