//! Body composition, one series per method (#256).

use domain::{
    analytical::composition::{Measured, Method, Stated, Unit, by_method},
    body::{Component, InvalidPercentage, Percentage},
    measure::Kg,
};
use jiff::civil::date;

const SCAN: Method = Method::BodyScan {
    algorithm: 218_300_672,
};

fn sheet(day: i8, component: Component, fraction: &str) -> Result<Measured, InvalidPercentage> {
    Ok(Measured {
        on: date(2018, 1, day),
        method: Method::Spreadsheets,
        component,
        stated: Stated::Percentage(Percentage::from_fraction(fraction)?),
        weighed: Kg::from_grams(73_900),
    })
}

const fn scan(method: Method, day: i8, component: Component, grams: u64) -> Measured {
    Measured {
        on: date(2026, 10, day),
        method,
        component,
        stated: Stated::Mass(Kg::from_grams(grams)),
        weighed: Kg::from_grams(83_648),
    }
}

#[test]
fn fat_is_a_percentage_whatever_its_source_stated() {
    let components = by_method(&[
        scan(SCAN, 16, Component::Fat, 17_610),
        sheet(1, Component::Fat, "0.247").expect("a fraction"),
    ]);
    let [fat] = components.as_slice() else {
        panic!("one component: {components:?}");
    };

    assert_eq!(fat.unit, Unit::Percentage);
    let methods: Vec<Method> = fat.series.iter().map(|one| one.method).collect();
    assert_eq!(
        methods,
        [Method::Spreadsheets, SCAN],
        "the earliest method first"
    );
    let values: Vec<String> = fat
        .series
        .iter()
        .flat_map(|one| &one.readings)
        .map(|reading| format!("{} {:.1}", reading.on, reading.value))
        .collect();
    assert_eq!(values, ["2018-01-01 24.7", "2026-10-16 21.1"]);
}

#[test]
fn what_only_a_scale_measures_is_its_mass() {
    let components = by_method(&[scan(SCAN, 16, Component::Bone, 3_290)]);
    let [bone] = components.as_slice() else {
        panic!("one component: {components:?}");
    };
    assert_eq!(bone.unit, Unit::Kilograms);
    assert_eq!(
        bone.series
            .first()
            .and_then(|one| one.readings.first())
            .map(|reading| reading.value),
        Some(3.29)
    );
}

#[test]
fn copies_of_a_day_are_one_reading_and_two_algorithms_are_two_series() {
    let first = Method::BodyScan {
        algorithm: 218_300_416,
    };
    let components = by_method(&[
        sheet(1, Component::Fat, "0.247").expect("a fraction"),
        sheet(1, Component::Fat, "0.247").expect("a fraction"),
        sheet(2, Component::Fat, "0.246").expect("a fraction"),
        scan(first, 11, Component::Fat, 18_000),
        scan(SCAN, 12, Component::Fat, 17_900),
    ]);
    let [fat] = components.as_slice() else {
        panic!("one component: {components:?}");
    };
    let counts: Vec<(Method, usize)> = fat
        .series
        .iter()
        .map(|one| (one.method, one.readings.len()))
        .collect();
    assert_eq!(counts, [(Method::Spreadsheets, 2), (first, 1), (SCAN, 1)]);
}

#[test]
fn components_come_in_a_fixed_order_and_only_where_measured() {
    let components = by_method(&[
        scan(SCAN, 16, Component::Bone, 3_290),
        sheet(1, Component::Water, "0.566").expect("a fraction"),
        scan(SCAN, 16, Component::Fat, 17_610),
    ]);
    let order: Vec<Component> = components.iter().map(|one| one.component).collect();
    assert_eq!(order, [Component::Fat, Component::Water, Component::Bone]);
}
