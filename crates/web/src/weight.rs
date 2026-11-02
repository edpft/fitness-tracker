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
//! **Weight change is a second chart** (#689): how fast the trend moved, in kg
//! a week, filled down to zero and against the plan's aim. No word says what it means; the
//! line against them shows it (operator, 2026-11-02, #688).
//!
//! **The summary is the latest trend weight**, whatever the range: the range
//! only chooses which days the charts show. Weight change is not in it: its
//! latest day is 17 days back, and as a figure it read as this week's change
//! (operator, 2026-11-02).

use std::collections::BTreeMap;

use domain::{
    analytical::{
        body_weight::{Aim, CHANGE_METHOD, METHOD, Series, Trend, WeightChange},
        confidence::BAND,
        range::{Over, Range, Ranges},
    },
    canonical::Occurred,
    measure::Kg,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Traced};

const TITLE: &str = "Body weight";
const HERE: &str = "/body-weight";

/// The whole page.
pub fn page(
    series: &Series,
    aims: &[Aim],
    ranges: &Ranges,
    over: Over,
    run: &crate::run::View,
) -> Markup {
    detail::page(HERE, TITLE, over, run, &body(series, aims, ranges, over))
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

/// The summary, the two charts over the range chosen, and the latest
/// weigh-ins, or why there is no range.
fn body(series: &Series, aims: &[Aim], ranges: &Ranges, over: Over) -> Markup {
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
            axis: Axis::Figures,
            layers: vec![
                Layer::Trend(within.trend.iter().map(|run| run.iter().map(traced).collect()).collect()),
                Layer::Dots(within.weigh_ins.iter().map(|weighed| (weighed.at.day(), kilograms(weighed.mass))).collect()),
            ],
            days: days(&within, today),
        }))
        (detail::chart(&Chart {
            id: "weight-change",
            metric: Metric::BodyWeight,
            title: "Weight change",
            sub: "kg a week".to_owned(),
            range,
            axis: Axis::Figures,
            layers: vec![
                Layer::Filled(within.weight_change.iter().map(|run| run.iter().map(changed).collect()).collect()),
                Layer::Aim(aimed(aims, range)),
                Layer::Zero,
            ],
            days: change_days(&within, today),
        }))
        (detail::latest(
            "Weigh-ins",
            "kg",
            &series
                .weigh_ins
                .iter()
                .rev()
                .take(7)
                .map(|weighed| (when(&weighed.at, today), html! { (tenths(weighed.mass)) }))
                .collect::<Vec<_>>(),
        ))
        (detail::foot(&format!(
            "Dots: each weigh-in. Line: the trend, the {METHOD}. Weight change: {CHANGE_METHOD}; dashed, the plan's aim. Band: {BAND}."
        )))
    }
}

/// The latest trend weight: what the underlying weight is taken to be now.
fn summary(series: &Series, today: Date) -> Markup {
    let Some(latest) = series.trend.iter().flatten().next_back() else {
        return html! {};
    };
    detail::summary(
        &[Cell {
            metric: Metric::BodyWeight,
            label: "Trend weight",
            figure: tenths(latest.mass),
            unit: "kg",
            margin: latest.margin.map(|margin| format!("{margin:.1} kg")),
            readings: Vec::new(),
        }],
        &detail::day(latest.on, today),
    )
}

/// What a tap on each day shows: its weigh-ins and its trend.
fn days(series: &Series, today: Date) -> Vec<(Date, Markup)> {
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
            };
            (on, said)
        })
        .collect()
}

/// What a tap on each day of weight change shows.
fn change_days(series: &Series, today: Date) -> Vec<(Date, Markup)> {
    series
        .weight_change
        .iter()
        .flatten()
        .map(|change| {
            let said = html! {
                span.on { (detail::day(change.on, today)) }
                " · " b { (signed(change.per_week)) " kg a week" }
                @if let Some(margin) = change.margin {
                    " ± " (format!("{margin:.2}"))
                }
            };
            (change.on, said)
        })
        .collect()
}

/// Each aim the plans state, as a line across the days of its mesocycle that
/// fall in `range`.
fn aimed(aims: &[Aim], range: Range) -> Vec<Vec<(Date, f64)>> {
    aims.iter()
        .filter_map(|aim| {
            let per_week = aim.per_week?;
            let last = aim.span.end().yesterday().ok()?;
            let from = aim.span.start().max(range.from);
            let to = last.min(range.to);
            (from <= to).then(|| vec![(from, per_week), (to, per_week)])
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

const fn changed(day: &WeightChange) -> Traced {
    Traced {
        on: day.on,
        value: day.per_week,
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

/// Kilograms a week to the nearest 0.01, with its sign.
fn signed(per_week: f64) -> String {
    let hundredths = (per_week * 100.0).round();
    let sign = if hundredths < 0.0 {
        "−"
    } else if hundredths > 0.0 {
        "+"
    } else {
        ""
    };
    format!("{sign}{:.2}", hundredths.abs() / 100.0)
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
            range::{Over, Range, Ranges},
            recomposition::NutritionPhase,
        },
        canonical::Occurred,
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
        plan::Span,
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
    fn the_summary_is_trend_weight_and_weight_change_is_charted() {
        // 40 g a day for 60 days from 2026-08-01: 0.28 kg a week. The last
        // weigh-in is 09-29, so the last weight change is 17 days before it.
        let weigh_ins = (0..60)
            .map(|offset| {
                let on = date(2026, 8, 1)
                    .checked_add(jiff::Span::new().days(offset))
                    .expect("a day");
                Weighed {
                    at: Occurred::On(on),
                    mass: Kg::from_grams(86_000 - 40 * u64::try_from(offset).expect("a count")),
                    fat_free: None,
                }
            })
            .collect();
        let page = shown(weigh_ins, &ranges(date(2026, 9, 29)), Over::All);
        assert!(page.contains("Trend weight"), "{page}");
        assert!(
            !page.contains("<small>kg a week</small>"),
            "weight change is not a figure in the summary: {page}"
        );
        assert!(!page.contains(r#"class="badge""#), "no arrow: {page}");
        assert!(
            page.contains("12 Sep</span> · <b>−0.28 kg a week</b>"),
            "{page}"
        );
    }

    #[test]
    fn weight_change_is_drawn_against_zero_and_the_aim() {
        let weigh_ins = (1..=30).map(|d| day(d, 84_000)).collect::<Vec<_>>();
        let mut more: Vec<Weighed> = (1..=31)
            .map(|d| Weighed {
                at: Occurred::On(date(2026, 10, d)),
                mass: Kg::from_grams(84_000),
                fat_free: None,
            })
            .collect();
        more.extend(weigh_ins);
        let aims = [Aim {
            span: Span::new(date(2026, 9, 28), 4),
            phase: NutritionPhase::Cut,
            per_week: Some(-0.25),
        }];
        let page = page(
            &Series::of(more),
            &aims,
            &ranges(date(2026, 10, 31)),
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(page.contains("<h2>Weight change</h2>"), "{page}");
        assert_eq!(page.matches(r#"class="zero""#).count(), 1, "{page}");
        assert_eq!(page.matches(r#"class="aim""#).count(), 1, "{page}");
        assert_eq!(
            page.matches(r#"class="below""#).count(),
            1,
            "filled down to zero, not a trend's band: {page}"
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
