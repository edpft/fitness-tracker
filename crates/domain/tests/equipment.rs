//! Equipment facts: which is true on a date, and what a specific may be
//! (issue #450).

use domain::{
    gym::{
        equipment::{
            Band, BandLadder, BandName, Combination, EquipmentFacts, Fact, FactId, InvalidSettings,
            Maker, Model, Settings, Specific, Timeline,
        },
        exercise::Implement,
    },
    measure::Kg,
    prescription::LoadSteps,
    sequence::NonEmpty,
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

fn supaflex() -> Result<(Specific, Settings), Box<dyn std::error::Error>> {
    let band = |stars: &str, kg: u64| -> Result<Band, Box<dyn std::error::Error>> {
        Ok(Band {
            name: BandName::try_from(stars)?,
            at_twice_length: Kg::from_grams(kg * 1_000),
        })
    };
    let step = |bands: &[&str]| -> Result<Combination, Box<dyn std::error::Error>> {
        let names = bands
            .iter()
            .map(|band| BandName::try_from(*band))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Combination::new(
            NonEmpty::new(names).map_err(|_| "a step of no bands")?,
        ))
    };
    let settings = Settings::bands(
        vec![
            band("1/5 stars", 9)?,
            band("2/5 stars", 11)?,
            band("3/5 stars", 13)?,
            band("4/5 stars", 21)?,
            band("5/5 stars", 32)?,
        ],
        vec![
            step(&["4/5 stars"])?,
            step(&["1/5 stars", "2/5 stars"])?,
            step(&["1/5 stars", "1/5 stars"])?,
            step(&["3/5 stars"])?,
            step(&["2/5 stars"])?,
            step(&["1/5 stars"])?,
        ],
    )?;
    let specific = Specific {
        maker: Maker::try_from("Physical")?,
        model: Model::try_from("Supaflex")?,
    };
    Ok((specific, settings))
}

/// The operator's chart of 2026-10-15, at each band's force at twice its
/// resting length.
#[test]
fn bands_used_together_add_and_step_down_the_chart() {
    let (specific, settings) = supaflex().expect("the operator's chart");
    let ladder = BandLadder::of(&specific, &settings).expect("bands");

    let top = ladder.nearest(Kg::from_grams(21_000)).expect("4/5");
    assert_eq!(top.to_string(), "Physical Supaflex 4/5 stars");
    let next = ladder.after(&top).expect("a step down");
    assert_eq!(next.to_string(), "Physical Supaflex 1/5 stars + 2/5 stars");
    assert_eq!(next.nominal, Kg::from_grams(20_000));
    let two = ladder.after(&next).expect("a step down");
    assert_eq!(two.to_string(), "Physical Supaflex 2 × 1/5 stars");
    assert_eq!(two.nominal, Kg::from_grams(18_000));

    let last = ladder.nearest(Kg::from_grams(9_000)).expect("1/5");
    assert_eq!(ladder.after(&last), None, "after the last band, none");
}

/// 2026-10-12's dips, recorded as 7kg of assistance, are nearest one band.
#[test]
fn assistance_off_the_chart_is_its_nearest_step() {
    let (specific, settings) = supaflex().expect("the operator's chart");
    let ladder = BandLadder::of(&specific, &settings).expect("bands");
    let dips = ladder.nearest(Kg::from_grams(7_000)).expect("a band");
    assert_eq!(dips.to_string(), "Physical Supaflex 1/5 stars");
    assert_eq!(ladder.nearest(Kg::from_grams(4_000)), None, "nearer none");
}

#[test]
fn a_chart_steps_down_bands_the_range_has() {
    let name = |name: &str| BandName::try_from(name).expect("a name");
    let band = |stars: &str, kg: u64| Band {
        name: name(stars),
        at_twice_length: Kg::from_grams(kg * 1_000),
    };
    let step = |band: &str| Combination::new(NonEmpty::of(name(band), vec![]));
    assert_eq!(
        Settings::bands(vec![band("1/5", 9)], vec![step("2/5")]),
        Err(InvalidSettings::UnknownBand)
    );
    assert_eq!(
        Settings::bands(
            vec![band("1/5", 9), band("2/5", 11)],
            vec![step("1/5"), step("2/5")]
        ),
        Err(InvalidSettings::StepsOutOfOrder)
    );
    assert_eq!(
        Settings::bands(vec![band("1/5", 9), band("1/5", 11)], vec![step("1/5")]),
        Err(InvalidSettings::RepeatedBand)
    );
    assert_eq!(
        Settings::bands(vec![band("1/5", 9)], vec![]),
        Err(InvalidSettings::NoSteps)
    );
}
