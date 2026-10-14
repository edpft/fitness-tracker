//! Body weight as one time series (#250).

use domain::{
    analytical::{
        Weighed,
        body_weight::{Trend, body_weight, trend},
    },
    canonical::Occurred,
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
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
    })
}

const fn dated(on: Date, grams: u64) -> Weighed {
    Weighed {
        at: Occurred::On(on),
        mass: Kg::from_grams(grams),
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
