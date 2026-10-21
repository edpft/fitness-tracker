//! Body composition against the nutrition phase in force (#489), on the
//! school's real calendar for autumn 2026.

use std::num::NonZeroU8;

use domain::{
    analytical::{
        composition::{Measured, Method, Stated},
        recomposition::{GOAL, NutritionPhase, read},
    },
    body::{Component, Percentage},
    macrocycle::{Macrocycle, Phase},
    measure::Kg,
    plan::Span,
    schedule::{Holidays, PublicHoliday, SchoolHoliday, SchoolHolidayKind},
};
use jiff::civil::{Date, date};

const SCAN: Method = Method::BodyScan {
    algorithm: 218_300_672,
};

fn holiday(first: Date, last: Date) -> Option<SchoolHoliday> {
    let days = (last - first).get_days().checked_add(1)?;
    Some(SchoolHoliday::new(
        first,
        NonZeroU8::new(u8::try_from(days).ok()?)?,
    ))
}

/// Summer 2026 to Christmas 2026 as the school's feed published them.
fn autumn() -> Option<Macrocycle> {
    let school = Holidays::from_school(
        vec![
            holiday(date(2026, 7, 21), date(2026, 9, 5))?,
            holiday(date(2026, 10, 26), date(2026, 10, 30))?,
            holiday(date(2026, 12, 21), date(2027, 1, 3))?,
        ],
        Some(date(2027, 1, 3)),
    );
    let public = Holidays::from_public(
        vec![
            PublicHoliday::new(date(2026, 8, 31), "Summer bank holiday".to_owned())
                .naming(SchoolHolidayKind::Summer),
            PublicHoliday::new(date(2026, 12, 25), "Christmas Day".to_owned()),
        ],
        Some(date(2027, 12, 31)),
    );
    Macrocycle::on(date(2026, 10, 19), &school.and(public))
}

#[test]
fn preparatory_is_a_cut_and_the_rest_maintenance() {
    assert_eq!(
        NutritionPhase::assumed(Phase::Preparatory),
        NutritionPhase::Cut
    );
    assert_eq!(
        NutritionPhase::assumed(Phase::Competitive),
        NutritionPhase::Maintenance
    );
    assert_eq!(
        NutritionPhase::assumed(Phase::Transition),
        NutritionPhase::Maintenance
    );
}

/// The operator's regions, 2026-10-16 and 2026-10-19: a cut keeps the
/// diagonal, below which fat-free mass falls faster than fat.
#[test]
fn each_phase_has_its_own_region() {
    assert_eq!(
        NutritionPhase::Cut.region(),
        [(0.0, 0.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)]
    );
    assert_eq!(
        NutritionPhase::Maintenance.region(),
        [(0.0, 0.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)]
    );
    assert_eq!(
        NutritionPhase::Bulk.region(),
        [(0.0, 0.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)]
    );
}

/// The operator, 2026-10-20: the goal is fat-free mass changing by more than
/// fat, whatever the phase, and each phase's region is the goal restricted by
/// its intent.
#[test]
fn every_region_lies_within_the_goal() {
    assert_eq!(GOAL, [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]);
    for phase in [
        NutritionPhase::Cut,
        NutritionPhase::Maintenance,
        NutritionPhase::Bulk,
    ] {
        for (fat, fat_free) in phase.region() {
            assert!(fat_free >= fat, "{phase} at ({fat}, {fat_free})");
        }
    }
}

/// A Body Scan every day from term's start, fat falling a kilogram and
/// fat-free mass holding.
fn cutting() -> Vec<Measured> {
    date(2026, 9, 7)
        .series(jiff::Span::new().days(1))
        .take(43)
        .enumerate()
        .map(|(day, on)| {
            let wobble: i64 = if day % 2 == 0 { 100 } else { -100 };
            let fat = 19_000 - i64::try_from(day).unwrap_or(0) * 25 + wobble;
            let fat_free = 66_000 - wobble;
            Measured {
                on,
                method: SCAN,
                component: Component::Fat,
                stated: Stated::Mass(Kg::from_grams(u64::try_from(fat).unwrap_or(0))),
                weighed: Kg::from_grams(u64::try_from(fat + fat_free).unwrap_or(0)),
            }
        })
        .collect()
}

/// A two-week entry test and the four-week mesocycle after it.
const fn planned() -> [Span; 2] {
    [
        Span::new(date(2026, 9, 14), 2),
        Span::new(date(2026, 9, 28), 4),
    ]
}

/// **A point per mesocycle**, each read only on its own days.
#[test]
fn each_mesocycle_is_a_point_in_the_cut() {
    let autumn = autumn().expect("a macrocycle");
    let read = read(&cutting(), autumn, &planned(), date(2026, 10, 19));

    let [test, current] = read.mesocycles.as_slice() else {
        panic!("two mesocycles have started");
    };
    assert_eq!(test.phase, NutritionPhase::Cut);
    assert!(!test.running);
    let cell = test
        .by_method
        .first()
        .and_then(|one| one.cell)
        .expect("a cell");
    assert_eq!((cell.from, cell.to), (date(2026, 9, 20), date(2026, 9, 27)));
    assert!((cell.weeks() - 1.0).abs() < f64::EPSILON);

    assert!(current.running);
    let [scan] = current.by_method.as_slice() else {
        panic!("one method");
    };
    let cell = scan.cell.expect("a cell");
    assert_eq!(scan.method, SCAN);
    assert_eq!(
        (cell.from, cell.to),
        (date(2026, 10, 4), date(2026, 10, 19))
    );
    assert!(
        cell.fat.by < -cell.fat.noise,
        "fat falling beyond the noise"
    );
    assert!(
        cell.fat_free.by.abs() < cell.fat_free.noise,
        "fat-free within it"
    );
}

/// **The competitive mesocycle and the transition are maintenance.**
#[test]
fn the_competitive_mesocycle_is_maintenance() {
    let autumn = autumn().expect("a macrocycle");
    let mut planned = planned().to_vec();
    planned.push(Span::new(date(2026, 10, 26), 4));
    planned.push(Span::new(date(2026, 11, 23), 4));
    planned.push(Span::new(date(2026, 12, 21), 2));
    let read = read(&[], autumn, &planned, date(2027, 1, 1));

    let phases: Vec<NutritionPhase> = read.mesocycles.iter().map(|one| one.phase).collect();
    assert_eq!(
        phases,
        [
            NutritionPhase::Cut,
            NutritionPhase::Cut,
            NutritionPhase::Cut,
            NutritionPhase::Maintenance,
            NutritionPhase::Maintenance,
        ]
    );
}

/// **A mesocycle not yet started is not a point.**
#[test]
fn a_mesocycle_not_yet_started_is_not_read() {
    let autumn = autumn().expect("a macrocycle");
    let mut planned = planned().to_vec();
    planned.push(Span::new(date(2026, 10, 26), 4));
    let read = read(&cutting(), autumn, &planned, date(2026, 10, 19));

    assert_eq!(read.mesocycles.len(), 2);
}

/// **Fat-free mass is the weigh-in less its fat**, however the fat was stated.
#[test]
fn a_percentage_reads_on_the_same_axes() {
    let autumn = autumn().expect("a macrocycle");
    let measured: Vec<Measured> = cutting()
        .into_iter()
        .map(|one| {
            let Stated::Mass(fat) = one.stated else {
                return one;
            };
            let tenths = fat.as_grams() * 1_000 / one.weighed.as_grams();
            Measured {
                method: Method::Spreadsheets,
                stated: Stated::Percentage(
                    Percentage::from_fraction(&format!("0.{tenths:03}")).expect("a fraction"),
                ),
                ..one
            }
        })
        .collect();
    let read = read(&measured, autumn, &planned(), date(2026, 10, 19));
    let cell = read
        .mesocycles
        .last()
        .and_then(|one| one.by_method.first())
        .and_then(|one| one.cell)
        .expect("a cell");
    assert!(
        cell.fat.by < -cell.fat.noise,
        "fat falling beyond the noise"
    );
    assert!(
        (cell.fat.by + cell.fat_free.by).abs() < 1.0,
        "the mass moved little"
    );
}

#[test]
fn too_few_weigh_ins_give_no_cell() {
    let autumn = autumn().expect("a macrocycle");
    let early: Vec<Measured> = cutting().into_iter().take(10).collect();
    let read = read(&early, autumn, &planned(), date(2026, 10, 19));

    let test = read.mesocycles.first().expect("the entry test");
    assert_eq!(test.by_method.first().map(|one| one.cell), Some(None));
}
