//! Equipment facts: which is true on a date, and what a specific may be
//! (issue #450).

use domain::{
    gym::{
        equipment::{EquipmentFacts, Fact, FactId, InvalidSettings, LevelName, Settings, Timeline},
        exercise::Implement,
    },
    measure::Kg,
    prescription::LoadSteps,
};
use jiff::{Timestamp, civil::Date};

const fn fact<T>(id: i64, from: Option<Date>, value: T) -> Fact<T> {
    Fact {
        id: FactId::new(id),
        from,
        recorded_at: Timestamp::UNIX_EPOCH,
        value,
    }
}

#[test]
fn a_change_holds_from_its_date_and_what_came_before_holds_before_it() {
    let plates = LoadSteps::uniform(Kg::from_grams(2_500)).expect("2.5kg");
    let fractional = LoadSteps::uniform(Kg::from_grams(1_000)).expect("1kg");
    let changed = Date::constant(2026, 6, 1);
    let timeline = Timeline::new(vec![
        fact(2, Some(changed), fractional.clone()),
        fact(1, None, plates.clone()),
    ]);

    let before = timeline
        .on(Date::constant(2026, 5, 31))
        .expect("undated holds");
    assert_eq!(before.value, plates);
    let on = timeline
        .on(changed)
        .expect("the change holds from its date");
    assert_eq!(on.value, fractional);
}

#[test]
fn nothing_holds_before_a_first_dated_fact() {
    let machine = LoadSteps::uniform(Kg::from_grams(7_000)).expect("7kg");
    let timeline = Timeline::new(vec![fact(1, Some(Date::constant(2026, 1, 12)), machine)]);
    assert!(timeline.on(Date::constant(2025, 12, 31)).is_none());
}

#[test]
fn the_scales_on_a_date_are_each_kinds_fact_then() {
    let plates = LoadSteps::uniform(Kg::from_grams(2_500)).expect("2.5kg");
    let mut facts = EquipmentFacts::default();
    facts.kinds.insert(
        Implement::Barbell,
        Timeline::new(vec![fact(1, None, plates.clone())]),
    );
    let scales = facts.scales_on(Date::constant(2026, 10, 16));
    assert_eq!(scales.for_implement(Implement::Barbell), Some(&plates));
    assert_eq!(scales.for_implement(Implement::Dumbbell), None);
}

/// The Kneeling Easy Chin Dip: 7kg steps to its 84kg standard stack.
#[test]
fn a_stack_ends_on_one_of_its_own_loads() {
    let steps = LoadSteps::uniform(Kg::from_grams(7_000)).expect("7kg");
    assert!(Settings::stack(steps.clone(), Kg::from_grams(84_000)).is_ok());
    assert_eq!(
        Settings::stack(steps.clone(), Kg::from_grams(80_000)),
        Err(InvalidSettings::MostOffTheStack)
    );
    assert_eq!(
        Settings::stack(steps, Kg::NONE),
        Err(InvalidSettings::EmptyStack)
    );
}

#[test]
fn a_range_has_levels_and_none_twice() {
    let level = |name: &str| LevelName::try_from(name).expect("a name");
    assert!(Settings::levels(vec![level("1"), level("2")]).is_ok());
    assert_eq!(Settings::levels(vec![]), Err(InvalidSettings::NoLevels));
    assert_eq!(
        Settings::levels(vec![level("1"), level("1")]),
        Err(InvalidSettings::RepeatedLevel)
    );
}
