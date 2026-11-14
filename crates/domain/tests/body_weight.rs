//! Body weight as one time series (#250).

use std::num::NonZeroU8;

use domain::{
    analytical::{
        Weighed,
        body_weight::{Series, Trend, aims, body_weight, trend},
        nutrition_phase::NutritionPhase,
        range::{Over, Range, Ranges},
    },
    canonical::Occurred,
    macrocycle::Macrocycle,
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
    plan::Span,
    schedule::{Holidays, PublicHoliday, SchoolHoliday, SchoolHolidayKind},
};
use jiff::civil::{Date, date};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn measured(on: Date, hour: i8, grams: u64) -> Fallible<Weighed> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let at = on
        .at(hour, 30, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    Ok(Weighed {
        at: Occurred::At(StartedAt::new(at, zone)),
        mass: Kg::from_grams(grams),
        fat_free: None,
    })
}

const fn dated(on: Date, grams: u64) -> Weighed {
    Weighed {
        at: Occurred::On(on),
        mass: Kg::from_grams(grams),
        fat_free: None,
    }
}

#[test]
fn weigh_ins_come_out_by_day_then_by_clock_whatever_order_they_went_in() {
    let evening = measured(date(2026, 7, 14), 21, 84_100).expect("a weigh-in");
    let small_hours = measured(date(2026, 7, 14), 0, 83_600).expect("a weigh-in");
    let series = body_weight(vec![
        evening.clone(),
        dated(date(2026, 7, 15), 83_900),
        small_hours.clone(),
        dated(date(2016, 1, 4), 66_300),
    ]);
    assert_eq!(
        series,
        vec![
            dated(date(2016, 1, 4), 66_300),
            small_hours,
            evening,
            dated(date(2026, 7, 15), 83_900),
        ]
    );
}

/// A weigh-in on each of `days` consecutive days from `from`, all at `grams`
/// except where `skip` says there was none.
fn daily(from: Date, days: i32, grams: u64, skip: &[Date]) -> Fallible<Vec<Weighed>> {
    let mut series = Vec::new();
    for offset in 0..days {
        let on = from.checked_add(jiff::Span::new().days(offset))?;
        if !skip.contains(&on) {
            series.push(dated(on, grams));
        }
    }
    Ok(series)
}

fn days_of(runs: &[Vec<Trend>]) -> Vec<(Date, Date)> {
    runs.iter()
        .filter_map(|run| Some((run.first()?.on, run.last()?.on)))
        .collect()
}

#[test]
fn the_trend_is_the_mean_of_the_week_ending_on_the_day() {
    // Monday to Sunday 2026-10-05..11, weekend heavier.
    let series = vec![
        dated(date(2026, 10, 5), 84_000),
        dated(date(2026, 10, 6), 84_000),
        dated(date(2026, 10, 7), 84_000),
        dated(date(2026, 10, 8), 84_000),
        dated(date(2026, 10, 9), 84_000),
        dated(date(2026, 10, 10), 85_000),
        dated(date(2026, 10, 11), 85_000),
        dated(date(2026, 10, 12), 84_000),
    ];
    let runs = trend(&series);
    assert_eq!(
        days_of(&runs),
        vec![(date(2026, 10, 8), date(2026, 10, 12))]
    );
    let sunday = runs
        .iter()
        .flatten()
        .find(|day| day.on == date(2026, 10, 11))
        .expect("a trend on the Sunday");
    // (5 × 84 + 2 × 85) / 7 = 84.2857…, to the nearest gram.
    assert_eq!(sunday.mass, Kg::from_grams(84_286));
    let monday = runs.iter().flatten().last().expect("a trend on the Monday");
    assert_eq!(
        monday.mass, sunday.mass,
        "a week holds one weekend whichever day it ends on"
    );
}

#[test]
fn one_missed_weigh_in_does_not_break_the_line() {
    let series = daily(date(2026, 9, 1), 30, 84_000, &[date(2026, 9, 15)]).expect("a series");
    assert_eq!(
        days_of(&trend(&series)),
        vec![(date(2026, 9, 4), date(2026, 9, 30))]
    );
}

#[test]
fn a_week_with_three_weigh_ins_breaks_it() {
    // 2026-09-12..15 missed: every week ending 09-15..09-18 holds 3 weighed days.
    let skip = [
        date(2026, 9, 12),
        date(2026, 9, 13),
        date(2026, 9, 14),
        date(2026, 9, 15),
    ];
    let series = daily(date(2026, 9, 1), 30, 84_000, &skip).expect("a series");
    assert_eq!(
        days_of(&trend(&series)),
        vec![
            (date(2026, 9, 4), date(2026, 9, 14)),
            (date(2026, 9, 19), date(2026, 9, 30)),
        ]
    );
}

#[test]
fn a_long_gap_has_no_trend_and_nothing_runs_past_the_last_weigh_in() {
    let mut series = daily(date(2024, 12, 1), 10, 80_000, &[]).expect("a series");
    series.extend(daily(date(2026, 1, 11), 10, 84_000, &[]).expect("a series"));
    assert_eq!(
        days_of(&trend(&series)),
        vec![
            (date(2024, 12, 4), date(2024, 12, 13)),
            (date(2026, 1, 14), date(2026, 1, 20)),
        ]
    );
}

#[test]
fn no_weigh_ins_is_no_trend() {
    assert!(trend(&[]).is_empty());
}

#[test]
fn the_aim_is_the_cut_s_where_a_plan_says_cut() {
    let autumn = autumn().expect("the macrocycle");
    let mesocycles = [
        Span::new(date(2026, 9, 14), 1),
        Span::new(date(2026, 9, 28), 4),
        Span::new(date(2026, 10, 26), 4),
        Span::new(date(2026, 11, 23), 4),
    ];
    let aims = aims(autumn, &mesocycles, date(2026, 11, 2));
    assert_eq!(
        aims.iter()
            .map(|aim| (aim.span.start(), aim.phase, aim.per_week))
            .collect::<Vec<_>>(),
        vec![
            (date(2026, 9, 14), NutritionPhase::Cut, Some(-0.25)),
            (date(2026, 9, 28), NutritionPhase::Cut, Some(-0.25)),
            (date(2026, 10, 26), NutritionPhase::Cut, Some(-0.25)),
        ],
        "a mesocycle not yet started has no aim"
    );
}

/// **The cut runs to the end of term** (operator, 2026-11-11): the
/// competitive mesocycle keeps the cut's aim, and the transition aims at no
/// change.
#[test]
fn the_cut_s_aim_runs_to_the_transition() {
    let autumn = autumn().expect("the macrocycle");
    let mesocycles = [
        Span::new(date(2026, 10, 26), 4),
        Span::new(date(2026, 11, 23), 4),
        Span::new(date(2026, 12, 21), 2),
    ];
    let aims = aims(autumn, &mesocycles, date(2027, 1, 1));
    assert_eq!(
        aims.iter()
            .map(|aim| (aim.span.start(), aim.phase, aim.per_week))
            .collect::<Vec<_>>(),
        vec![
            (date(2026, 10, 26), NutritionPhase::Cut, Some(-0.25)),
            (date(2026, 11, 23), NutritionPhase::Cut, Some(-0.25)),
            (date(2026, 12, 21), NutritionPhase::Maintenance, Some(0.0)),
        ]
    );
}

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
fn a_range_keeps_the_readings_of_its_own_days() {
    let series = Series::of(daily(date(2026, 9, 1), 60, 84_000, &[]).expect("a series"));
    let within = series.within(Range {
        from: date(2026, 9, 15),
        to: date(2026, 9, 21),
    });
    assert_eq!(within.weigh_ins.len(), 7);
    assert_eq!(
        within.trend.iter().flatten().count(),
        7,
        "each day's trend is the series', not the range's"
    );
    assert_eq!(
        within
            .summary()
            .map(|summary| (summary.average, summary.difference)),
        Some((Kg::from_grams(84_000), Some(0)))
    );
}

/// The autumn's plan starts on Monday 2026-09-14, a week into the term (#443).
#[test]
fn the_macrocycle_runs_from_the_plan_not_the_term() {
    let autumn = Span::new(date(2026, 9, 14), 15);
    let ranges = Ranges::on(date(2026, 10, 16), &[], &[autumn]);
    assert_eq!(
        ranges.macrocycle,
        Some(Range {
            from: date(2026, 9, 14),
            to: date(2026, 10, 16),
        })
    );
}

#[test]
fn a_day_no_plan_covers_has_no_macrocycle() {
    let autumn = Span::new(date(2026, 9, 14), 15);
    assert_eq!(
        Ranges::on(date(2027, 1, 11), &[], &[autumn]).macrocycle,
        None
    );
    assert_eq!(
        Ranges::on(date(2026, 9, 10), &[], &[autumn]).macrocycle,
        None
    );
}

/// The mesocycle that began on Monday 2026-09-28, inside the autumn's plan.
#[test]
fn a_range_is_the_cycle_named_or_everything_recorded() {
    let autumn = Span::new(date(2026, 9, 14), 15);
    let october = Span::new(date(2026, 9, 28), 4);
    let ranges = Ranges::on(date(2026, 10, 19), &[october], &[autumn]);
    let recorded = Range::spanning([date(2026, 10, 1), date(2016, 3, 2), date(2026, 10, 18)]);
    assert_eq!(
        ranges.over(Over::Mesocycle, recorded),
        Some(Range {
            from: date(2026, 9, 28),
            to: date(2026, 10, 19),
        })
    );
    assert_eq!(
        ranges.over(Over::All, recorded),
        Some(Range {
            from: date(2016, 3, 2),
            to: date(2026, 10, 18),
        })
    );
    assert_eq!(ranges.over(Over::All, Range::spanning([])), None);
}
