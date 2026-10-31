//! Body weight as one time series (#250), on the detail template (#614).
//!
//! **Date across, mass up, one dot per weigh-in.** Nothing joins two dots: a
//! line between two weigh-ins would claim something about the days between
//! them, and a gap in the record is drawn as the empty stretch it is (§ 37).
//!
//! **The trend is the line** (#252): an estimate of a continuous mass, drawn
//! as one line per unbroken run `domain` gives, so where it has no trend the
//! line breaks.
//!
//! **The summary is the latest trend and its change over 7 days**, against the
//! 0.125 kg within which `domain` reads it as maintaining. It is the latest
//! whatever the range: the range only chooses which days the chart shows.

use std::collections::BTreeMap;

use domain::{
    analytical::{
        body_weight::{Change, MAINTAINING_GRAMS, METHOD, Series, Trend},
        range::{Over, Range, Ranges},
    },
    canonical::Occurred,
    measure::Kg,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::detail::{self, Cell, Chart, Layer, Metric, Reading, Traced};

const TITLE: &str = "Body weight";
const HERE: &str = "/body-weight";

/// The whole page.
pub fn page(series: &Series, ranges: &Ranges, over: Over, run: &crate::run::View) -> Markup {
    detail::page(HERE, TITLE, over, run, &body(series, ranges, over))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::menu(HERE))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// The summary, the chart over the range chosen, and the latest weigh-ins, or
/// why there is no range.
fn body(series: &Series, ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(series.weigh_ins.iter().map(|weighed| weighed.at.day()));
    let range = match crate::range::chosen(ranges, over, recorded, "No weigh-in has been recorded.")
    {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let within = series.within(range);
    html! {
        (summary(series, today))
        (detail::chart(&Chart {
            id: "weight",
            metric: Metric::BodyWeight,
            title: TITLE,
            sub: weigh_ins(within.weigh_ins.len()),
            range,
            layers: vec![
                Layer::Trend(within.trend.iter().map(|run| run.iter().map(traced).collect()).collect()),
                Layer::Dots(within.weigh_ins.iter().map(|weighed| (weighed.at.day(), kilograms(weighed.mass))).collect()),
            ],
            days: days(&within, today),
        }))
        (detail::latest(
            "Weigh-ins",
            "kg",
            &series
                .weigh_ins
                .iter()
                .rev()
                .take(7)
                .map(|weighed| (when(&weighed.at, today), weighed.mass.to_string()))
                .collect::<Vec<_>>(),
        ))
        (detail::foot(&format!("Dots: each weigh-in. Line: the trend, the {METHOD}.")))
    }
}

/// The latest trend, and its change over the 7 days to it.
fn summary(series: &Series, today: Date) -> Markup {
    let Some(latest) = series.trend.iter().flatten().next_back() else {
        return html! {};
    };
    let reading = series
        .changes
        .last()
        .filter(|change| change.on == latest.on)
        .map_or_else(
            || Reading::NoRule("No trend 7 days before to read it against.".to_owned()),
            |change| Reading::Change {
                by: grams_as_kilograms(change.grams),
                noise: grams_as_kilograms(MAINTAINING_GRAMS),
                direction: change.direction,
                said: format!("{} kg", signed(change.grams)),
                over: "in 7 days".to_owned(),
            },
        );
    detail::summary(
        &[Cell {
            metric: Metric::BodyWeight,
            label: "Trend",
            figure: tenths(latest.mass),
            unit: "kg",
            margin: None,
            reading,
        }],
        &detail::day(latest.on, today),
    )
}

/// What a tap on each day shows: its weigh-ins, its trend, and the trend's
/// change over the 7 days to it.
fn days(series: &Series, today: Date) -> Vec<(Date, Markup)> {
    let mut weighed: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    for one in &series.weigh_ins {
        weighed
            .entry(one.at.day())
            .or_default()
            .push(format!("{} kg", one.mass));
    }
    let trend: BTreeMap<Date, Trend> = series
        .trend
        .iter()
        .flatten()
        .map(|day| (day.on, *day))
        .collect();
    let changes: BTreeMap<Date, Change> = series
        .changes
        .iter()
        .map(|change| (change.on, *change))
        .collect();
    let mut every: Vec<Date> = weighed.keys().chain(trend.keys()).copied().collect();
    every.sort_unstable();
    every.dedup();
    every
        .into_iter()
        .map(|on| {
            let said = html! {
                span.on { (detail::day(on, today)) }
                @if let Some(masses) = weighed.get(&on) {
                    " · " b { (masses.join(", ")) }
                }
                @if let Some(day) = trend.get(&on) {
                    " · trend " (tenths(day.mass)) " kg"
                }
                @if let Some(change) = changes.get(&on) {
                    " · " (signed(change.grams)) " kg in 7 days"
                }
            };
            (on, said)
        })
        .collect()
}

fn traced(day: &Trend) -> Traced {
    Traced {
        on: day.on,
        value: kilograms(day.mass),
        margin: None,
    }
}

fn weigh_ins(count: usize) -> String {
    if count == 1 {
        "1 weigh-in".to_owned()
    } else {
        format!("{count} weigh-ins")
    }
}

/// A change in grams to the nearest 0.01 kg, with its sign.
///
/// **Hundredths, not the trend's tenths**: the line between maintaining and
/// not is 0.125 kg, and −0.14 kg shown as −0.1 would read as maintaining.
fn signed(grams: i64) -> String {
    let hundredths = (grams.abs() + 5) / 10;
    let sign = if grams < 0 && hundredths > 0 {
        "−"
    } else if grams > 0 && hundredths > 0 {
        "+"
    } else {
        ""
    };
    format!("{sign}{}.{:02}", hundredths / 100, hundredths % 100)
}

/// A trend to the nearest 0.1 kg: it is a mean, and its grams are arithmetic.
fn tenths(mass: Kg) -> String {
    let tenths = (mass.as_grams() + 50) / 100;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// A mass in kilograms, to the gram.
fn kilograms(mass: Kg) -> f64 {
    f64::from(u32::try_from(mass.as_grams()).unwrap_or(u32::MAX)) / 1000.0
}

fn grams_as_kilograms(grams: i64) -> f64 {
    f64::from(i32::try_from(grams).unwrap_or(0)) / 1000.0
}

/// The day, and the time where a clock was read.
fn when(at: &Occurred, today: Date) -> String {
    let day = detail::day(at.day(), today);
    at.instant().map_or_else(
        || day.clone(),
        |started| format!("{day} {}", started.wall_clock().strftime("%H:%M")),
    )
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            Weighed,
            body_weight::Series,
            range::{Over, Range, Ranges},
        },
        canonical::Occurred,
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::civil::{Date, date};

    use super::page;

    fn day(d: i8, grams: u64) -> Weighed {
        Weighed {
            at: Occurred::On(date(2026, 9, d)),
            mass: Kg::from_grams(grams),
            fat_free: None,
        }
    }

    /// The ranges on `today`, in a macrocycle from Monday 2026-09-07 and a
    /// mesocycle from Monday 2026-09-14.
    fn ranges(today: Date) -> Ranges {
        let range = |from| Range { from, to: today };
        Ranges {
            mesocycle: Some(range(date(2026, 9, 14))),
            macrocycle: Some(range(date(2026, 9, 7))),
        }
    }

    fn shown(weigh_ins: Vec<Weighed>, ranges: &Ranges, over: Over) -> String {
        page(
            &Series::of(weigh_ins),
            ranges,
            over,
            &crate::run::View::default(),
        )
        .into_string()
    }

    #[test]
    fn a_weigh_in_is_a_dot_and_nothing_joins_two() {
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = date(2026, 10, 1)
            .at(7, 12, 0, 0)
            .to_zoned(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        let weigh_ins = vec![
            day(10, 79_000),
            Weighed {
                at: Occurred::At(StartedAt::new(at, zone)),
                mass: Kg::from_grams(84_100),
                fat_free: None,
            },
        ];
        let page = shown(weigh_ins, &ranges(date(2026, 10, 1)), Over::All);
        assert_eq!(page.matches(r#"class="dot""#).count(), 2, "{page}");
        assert!(!page.contains("<polyline"), "{page}");
        assert!(
            page.contains(r#"<span class="on">10 Sep</span> · <b>79 kg</b>"#),
            "{page}"
        );
        assert!(
            page.contains(r#"<span class="on">1 Oct 07:12</span><span class="value">84.1</span>"#),
            "{page}"
        );
    }

    #[test]
    fn no_weigh_ins_is_said_rather_than_drawn() {
        let page = shown(Vec::new(), &ranges(date(2026, 9, 23)), Over::All);
        assert!(page.contains("No weigh-in has been recorded."), "{page}");
        assert!(!page.contains(r#"class="plotted""#), "{page}");
    }

    #[test]
    fn the_trend_is_one_line_per_unbroken_run() {
        // Weigh-ins on the 1st to 7th and the 22nd to 28th, none between.
        let weigh_ins = (1..=7).chain(22..=28).map(|d| day(d, 84_043)).collect();
        let page = shown(weigh_ins, &ranges(date(2026, 9, 28)), Over::All);
        assert_eq!(page.matches(r#"class="line""#).count(), 2, "{page}");
        assert_eq!(page.matches(r#"class="dot""#).count(), 14, "{page}");
        assert!(
            page.contains("· trend 84.0 kg"),
            "to the nearest 0.1 kg: {page}"
        );
    }

    #[test]
    fn the_summary_is_the_latest_trend_and_its_change_against_the_noise() {
        // A week at 84 kg from Monday 2026-09-07, then a week at 83.75 kg.
        let weigh_ins = (7..=13)
            .map(|d| day(d, 84_000))
            .chain((14..=20).map(|d| day(d, 83_750)))
            .collect();
        let page = shown(weigh_ins, &ranges(date(2026, 9, 20)), Over::Macrocycle);
        assert!(
            page.contains(r#"<div class="figure">83.8<small>kg</small></div>"#),
            "{page}"
        );
        assert!(
            page.contains("<b>−0.25 kg</b><span>in 7 days</span>"),
            "{page}"
        );
        assert!(
            page.contains(r#"class="badge""#),
            "falling is an arrow: {page}"
        );
        assert!(page.contains(r#"class="noise""#), "{page}");
        assert!(
            page.contains("· trend 83.8 kg · −0.25 kg in 7 days"),
            "{page}"
        );
        assert!(!page.contains("falling"), "no verdict words: {page}");
    }

    #[test]
    fn a_range_runs_from_the_start_of_its_cycle_to_today() {
        // Daily from 2026-09-01; this mesocycle began on Monday the 14th.
        let weigh_ins = (1..=23).map(|d| day(d, 84_000)).collect();
        let page = shown(weigh_ins, &ranges(date(2026, 9, 23)), Over::Mesocycle);
        assert_eq!(page.matches(r#"class="dot""#).count(), 10, "{page}");
        assert!(page.contains("10 weigh-ins"), "{page}");
        assert!(page.contains("<strong>this mesocycle</strong>"), "{page}");
    }

    #[test]
    fn a_day_with_no_plan_has_no_macrocycle() {
        let mut ranges = ranges(date(2026, 9, 23));
        ranges.macrocycle = None;
        let page = shown(vec![day(1, 84_000)], &ranges, Over::Macrocycle);
        assert!(
            page.contains("No macrocycle is planned for today."),
            "{page}"
        );
        assert!(
            page.contains("this mesocycle"),
            "the other ranges are still offered"
        );
    }
}
