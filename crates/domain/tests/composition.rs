//! Body composition, one series per method (#256).

use domain::{
    analytical::composition::{
        ComponentSeries, Direction, Measured, Method, MethodSeries, Reading, Stated, Unit,
        by_method,
    },
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

/// Each chart's heading and its figures, as `date value method`.
fn drawn(components: &[ComponentSeries]) -> Vec<(Component, Unit, Vec<String>)> {
    components
        .iter()
        .map(|chart| {
            let figures = chart
                .series
                .iter()
                .flat_map(|one| {
                    one.readings.iter().map(|reading| {
                        format!("{} {:.2} {}", reading.on, reading.value, one.method)
                    })
                })
                .collect();
            (chart.component, chart.unit, figures)
        })
        .collect()
}

#[test]
fn fat_is_charted_in_kilograms_and_as_a_percentage_whatever_its_source_stated() {
    let components = by_method(&[
        scan(SCAN, 16, Component::Fat, 17_610),
        scan(SCAN, 16, Component::FatFree, 66_038),
        sheet(1, Component::Fat, "0.247").expect("a fraction"),
    ]);

    assert_eq!(
        drawn(&components),
        [
            (
                Component::Fat,
                Unit::Kilograms,
                vec![
                    "2018-01-01 18.25 spreadsheets".to_owned(),
                    "2026-10-16 17.61 Body Scan, algorithm 218300672".to_owned(),
                ]
            ),
            (
                Component::FatFree,
                Unit::Kilograms,
                vec![
                    "2018-01-01 55.65 spreadsheets".to_owned(),
                    "2026-10-16 66.04 Body Scan, algorithm 218300672".to_owned(),
                ]
            ),
            (
                Component::Fat,
                Unit::Percentage,
                vec![
                    "2018-01-01 24.70 spreadsheets".to_owned(),
                    "2026-10-16 21.05 Body Scan, algorithm 218300672".to_owned(),
                ]
            ),
        ],
        "the earliest method first; fat-free mass as stated, or weighed less fat"
    );
}

#[test]
fn a_method_that_states_fat_free_mass_is_never_also_given_weighed_less_fat() {
    let components = by_method(&[
        scan(SCAN, 16, Component::Fat, 17_610),
        scan(SCAN, 16, Component::FatFree, 66_000),
    ]);
    let fat_free = components
        .iter()
        .find(|chart| chart.component == Component::FatFree)
        .expect("a fat-free chart");
    let values: Vec<f64> = fat_free
        .series
        .iter()
        .flat_map(|one| &one.readings)
        .map(|reading| reading.value)
        .collect();
    assert_eq!(values, [66.0]);
}

#[test]
fn nothing_but_fat_and_fat_free_mass_is_charted() {
    let components = by_method(&[
        scan(SCAN, 16, Component::Bone, 3_290),
        scan(SCAN, 16, Component::Muscle, 62_740),
        scan(SCAN, 16, Component::Water, 48_200),
        scan(SCAN, 16, Component::ExtracellularWater, 19_300),
        sheet(1, Component::Water, "0.566").expect("a fraction"),
    ]);
    assert!(components.is_empty(), "{components:?}");
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
    for chart in &components {
        let counts: Vec<(Method, usize)> = chart
            .series
            .iter()
            .map(|one| (one.method, one.readings.len()))
            .collect();
        assert_eq!(counts, [(Method::Spreadsheets, 2), (first, 1), (SCAN, 1)]);
    }
    assert_eq!(components.len(), 3);
}

/// One figure a day from 1 January 2026, for as many values as are given.
fn daily(values: &[f64]) -> Vec<Reading<f64>> {
    date(2026, 1, 1)
        .series(jiff::Span::new().days(1))
        .zip(values.iter().copied())
        .map(|(on, value)| Reading { on, value })
        .collect()
}

/// `days` figures around `level`, a tenth either side on alternate days.
fn level(level: f64, days: usize) -> Vec<f64> {
    (0..days)
        .map(|day| {
            if day % 2 == 0 {
                level + 0.1
            } else {
                level - 0.1
            }
        })
        .collect()
}

#[test]
fn the_trend_breaks_where_fewer_than_four_of_seven_days_were_measured() {
    let mut readings = daily(&level(18.0, 10));
    readings.extend(daily(&level(18.0, 30)).into_iter().skip(20));
    let series = MethodSeries::of(SCAN, readings);

    let runs: Vec<(String, String)> = series
        .trend
        .iter()
        .filter_map(|run| Some((run.first()?.on.to_string(), run.last()?.on.to_string())))
        .collect();
    assert_eq!(
        runs,
        [
            ("2026-01-04".to_owned(), "2026-01-13".to_owned()),
            ("2026-01-24".to_owned(), "2026-01-30".to_owned()),
        ],
        "a week with 4 measured days has a trend, one with 3 does not"
    );
}

#[test]
fn a_change_over_28_days_counts_only_beyond_the_methods_own_scatter() {
    let mut values = level(10.0, 35);
    values.extend(level(12.0, 35));
    let series = MethodSeries::of(SCAN, daily(&values));

    let on = |month: i8, day: i8| {
        series
            .changes
            .iter()
            .find(|change| change.on == date(2026, month, day))
            .map(|change| change.direction)
    };
    assert_eq!(on(1, 31), None, "no trend 28 days before");
    assert_eq!(on(2, 1), Some(Direction::Maintaining), "both weeks at 10");
    assert_eq!(on(2, 15), Some(Direction::Rising), "12 against 10");
    let noise = series.changes.first().map(|change| change.noise);
    assert!(
        noise.is_some_and(|noise| noise > 0.1 && noise < 0.5),
        "1.96 × the scatter × √(2/7): {noise:?}"
    );
}

#[test]
fn without_two_pairs_of_consecutive_days_no_direction_is_read() {
    let readings: Vec<Reading<f64>> = daily(&level(18.0, 60)).into_iter().step_by(2).collect();
    let series = MethodSeries::of(SCAN, readings);
    assert!(series.changes.is_empty(), "{:?}", series.changes);
}
