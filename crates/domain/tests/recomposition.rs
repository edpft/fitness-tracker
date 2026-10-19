//! Body composition against the nutrition phase in force (#489), on the
//! school's real calendar for autumn 2026.

use std::num::NonZeroU8;

use domain::{
    analytical::{
        composition::{Direction, Measured, Method, Stated},
        recomposition::{Cell, Moved, NutritionPhase, Period, Standing, periods, read},
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

/// **Until the competitive mesocycle is planned, maintenance starts on no
/// known day**, and the cut runs on.
#[test]
fn maintenance_waits_for_the_competitive_mesocycle() {
    let autumn = autumn().expect("a macrocycle");
    let preparatory = [Span::new(date(2026, 9, 28), 4)];

    assert_eq!(
        periods(&autumn, &preparatory, date(2026, 10, 19)),
        [
            Period {
                phase: NutritionPhase::Cut,
                from: Some(date(2026, 9, 7)),
                to: None,
            },
            Period {
                phase: NutritionPhase::Maintenance,
                from: None,
                to: Some(date(2027, 1, 3)),
            },
        ]
    );
}

#[test]
fn the_competitive_mesocycle_starts_maintenance() {
    let autumn = autumn().expect("a macrocycle");
    let planned = [
        Span::new(date(2026, 9, 28), 4),
        Span::new(date(2026, 11, 23), 4),
    ];

    let [cut, maintenance] = periods(&autumn, &planned, date(2026, 10, 19));
    assert_eq!(cut.to, Some(date(2026, 11, 22)));
    assert_eq!(maintenance.from, Some(date(2026, 11, 23)));
}

/// **A term whose plan stopped short has no competitive phase**, so
/// maintenance is the holiday.
#[test]
fn without_a_competitive_mesocycle_the_transition_is_maintenance() {
    let autumn = autumn().expect("a macrocycle");

    let [cut, maintenance] = periods(&autumn, &[], date(2026, 12, 22));
    assert_eq!(cut.to, Some(date(2026, 12, 20)));
    assert_eq!(maintenance.from, Some(date(2026, 12, 21)));
}

const fn moved(by: f64, direction: Direction) -> Moved {
    Moved {
        by,
        noise: 0.2,
        direction,
    }
}

const fn cell(fat: Moved, fat_free: Moved) -> Cell {
    Cell {
        from: date(2026, 9, 13),
        to: date(2026, 10, 19),
        fat,
        fat_free,
    }
}

/// The operator's regions, 2026-10-16 and 2026-10-19.
#[test]
fn each_phase_has_its_own_region() {
    use Direction::{Falling, Maintaining, Rising};
    let falling = |by: f64| moved(-by, Falling);
    let holding = moved(0.0, Maintaining);
    let rising = |by: f64| moved(by, Rising);

    let cut = |fat, fat_free| NutritionPhase::Cut.succeeds(&cell(fat, fat_free));
    assert!(cut(falling(1.5), rising(0.5)));
    assert!(cut(falling(1.5), holding));
    assert!(cut(falling(1.5), falling(0.4)), "losing it more slowly");
    assert!(!cut(falling(0.4), falling(1.5)));
    assert!(!cut(holding, rising(0.5)));
    assert!(!cut(rising(0.5), rising(1.5)));

    let maintenance = |fat, fat_free| NutritionPhase::Maintenance.succeeds(&cell(fat, fat_free));
    assert!(maintenance(falling(1.0), holding));
    assert!(maintenance(holding, holding));
    assert!(maintenance(holding, rising(0.5)));
    assert!(!maintenance(falling(1.5), falling(0.4)));
    assert!(!maintenance(rising(0.3), rising(1.5)), "never fat rising");
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

#[test]
fn this_cut_falls_in_a_successful_cell() {
    let autumn = autumn().expect("a macrocycle");
    let read = read(&cutting(), autumn, &[], date(2026, 10, 19));

    let [cut, maintenance] = read.periods.as_slice() else {
        panic!("two periods");
    };
    let Standing::Read(methods) = &cut.standing else {
        panic!("the cut has started");
    };
    let [scan] = methods.as_slice() else {
        panic!("one method");
    };
    let cell = scan.cell.expect("a cell");
    assert_eq!(scan.method, SCAN);
    assert_eq!(
        cell.from,
        date(2026, 9, 13),
        "the first week wholly in the cut"
    );
    assert_eq!(cell.to, date(2026, 10, 19));
    assert_eq!(cell.fat.direction, Direction::Falling);
    assert_eq!(cell.fat_free.direction, Direction::Maintaining);
    assert!(NutritionPhase::Cut.succeeds(&cell));
    assert_eq!(maintenance.standing, Standing::NotStarted);
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
    let read = read(&measured, autumn, &[], date(2026, 10, 19));
    let Some(Standing::Read(methods)) = read.periods.first().map(|one| &one.standing) else {
        panic!("the cut has started");
    };
    let cell = methods.first().and_then(|one| one.cell).expect("a cell");
    assert_eq!(cell.fat.direction, Direction::Falling);
    assert!(
        (cell.fat.by + cell.fat_free.by).abs() < 1.0,
        "the mass moved little"
    );
}

#[test]
fn too_few_weigh_ins_give_no_cell() {
    let autumn = autumn().expect("a macrocycle");
    let early: Vec<Measured> = cutting().into_iter().take(10).collect();
    let read = read(&early, autumn, &[], date(2026, 10, 19));

    let Some(Standing::Read(methods)) = read.periods.first().map(|one| &one.standing) else {
        panic!("the cut has started");
    };
    assert_eq!(methods.first().map(|one| one.cell), Some(None));
}
