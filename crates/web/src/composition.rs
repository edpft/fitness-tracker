//! Body composition, on the body page (#619, #786).
//!
//! **Fat and fat-free mass side by side**: each one's latest trend and its
//! change over [`AGAINST_DAYS`] days against the method's own noise, as #419
//! reads it. It is the latest whatever the range: the range only chooses
//! which days the charts show.
//!
//! **A chart per component, its colour the component's** (operator,
//! 2026-10-28, on the mocks). Fat is one chart with its unit a choice, kilograms
//! first (operator, 2026-10-20). Nothing joins two points, so a gap in the
//! record is the empty stretch it is (§ 37), and nothing joins two methods,
//! since each one's figures are its own series (§ 6): each has its own trend,
//! and the spreadsheets' points are hollow where the Body Scan's are solid.
//!
//! **Each in kilograms against what the aims expect** (#786): fat falling as
//! body weight's aim does and fat-free mass holding, reset at each
//! mesocycle's start. It replaced the mesocycle matrix (operator, 2026-11-13).

use std::collections::BTreeMap;

use domain::{
    analytical::{
        body_weight::Aim,
        composition::{AGAINST_DAYS, Change, ComponentSeries, Method, MethodSeries, Unit},
        range::Range,
    },
    body::Component,
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced};

/// The component in the unit, where anything measured it.
fn charted(
    components: &[ComponentSeries],
    component: Component,
    unit: Unit,
) -> Option<&ComponentSeries> {
    components
        .iter()
        .find(|one| one.component == component && one.unit == unit)
}

/// The days anything measured a component.
pub fn recorded(components: &[ComponentSeries]) -> Option<Range> {
    Range::spanning(
        components
            .iter()
            .flat_map(|component| &component.series)
            .flat_map(|one| one.readings.iter().map(|reading| reading.on)),
    )
}

/// Fat and fat-free mass over `range`, each in kilograms against where the
/// aims would have taken it since each mesocycle of the macrocycle that
/// `began` on a day started.
pub fn charts(
    components: &[ComponentSeries],
    aims: &[Aim],
    began: Option<Date>,
    range: Range,
) -> Markup {
    let today = range.to;
    let shaded = detail::mesocycles(&aims.iter().map(|aim| aim.span).collect::<Vec<_>>(), range);
    let expected = |one: &ComponentSeries| -> Vec<Vec<(Date, f64)>> {
        let Some(began) = began.filter(|_| one.unit == Unit::Kilograms) else {
            return Vec::new();
        };
        one.series
            .iter()
            .flat_map(|series| {
                detail::expected(
                    &series.against_aims(one.component, aims, began, today),
                    range,
                )
            })
            .collect()
    };
    let fat: Vec<(&str, Chart<'_>)> = [Unit::Kilograms, Unit::Percentage]
        .into_iter()
        .filter_map(|unit| charted(components, Component::Fat, unit))
        .map(|one| {
            (
                one.unit.as_str(),
                chart(
                    &one.within(range),
                    Metric::Fat,
                    range,
                    today,
                    expected(one),
                    &shaded,
                ),
            )
        })
        .collect();
    let fat_free = charted(components, Component::FatFree, Unit::Kilograms).map(|one| {
        chart(
            &one.within(range),
            Metric::FatFree,
            range,
            today,
            expected(one),
            &shaded,
        )
    });
    html! {
        @match fat.as_slice() {
            [] => {}
            [(_, one)] => (detail::chart(one)),
            _ => (detail::choice("fat-unit", Metric::Fat, heading(Component::Fat), &fat)),
        }
        @if let Some(one) = &fat_free {
            (detail::chart(one))
        }
    }
}

/// Whether the spreadsheets measured anything in `range`, so that some
/// points are hollow.
pub fn hollow(components: &[ComponentSeries], range: Range) -> bool {
    components
        .iter()
        .map(|component| component.within(range))
        .any(|component| {
            component
                .series
                .iter()
                .any(|one| one.method == Method::Spreadsheets && !one.readings.is_empty())
        })
}

/// What a component is headed.
const fn heading(component: Component) -> &'static str {
    match component {
        Component::Fat => "Fat",
        Component::FatFree => "Fat-free mass",
        Component::Muscle => "Muscle mass",
        Component::SkeletalMuscle => "Skeletal muscle mass",
        Component::Lean => "Lean mass",
        Component::Water => "Water",
        Component::ExtracellularWater => "Extracellular water",
        Component::IntracellularWater => "Intracellular water",
        Component::Bone => "Bone mass",
        Component::Protein => "Protein",
        Component::Minerals => "Minerals",
    }
}

/// The method whose trend is the latest, and that trend.
fn latest_trend(
    component: &ComponentSeries,
) -> Option<(&MethodSeries, domain::analytical::composition::Trend)> {
    component
        .series
        .iter()
        .filter_map(|one| {
            one.trend
                .iter()
                .flatten()
                .next_back()
                .map(|day| (one, *day))
        })
        .max_by_key(|(_, day)| day.on)
}

/// Fat and fat-free mass, each its latest trend and its change over
/// [`AGAINST_DAYS`] days against the method's noise, and the day and method
/// each trend is.
pub fn summary(components: &[ComponentSeries], today: Date) -> (Vec<Cell<'static>>, Vec<String>) {
    let read: Vec<(
        Metric,
        &'static str,
        &MethodSeries,
        domain::analytical::composition::Trend,
    )> = [
        (Component::Fat, Metric::Fat, "Fat"),
        (Component::FatFree, Metric::FatFree, "Fat-free"),
    ]
    .into_iter()
    .filter_map(|(component, metric, label)| {
        let (one, latest) = latest_trend(charted(components, component, Unit::Kilograms)?)?;
        Some((metric, label, one, latest))
    })
    .collect();
    let cells: Vec<Cell<'static>> = read
        .iter()
        .map(|(metric, label, one, latest)| Cell {
            metric: *metric,
            label,
            figure: format!("{:.1}", latest.value),
            unit: "kg",
            margin: latest.margin.map(|margin| format!("{margin:.1} kg")),
            readings: vec![
                one.changes
                    .last()
                    .filter(|change| change.on == latest.on)
                    .map_or_else(
                        || {
                            Reading::NoRule(format!(
                                "No trend {AGAINST_DAYS} days before to read it against."
                            ))
                        },
                        |change| Reading::Change {
                            by: change.by,
                            noise: change.noise,
                            direction: None,
                            said: format!("{} kg", signed(change.by)),
                            over: format!("in {AGAINST_DAYS} days"),
                        },
                    ),
            ],
        })
        .collect();
    let mut asof: Vec<String> = read
        .iter()
        .map(|(_, _, one, latest)| {
            format!(
                "Trend on {} · {}",
                detail::day(latest.on, today),
                one.method
            )
        })
        .collect();
    asof.dedup();
    (cells, asof)
}

/// One component in one unit over `range`: each method's trend, its points,
/// solid or hollow, and where the aims expected it.
fn chart(
    component: &ComponentSeries,
    metric: Metric,
    range: Range,
    today: Date,
    expected: Vec<Vec<(Date, f64)>>,
    mesocycles: &[(Date, Date, String)],
) -> Chart<'static> {
    let aimed: BTreeMap<Date, f64> = expected.iter().flatten().copied().collect();
    let points = |hollow: bool| -> Vec<(Date, f64)> {
        component
            .series
            .iter()
            .filter(|one| (one.method == Method::Spreadsheets) == hollow)
            .flat_map(|one| {
                one.readings
                    .iter()
                    .map(|reading| (reading.on, reading.value))
            })
            .collect()
    };
    let count = component.series.iter().map(|one| one.readings.len()).sum();
    Chart {
        id: match (component.component, component.unit) {
            (Component::Fat, Unit::Percentage) => "fat-percentage",
            (Component::Fat, Unit::Kilograms) => "fat-kilograms",
            _ => "fat-free",
        },
        metric,
        title: heading(component.component),
        sub: weigh_ins(count),
        range,
        axis: Axis::Figures,
        layers: vec![
            Layer::Spans(mesocycles.to_vec()),
            Layer::Trend(
                component
                    .series
                    .iter()
                    .flat_map(|one| &one.trend)
                    .map(|run| {
                        run.iter()
                            .map(|day| Traced {
                                on: day.on,
                                value: day.value,
                                margin: day.margin,
                            })
                            .collect()
                    })
                    .collect(),
            ),
            Layer::Hollow(points(true)),
            Layer::Dots(points(false)),
            Layer::Aim(expected),
        ],
        days: days(component, &aimed, today),
    }
}

/// What a tap on each day shows: its figures, its trend, the trend's change
/// over [`AGAINST_DAYS`] days to it, and where the aims expected it.
fn days(
    component: &ComponentSeries,
    aimed: &BTreeMap<Date, f64>,
    today: Date,
) -> Vec<(Date, Markup)> {
    let unit = component.unit;
    let mut figures: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut trends: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut changes: BTreeMap<Date, Vec<Change>> = BTreeMap::new();
    for one in &component.series {
        for reading in &one.readings {
            figures
                .entry(reading.on)
                .or_default()
                .push(figure(reading.value, unit));
        }
        for day in one.trend.iter().flatten() {
            trends
                .entry(day.on)
                .or_default()
                .push(figure(day.value, unit));
        }
        for change in &one.changes {
            changes.entry(change.on).or_default().push(*change);
        }
    }
    let mut every: Vec<Date> = figures.keys().chain(trends.keys()).copied().collect();
    every.sort_unstable();
    every.dedup();
    every
        .into_iter()
        .map(|on| {
            let said = html! {
                span.on { (detail::day(on, today)) }
                @if let Some(figures) = figures.get(&on) {
                    " · " b { (figures.join(", ")) }
                }
                @if let Some(trends) = trends.get(&on) {
                    " · trend " (trends.join(", "))
                }
                @for change in changes.get(&on).into_iter().flatten() {
                    " · " (signed(change.by)) " " (unit.as_str()) " in " (AGAINST_DAYS) " days"
                }
                @if let Some(expected) = aimed.get(&on) {
                    " · expected " (figure(*expected, unit))
                }
            };
            (on, said)
        })
        .collect()
}

/// A figure to the nearest tenth, with its unit.
fn figure(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Percentage => format!("{value:.1}%"),
        Unit::Kilograms => format!("{value:.1} kg"),
    }
}

/// A change to the nearest hundredth, with its sign: one place more than a
/// figure, so a change just inside the noise does not read as one just
/// outside.
fn signed(by: f64) -> String {
    format!("{by:+.2}").replace('-', "−")
}

fn weigh_ins(count: usize) -> String {
    if count == 1 {
        "1 weigh-in".to_owned()
    } else {
        format!("{count} weigh-ins")
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            body_weight::Aim,
            composition::{ComponentSeries, Method, MethodSeries, Reading, Unit},
            range::{Over, Range, Ranges},
            recomposition::NutritionPhase,
        },
        body::Component,
    };

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    const SCAN: Method = Method::BodyScan {
        algorithm: 218_300_672,
    };

    fn reading(year: i16, month: i8, day: i8, value: f64) -> Reading<f64> {
        Reading {
            on: jiff::civil::date(year, month, day),
            value,
        }
    }

    fn drawn(components: &[ComponentSeries], aims: &[Aim], ranges: &Ranges, over: Over) -> String {
        crate::body::page(
            &domain::analytical::body_weight::Series::of(Vec::new()),
            aims,
            components,
            ranges,
            over,
            &crate::run::View::default(),
        )
        .into_string()
    }

    #[test]
    fn the_spreadsheets_points_are_hollow_and_nothing_joins_two() {
        let fat = ComponentSeries {
            component: Component::Fat,
            unit: Unit::Percentage,
            series: vec![
                MethodSeries::of(
                    Method::Spreadsheets,
                    vec![reading(2018, 1, 1, 24.7), reading(2018, 1, 2, 24.6)],
                ),
                MethodSeries::of(
                    SCAN,
                    vec![reading(2026, 10, 16, 17_610.0 / 83_648.0 * 100.0)],
                ),
            ],
        };
        let page = drawn(&[fat], &[], &NO_PLAN, Over::All);

        assert!(page.contains("<h2>Fat</h2>"), "{page}");
        assert_eq!(page.matches(r#"class="hollow""#).count(), 2, "{page}");
        assert_eq!(page.matches(r#"class="dot""#).count(), 1, "{page}");
        assert!(page.contains("<b>24.7%</b>"), "{page}");
        assert!(page.contains("<b>21.1%</b>"), "{page}");
        assert!(!page.contains("<polyline"), "{page}");
        assert!(page.contains("hollow from the spreadsheets"), "{page}");
    }

    #[test]
    fn fat_in_two_units_is_one_card_with_a_choice_kilograms_first() {
        let fat = |unit, value| ComponentSeries {
            component: Component::Fat,
            unit,
            series: vec![MethodSeries::of(SCAN, vec![reading(2026, 10, 16, value)])],
        };
        let page = drawn(
            &[fat(Unit::Kilograms, 17.61), fat(Unit::Percentage, 21.05)],
            &[],
            &NO_PLAN,
            Over::All,
        );

        assert_eq!(page.matches("<h2>Fat</h2>").count(), 1, "{page}");
        assert!(
            page.contains(
                r#"id="fat-unit-0" class="o0" checked><label for="fat-unit-0">kg</label>"#
            ),
            "kg first and chosen: {page}"
        );
        assert!(
            page.contains(r#"<label for="fat-unit-1">%</label>"#),
            "{page}"
        );
        assert!(page.contains("<b>17.6 kg</b>"), "{page}");
        assert!(page.contains("<b>21.1%</b>"), "{page}");
        assert!(!page.contains("spreadsheets"), "{page}");
    }

    #[test]
    fn the_summary_is_the_latest_trend_and_its_change_against_the_noise() {
        let readings: Vec<Reading<f64>> = jiff::civil::date(2026, 1, 1)
            .series(jiff::Span::new().days(1))
            .take(70)
            .enumerate()
            .map(|(day, on)| {
                let level = if day < 35 { 62.0 } else { 63.0 };
                let wobble = if day % 2 == 0 { 0.1 } else { -0.1 };
                Reading {
                    on,
                    value: level + wobble,
                }
            })
            .collect();
        let fat_free = ComponentSeries {
            component: Component::FatFree,
            unit: Unit::Kilograms,
            series: vec![MethodSeries::of(SCAN, readings)],
        };
        let page = drawn(&[fat_free], &[], &NO_PLAN, Over::All);

        assert_eq!(
            page.matches(r#"class="line""#).count(),
            1,
            "one unbroken run: {page}"
        );
        assert!(page.contains(r#"class="cell fat-free""#), "{page}");
        assert!(
            page.contains(r#"<div class="figure">63.0<small>kg</small></div>"#),
            "{page}"
        );
        assert!(
            page.contains("<b>+0.00 kg</b><span>in 28 days</span>"),
            "{page}"
        );
        assert!(page.contains(r#"class="noise""#), "{page}");
        assert!(
            page.contains("Trend on 11 Mar · Body Scan, algorithm 218300672"),
            "{page}"
        );
        assert!(!page.contains("rising"), "no verdict words: {page}");
        assert!(
            !page.contains(r#"class="badge"#),
            "no badge, which would contradict the line: {page}"
        );
    }

    #[test]
    fn nothing_recorded_is_said_rather_than_drawn() {
        let page = drawn(&[], &[], &NO_PLAN, Over::All);
        assert!(page.contains("Nothing has been recorded."), "{page}");
        assert!(!page.contains(r#"class="plotted""#), "{page}");
    }

    #[test]
    fn a_range_shows_only_its_days() {
        let fat = ComponentSeries {
            component: Component::Fat,
            unit: Unit::Percentage,
            series: vec![
                MethodSeries::of(Method::Spreadsheets, vec![reading(2018, 1, 1, 24.7)]),
                MethodSeries::of(SCAN, vec![reading(2026, 10, 16, 21.1)]),
            ],
        };
        let ranges = Ranges {
            mesocycle: Some(Range {
                from: jiff::civil::date(2026, 9, 28),
                to: jiff::civil::date(2026, 10, 19),
            }),
            macrocycle: None,
        };
        let page = drawn(&[fat], &[], &ranges, Over::Mesocycle);

        assert!(!page.contains("24.7%"), "before this mesocycle: {page}");
        assert!(!page.contains(r#"class="hollow""#), "{page}");
        assert!(!page.contains("spreadsheets"), "{page}");
        assert_eq!(page.matches(r#"class="dot""#).count(), 1, "{page}");
        assert!(page.contains(r#"href="/body?over=all""#), "{page}");
    }

    /// **Fat falls as the aim does and fat-free mass holds**, each from its
    /// trend on the first day of the mesocycle, reset at the next.
    #[test]
    fn fat_and_fat_free_mass_are_drawn_against_the_aims() {
        let component = |component, value| ComponentSeries {
            component,
            unit: Unit::Kilograms,
            series: vec![MethodSeries::of(
                SCAN,
                jiff::civil::date(2026, 9, 14)
                    .series(jiff::Span::new().days(1))
                    .take(43)
                    .map(|on| Reading { on, value })
                    .collect(),
            )],
        };
        let cut = |start, weeks| Aim {
            span: domain::plan::Span::new(start, weeks),
            phase: NutritionPhase::Cut,
            per_week: Some(-0.25),
        };
        let ranges = Ranges {
            mesocycle: None,
            macrocycle: Some(Range {
                from: jiff::civil::date(2026, 9, 14),
                to: jiff::civil::date(2026, 10, 26),
            }),
        };
        let page = drawn(
            &[
                component(Component::Fat, 19.0),
                component(Component::FatFree, 65.5),
            ],
            &[
                cut(jiff::civil::date(2026, 9, 14), 2),
                cut(jiff::civil::date(2026, 9, 28), 4),
                cut(jiff::civil::date(2026, 10, 26), 4),
            ],
            &ranges,
            Over::Macrocycle,
        );

        assert_eq!(
            page.matches(r#"class="aim""#).count(),
            6,
            "three mesocycles each, reset at each: {page}"
        );
        assert!(
            page.contains("expected 18.0 kg"),
            "four weeks of the cut from 28 Sep's trend, by 25 Oct: {page}"
        );
        assert!(page.contains("expected 65.5 kg"), "{page}");
        assert!(!page.contains("matrix"), "the lines replaced it: {page}");
        assert_eq!(
            page.matches(r#"class="span""#).count(),
            4,
            "Mesocycles 0 and 2 shaded on each chart: {page}"
        );
        assert!(page.contains(">Mesocycle 1</text>"), "{page}");
    }
}
