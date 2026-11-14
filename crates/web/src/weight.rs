//! Body weight as one time series (#250), on the body page (#786).
//!
//! **Date across, mass up, one dot per weigh-in.** Nothing joins two dots: a
//! line between two weigh-ins would claim something about the days between
//! them, and a gap in the record is drawn as the empty stretch it is (§ 37).
//!
//! **The trend is the line** (#252): an estimate of a continuous mass, drawn
//! as one line per unbroken run `domain` gives, so where it has no trend the
//! line breaks.
//!
//! **Against where the plan's aims would have taken it** (#786): the trend
//! against expectation, as Progress's body tile draws it. The weight change
//! chart (#689) went with it (operator, 2026-11-13).
//!
//! **The summary is the latest trend weight**, whatever the range: the range
//! only chooses which days the chart shows.

use std::collections::BTreeMap;

use domain::{
    analytical::{
        body_weight::{Aim, Series, Trend, against_aims},
        range::Range,
    },
    canonical::Occurred,
    measure::Kg,
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Traced};

/// The days a weigh-in was recorded on.
pub fn recorded(series: &Series) -> Option<Range> {
    Range::spanning(series.weigh_ins.iter().map(|weighed| weighed.at.day()))
}

/// Body weight over `range`, against where the plan's aims would have taken
/// the trend since each mesocycle of the macrocycle that `began` on a day
/// started (#786). Nothing where
/// no weigh-in was recorded, since the page may still have composition to
/// show.
///
/// **The expectation is Progress's body tile's** (`against_aims`), so the
/// tile and the page draw one line, not two versions of it (operator,
/// 2026-11-13).
pub fn charts(series: &Series, aims: &[Aim], began: Option<Date>, range: Range) -> Markup {
    if series.weigh_ins.is_empty() {
        return html! {};
    }
    let today = range.to;
    let within = series.within(range);
    let expected = detail::expected(
        &began
            .and_then(|began| against_aims(&series.trend, aims, began, today))
            .map(|weight| weight.aimed)
            .unwrap_or_default(),
        range,
    );
    html! {
        (detail::chart(&Chart {
            id: "weight",
            metric: Metric::BodyWeight,
            title: "Body weight",
            sub: weigh_ins(within.weigh_ins.len()),
            range,
            axis: Axis::Figures,
            layers: vec![
                Layer::Spans(detail::mesocycles(&aims.iter().map(|aim| aim.span).collect::<Vec<_>>(), range)),
                Layer::Trend(within.trend.iter().map(|run| run.iter().map(traced).collect()).collect()),
                Layer::Dots(within.weigh_ins.iter().map(|weighed| (weighed.at.day(), kilograms(weighed.mass))).collect()),
                Layer::Aim(expected.clone()),
            ],
            days: days(&within, &expected.iter().flatten().copied().collect(), today),
        }))
    }
}

/// The latest seven weigh-ins, each its mass and, where it gave one, its fat
/// and fat-free mass.
///
/// **Fat is the weigh-in's mass less its fat-free mass**, which is how the
/// weigh-in's fat-free mass was had where it stated its fat.
pub fn latest(series: &Series, today: Date) -> Markup {
    let rows: Vec<(String, Markup)> = series
        .weigh_ins
        .iter()
        .rev()
        .take(7)
        .map(|weighed| {
            let fat = weighed
                .fat_free
                .and_then(|fat_free| weighed.mass.as_grams().checked_sub(fat_free.as_grams()))
                .map(Kg::from_grams);
            let split = |mass: Option<Kg>| mass.map_or_else(|| "–".to_owned(), tenths);
            (
                when(&weighed.at, today),
                html! {
                    (tenths(weighed.mass))
                    " · " span.fat { (split(fat)) }
                    " · " span.fat-free { (split(weighed.fat_free)) }
                },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Weigh-ins", "weight · fat · fat-free, kg", &rows)
}

/// The latest trend weight: what the underlying weight is taken to be now,
/// and the day it is the trend on.
pub fn cell(series: &Series) -> Option<(Cell<'static>, Date)> {
    let latest = series.trend.iter().flatten().next_back()?;
    Some((
        Cell {
            metric: Metric::BodyWeight,
            label: "Trend weight",
            figure: tenths(latest.mass),
            unit: "kg",
            margin: latest.margin.map(|margin| format!("{margin:.1} kg")),
            readings: Vec::new(),
        },
        latest.on,
    ))
}

/// What a tap on each day shows: its weigh-ins, its trend and where the aims
/// would have taken it.
fn days(series: &Series, expected: &BTreeMap<Date, f64>, today: Date) -> Vec<(Date, Markup)> {
    let mut weighed: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    for one in &series.weigh_ins {
        weighed
            .entry(one.at.day())
            .or_default()
            .push(format!("{} kg", tenths(one.mass)));
    }
    let trend: BTreeMap<Date, Trend> = series
        .trend
        .iter()
        .flatten()
        .map(|day| (day.on, *day))
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
                @if let Some(expected) = expected.get(&on) {
                    " · expected " (format!("{expected:.1}")) " kg"
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
        margin: day.margin,
    }
}

fn weigh_ins(count: usize) -> String {
    if count == 1 {
        "1 weigh-in".to_owned()
    } else {
        format!("{count} weigh-ins")
    }
}

/// A mass to the nearest 0.1 kg (operator, 2026-10-31, on the weigh-ins).
fn tenths(mass: Kg) -> String {
    let tenths = (mass.as_grams() + 50) / 100;
    format!("{}.{}", tenths / 10, tenths % 10)
}

/// A mass in kilograms, to the gram.
fn kilograms(mass: Kg) -> f64 {
    f64::from(u32::try_from(mass.as_grams()).unwrap_or(u32::MAX)) / 1000.0
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
            body_weight::{Aim, Series},
            nutrition_phase::NutritionPhase,
            range::{Over, Range, Ranges},
        },
        canonical::Occurred,
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
        plan::Span,
    };
    use jiff::civil::{Date, date};
    use maud::Markup;

    /// The body page with no composition recorded.
    fn page(
        series: &Series,
        aims: &[Aim],
        ranges: &Ranges,
        over: Over,
        run: &crate::run::View,
    ) -> Markup {
        crate::body::page(series, aims, &[], ranges, over, run)
    }

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
            &[],
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
            page.contains(r#"<span class="on">10 Sep</span> · <b>79.0 kg</b>"#),
            "{page}"
        );
        assert!(
            page.contains(r#"<span class="on">1 Oct 07:12</span><span class="value">84.1 · <span class="fat">–</span> · <span class="fat-free">–</span></span>"#),
            "{page}"
        );
    }

    /// **A weigh-in is one row**: its mass, its fat and its fat-free mass.
    #[test]
    fn a_weigh_in_lists_its_fat_and_fat_free_mass() {
        let weigh_ins = vec![Weighed {
            at: Occurred::On(date(2026, 11, 12)),
            mass: Kg::from_grams(83_648),
            fat_free: Some(Kg::from_grams(66_038)),
        }];
        let page = shown(weigh_ins, &ranges(date(2026, 11, 12)), Over::All);
        assert!(
            page.contains(r#"<span class="value">83.6 · <span class="fat">17.6</span> · <span class="fat-free">66.0</span></span>"#),
            "{page}"
        );
        assert!(page.contains("weight · fat · fat-free, kg"), "{page}");
    }

    #[test]
    fn no_weigh_ins_is_said_rather_than_drawn() {
        let page = shown(Vec::new(), &ranges(date(2026, 9, 23)), Over::All);
        assert!(page.contains("Nothing has been recorded."), "{page}");
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
    fn the_summary_is_trend_weight_and_weight_change_is_not_charted() {
        let weigh_ins = (1..=28).map(|d| day(d, 84_000)).collect();
        let page = shown(weigh_ins, &ranges(date(2026, 9, 28)), Over::All);
        assert!(page.contains("Trend weight"), "{page}");
        assert!(!page.contains(r#"class="badge""#), "no arrow: {page}");
        assert!(
            !page.contains("Weight change"),
            "weight change went (2026-11-13): {page}"
        );
    }

    /// **The trend against where the aims would have taken it**, from the
    /// trend on each mesocycle's first day, as Progress's tile draws it.
    #[test]
    fn the_trend_is_drawn_against_the_aims() {
        let mut weigh_ins: Vec<Weighed> = (1..=30).map(|d| day(d, 84_000)).collect();
        weigh_ins.extend((1..=31).map(|d| Weighed {
            at: Occurred::On(date(2026, 10, d)),
            mass: Kg::from_grams(84_000),
            fat_free: None,
        }));
        let aims = [Aim {
            span: Span::new(date(2026, 9, 28), 4),
            phase: NutritionPhase::Cut,
            per_week: Some(-0.25),
        }];
        let page = page(
            &Series::of(weigh_ins),
            &aims,
            &ranges(date(2026, 10, 31)),
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();
        assert_eq!(
            page.matches(r#"class="aim""#).count(),
            2,
            "reset where the cut starts: {page}"
        );
        assert!(
            page.contains("31 Oct</span> · <b>84.0 kg</b> · trend 84.0 kg · expected 83.0 kg"),
            "four weeks at −0.25 kg from 28 Sep's trend: {page}"
        );
        assert!(
            !page.contains(
                r#"<span class="on">6 Sep</span> · <b>84.0 kg</b> · trend 84.0 kg · expected"#
            ),
            "nothing is expected before the macrocycle: {page}"
        );
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
