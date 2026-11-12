//! Body composition, on the detail template (#619).
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
//! **The mesocycle matrix comes after the two charts** (operator, 2026-10-28):
//! each mesocycle of this macrocycle, per method, against its nutrition
//! phase's region and, beneath it, the goal's half-plane (#489, #519).

use std::collections::BTreeMap;

use application::composition::AgainstPhase;
use domain::{
    analytical::{
        composition::{AGAINST_DAYS, Change, ComponentSeries, Method, MethodSeries, TREND, Unit},
        confidence::BAND,
        range::{Over, Range, Ranges},
        recomposition::{Cell as Moved, GOAL, MesocycleReading, NutritionPhase},
    },
    body::Component,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced},
    run::View,
};

const TITLE: &str = "Body composition";
const HERE: &str = "/body-composition";

/// The whole page, over the range chosen.
pub fn page(
    components: &[ComponentSeries],
    against: &AgainstPhase,
    ranges: &Ranges,
    over: Over,
    run: &View,
) -> Markup {
    detail::page(
        HERE,
        TITLE,
        Some(over),
        run,
        &body(components, against, ranges, over),
    )
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::bar(HERE))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

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

/// The summary, the two charts over the range chosen, the matrix and the
/// latest weigh-ins, or why there is no range.
fn body(
    components: &[ComponentSeries],
    against: &AgainstPhase,
    ranges: &Ranges,
    over: Over,
) -> Markup {
    let recorded = Range::spanning(
        components
            .iter()
            .flat_map(|component| &component.series)
            .flat_map(|one| one.readings.iter().map(|reading| reading.on)),
    );
    let range = match crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.") {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let fat: Vec<(&str, Chart<'_>)> = [Unit::Kilograms, Unit::Percentage]
        .into_iter()
        .filter_map(|unit| charted(components, Component::Fat, unit))
        .map(|one| {
            (
                one.unit.as_str(),
                chart(&one.within(range), Metric::Fat, range, today),
            )
        })
        .collect();
    let fat_free = charted(components, Component::FatFree, Unit::Kilograms)
        .map(|one| chart(&one.within(range), Metric::FatFree, range, today));
    let hollow = components
        .iter()
        .map(|component| component.within(range))
        .any(|component| {
            component
                .series
                .iter()
                .any(|one| one.method == Method::Spreadsheets && !one.readings.is_empty())
        });
    html! {
        (summary(components, today))
        @match fat.as_slice() {
            [] => {}
            [(_, one)] => (detail::chart(one)),
            _ => (detail::choice("fat-unit", Metric::Fat, heading(Component::Fat), &fat)),
        }
        @if let Some(one) = &fat_free {
            (detail::chart(one))
        }
        (against_phase(against))
        (latest(components, today))
        (detail::foot(&if hollow {
            format!("Dots: each weigh-in, hollow from the spreadsheets. Line: each method's trend, the {TREND}. Band: {BAND}.")
        } else {
            format!("Dots: each weigh-in. Line: the trend, the {TREND}. Band: {BAND}.")
        }))
    }
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
/// [`AGAINST_DAYS`] days against the method's noise.
fn summary(components: &[ComponentSeries], today: Date) -> Markup {
    let read: Vec<(
        Metric,
        &str,
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
    if read.is_empty() {
        return html! {};
    }
    let cells: Vec<Cell<'_>> = read
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
    detail::summary(&cells, &asof.join("; "))
}

/// One component in one unit over `range`: each method's trend, and its
/// points, solid or hollow.
fn chart(component: &ComponentSeries, metric: Metric, range: Range, today: Date) -> Chart<'static> {
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
        ],
        days: days(component, today),
    }
}

/// What a tap on each day shows: its figures, its trend, and the trend's
/// change over [`AGAINST_DAYS`] days to it.
fn days(component: &ComponentSeries, today: Date) -> Vec<(Date, Markup)> {
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

/// The latest seven weigh-ins, each its fat and fat-free mass.
fn latest(components: &[ComponentSeries], today: Date) -> Markup {
    let mut weighed: BTreeMap<(Date, Method), (Option<f64>, Option<f64>)> = BTreeMap::new();
    for (component, fat) in [(Component::Fat, true), (Component::FatFree, false)] {
        for one in charted(components, component, Unit::Kilograms)
            .into_iter()
            .flat_map(|charted| &charted.series)
        {
            for reading in &one.readings {
                let entry = weighed.entry((reading.on, one.method)).or_default();
                if fat {
                    entry.0 = Some(reading.value);
                } else {
                    entry.1 = Some(reading.value);
                }
            }
        }
    }
    let tenths =
        |value: Option<f64>| value.map_or_else(|| "–".to_owned(), |value| format!("{value:.1}"));
    let rows: Vec<(String, Markup)> = weighed
        .iter()
        .rev()
        .take(7)
        .map(|((on, _), (fat, fat_free))| {
            (
                detail::day(*on, today),
                html! { span.fat { (tenths(*fat)) } " · " span.fat-free { (tenths(*fat_free)) } },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Weigh-ins", "fat · fat-free, kg", &rows)
}

/// The current macrocycle, one matrix per nutrition phase and a point per
/// mesocycle.
fn against_phase(against: &AgainstPhase) -> Markup {
    match against {
        AgainstPhase::Unreadable(error) => html! {
            section.card { p.failed { "the school calendar could not be read: " (error) } }
        },
        AgainstPhase::NoMacrocycle => html! {},
        AgainstPhase::Read(read) => html! {
            section.card.matrix {
                div.head { h2 { (capitalised(&read.macrocycle.to_string())) } span { "kg a week" } }
                p.sub { "Each mesocycle's change in fat against fat-free mass" }
                @for phase in [NutritionPhase::Cut, NutritionPhase::Maintenance, NutritionPhase::Bulk] {
                    @let mesocycles: Vec<&MesocycleReading> = read
                        .mesocycles
                        .iter()
                        .filter(|one| one.phase == phase)
                        .collect();
                    @if mesocycles.is_empty() {
                        // No macrocycle phase assumes a bulk, so none is due.
                        @if phase != NutritionPhase::Bulk {
                            p.sub { (capitalised(&phase.to_string())) ": no mesocycle yet" }
                        }
                    } @else {
                        (matrix(phase, &mesocycles))
                    }
                }
            }
        },
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

const SIDE: f64 = 360.0;
const MARGIN: f64 = 40.0;
/// Room for the value axis's labels and its name beside them.
const MATRIX_LEFT: f64 = 56.0;

/// One mesocycle by one method, per week.
struct Plotted<'a> {
    mesocycle: &'a MesocycleReading,
    method: Method,
    cell: Moved,
    fat: f64,
    fat_free: f64,
    fat_noise: f64,
    fat_free_noise: f64,
}

impl<'a> Plotted<'a> {
    fn of(mesocycle: &'a MesocycleReading, method: Method, cell: Moved) -> Option<Self> {
        let weeks = cell.weeks();
        (weeks > 0.0).then(|| Self {
            mesocycle,
            method,
            cell,
            fat: cell.fat.by / weeks,
            fat_free: cell.fat_free.by / weeks,
            fat_noise: cell.fat.noise / weeks,
            fat_free_noise: cell.fat_free.noise / weeks,
        })
    }

    /// What the point is called: the day its mesocycle starts.
    fn label(&self) -> String {
        let start = self.mesocycle.span.start().strftime("%-d %b");
        let so_far = if self.mesocycle.running {
            ", so far"
        } else {
            ""
        };
        format!("{start}{so_far}")
    }

    fn detail(&self) -> String {
        format!(
            "{} · mesocycle from {} · trend on {} against {} · fat {} · fat-free {}",
            self.method,
            self.mesocycle.span.start(),
            self.cell.to,
            self.cell.from,
            per_week(self.fat, self.fat_noise),
            per_week(self.fat_free, self.fat_free_noise),
        )
    }
}

fn per_week(value: f64, noise: f64) -> String {
    format!("{} ±{noise:.2} kg a week", signed(value))
}

/// Half the side of the square the points fall in: whole steps past the
/// furthest end of any point's noise.
fn extent(plotted: &[Plotted<'_>]) -> (f64, f64) {
    let furthest = plotted
        .iter()
        .map(|one| (one.fat.abs() + one.fat_noise).max(one.fat_free.abs() + one.fat_free_noise))
        .fold(0.25, f64::max);
    let step = [0.1, 0.25, 0.5, 1.0, 2.0]
        .into_iter()
        .find(|step| furthest / step <= 4.0)
        .unwrap_or(5.0);
    ((furthest / step).ceil() * step, step)
}

/// A nutrition phase's mesocycles as points, fat across and fat-free mass up,
/// each in kilograms a week with its noise either way in the component's
/// colour, and beneath, each one's figures.
fn matrix(phase: NutritionPhase, mesocycles: &[&MesocycleReading]) -> Markup {
    let plotted: Vec<Plotted<'_>> = mesocycles
        .iter()
        .flat_map(|mesocycle| {
            mesocycle.by_method.iter().filter_map(|one| {
                one.cell
                    .and_then(|cell| Plotted::of(mesocycle, one.method, cell))
            })
        })
        .collect();
    let unread: Vec<String> = mesocycles
        .iter()
        .filter(|one| one.by_method.iter().all(|by| by.cell.is_none()))
        .map(|one| one.span.start().strftime("%-d %b").to_string())
        .collect();
    let (extent, step) = extent(&plotted);
    let scale = (SIDE - MATRIX_LEFT - MARGIN) / (2.0 * extent);
    let x = |value: f64| (value + extent).mul_add(scale, MATRIX_LEFT);
    let y = |value: f64| (extent - value).mul_add(scale, MARGIN / 2.0);
    let lines: Vec<f64> = (0_u32..=40)
        .map(|line| f64::from(line).mul_add(step, -extent))
        .take_while(|value| *value <= extent + step / 2.0)
        .collect();
    let name = capitalised(&phase.to_string());
    html! {
        h3 { (name) }
        svg viewBox=(format!("0 0 {SIDE} {:.0}", y(-extent) + 38.0)) role="img"
            aria-label=(format!("{name}: each mesocycle's change in fat against fat-free mass, in kilograms a week")) {
            (regions(phase, extent, &x, &y))
            @for value in &lines {
                line.grid x1=(x(*value)) x2=(x(*value)) y1=(y(extent)) y2=(y(-extent)) {}
                line.grid x1=(x(-extent)) x2=(x(extent)) y1=(y(*value)) y2=(y(*value)) {}
            }
            line.baseline x1=(x(0.0)) x2=(x(0.0)) y1=(y(extent)) y2=(y(-extent)) {}
            line.baseline x1=(x(-extent)) x2=(x(extent)) y1=(y(0.0)) y2=(y(0.0)) {}
            @for value in &lines {
                text.tick x=(x(*value)) y=(y(-extent) + 14.0) text-anchor="middle" { (tick(*value, step)) }
                text.tick x=(x(-extent) - 4.0) y=(y(*value) + 4.0) text-anchor="end" { (tick(*value, step)) }
            }
            text.fat x=(x(0.0)) y=(y(-extent) + 30.0) text-anchor="middle" { "fat →" }
            text.fat-free transform=(format!("translate(14 {}) rotate(-90)", y(0.0))) text-anchor="middle" { "fat-free mass →" }
            @for one in &plotted {
                g {
                    title { (one.detail()) }
                    line.spread.fat x1=(x(one.fat - one.fat_noise)) x2=(x(one.fat + one.fat_noise)) y1=(y(one.fat_free)) y2=(y(one.fat_free)) {}
                    line.spread.fat-free x1=(x(one.fat)) x2=(x(one.fat)) y1=(y(one.fat_free - one.fat_free_noise)) y2=(y(one.fat_free + one.fat_free_noise)) {}
                    circle class=(if one.method == Method::Spreadsheets { "point hollow" } else { "point" })
                        cx=(x(one.fat)) cy=(y(one.fat_free)) r="5" {}
                    text.label x=(x(one.fat) + 8.0) y=(y(one.fat_free) - 8.0) { (one.label()) }
                }
            }
        }
        @for one in &plotted {
            div.row {
                span.on { (one.label()) }
                span.value {
                    span.fat { (signed(one.fat)) small { " ±" (format!("{:.2}", one.fat_noise)) } }
                    " · "
                    span.fat-free { (signed(one.fat_free)) small { " ±" (format!("{:.2}", one.fat_free_noise)) } }
                }
            }
        }
        @if !unread.is_empty() {
            p.sub { (unread.join(" and ")) ": too few weigh-ins yet" }
        }
    }
}

/// The goal's half-plane and, over it, the phase's region, each named, in
/// neutral grey.
fn regions(
    phase: NutritionPhase,
    extent: f64,
    x: &impl Fn(f64) -> f64,
    y: &impl Fn(f64) -> f64,
) -> Markup {
    let corners = |corners: &[(f64, f64)]| {
        corners
            .iter()
            .map(|(across, up)| format!("{:.1},{:.1}", x(across * extent), y(up * extent)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    // The goal's own name sits in a wedge outside the phase's region.
    let (goal_x, goal_y) = match phase {
        NutritionPhase::Cut | NutritionPhase::Maintenance => (x(0.0) + 6.0, y(extent) + 16.0),
        NutritionPhase::Bulk => (x(-extent) + 6.0, y(-extent / 2.0) + 4.0),
    };
    html! {
        polygon.goal points=(corners(&GOAL)) {}
        polygon.phase points=(corners(&phase.region())) {}
        text.region x=(x(-extent) + 6.0) y=(y(extent) + 16.0) { (phase) }
        text.region x=(goal_x) y=(goal_y) { "goal" }
    }
}

fn tick(value: f64, step: f64) -> String {
    let text = if step < 0.25 {
        format!("{value:.1}")
    } else if step < 1.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.0}")
    };
    text.replace('-', "−")
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use application::composition::AgainstPhase;
    use domain::{
        analytical::{
            composition::{
                ComponentSeries, Measured, Method, MethodSeries, Reading, Stated, Unit, by_method,
            },
            range::{Over, Range, Ranges},
            recomposition::read,
        },
        body::Component,
        macrocycle::Macrocycle,
        measure::Kg,
        schedule::{Holidays, SchoolHoliday},
    };

    use super::page;

    const NONE: AgainstPhase = AgainstPhase::NoMacrocycle;

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

    fn drawn(
        components: &[ComponentSeries],
        against: &AgainstPhase,
        ranges: &Ranges,
        over: Over,
    ) -> String {
        page(
            components,
            against,
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
        let page = drawn(&[fat], &NONE, &NO_PLAN, Over::All);

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
            &NONE,
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
        let page = drawn(&[fat_free], &NONE, &NO_PLAN, Over::All);

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
        let page = drawn(&[], &NONE, &NO_PLAN, Over::All);
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
        let page = drawn(&[fat], &NONE, &ranges, Over::Mesocycle);

        assert!(!page.contains("24.7%"), "before this mesocycle: {page}");
        assert!(!page.contains(r#"class="hollow""#), "{page}");
        assert!(!page.contains("spreadsheets"), "{page}");
        assert_eq!(page.matches(r#"class="dot""#).count(), 1, "{page}");
        assert!(
            page.contains(r#"href="/body-composition?over=all""#),
            "{page}"
        );
    }

    /// Autumn 2026, from the school's feed.
    fn autumn() -> Option<Macrocycle> {
        let holiday = |first, days| Some(SchoolHoliday::new(first, NonZeroU8::new(days)?));
        let school = Holidays::from_school(
            vec![
                holiday(jiff::civil::date(2026, 7, 21), 47)?,
                holiday(jiff::civil::date(2026, 12, 21), 14)?,
            ],
            Some(jiff::civil::date(2027, 1, 3)),
        );
        let public = Holidays::from_public(
            vec![
                domain::schedule::PublicHoliday::new(
                    jiff::civil::date(2026, 8, 31),
                    "Summer bank holiday".to_owned(),
                )
                .naming(domain::schedule::SchoolHolidayKind::Summer),
                domain::schedule::PublicHoliday::new(
                    jiff::civil::date(2026, 12, 25),
                    "Christmas Day".to_owned(),
                ),
            ],
            Some(jiff::civil::date(2027, 12, 31)),
        );
        Macrocycle::on(jiff::civil::date(2026, 10, 19), &school.and(public))
    }

    #[test]
    fn each_mesocycle_is_a_point_on_its_phases_matrix_after_the_charts() {
        let measured: Vec<Measured> = jiff::civil::date(2026, 9, 7)
            .series(jiff::Span::new().days(1))
            .take(43)
            .zip(0_u64..)
            .map(|(on, day)| {
                let wobble = if day % 2 == 0 { 100 } else { 0 };
                let fat = 19_100 - day * 25 - wobble;
                Measured {
                    on,
                    method: SCAN,
                    component: Component::Fat,
                    stated: Stated::Mass(Kg::from_grams(fat)),
                    weighed: Kg::from_grams(fat + 66_000 + wobble),
                }
            })
            .collect();
        let against = AgainstPhase::Read(read(
            &measured,
            autumn().expect("a macrocycle"),
            &[
                domain::plan::Span::new(jiff::civil::date(2026, 9, 14), 2),
                domain::plan::Span::new(jiff::civil::date(2026, 9, 28), 4),
            ],
            jiff::civil::date(2026, 10, 19),
        ));
        let page = drawn(&by_method(&measured), &against, &NO_PLAN, Over::All);

        assert!(page.contains("<h2>Autumn 2026</h2>"), "{page}");
        assert!(page.contains("<h3>Cut</h3>"), "{page}");
        assert_eq!(page.matches(r#"<polygon class="phase""#).count(), 1);
        assert_eq!(
            page.matches(r#"<polygon class="goal""#).count(),
            1,
            "the goal is drawn whatever the phase"
        );
        assert!(page.contains(">cut</text>"), "{page}");
        assert!(page.contains(">goal</text>"), "{page}");
        assert!(!page.contains("Bulk"), "no bulk is due: {page}");
        assert!(page.contains(">14 Sep</text>"), "{page}");
        assert!(page.contains(">28 Sep, so far</text>"), "{page}");
        assert!(page.contains("Maintenance: no mesocycle yet"), "{page}");
        let chart = page
            .find("<h2>Fat-free mass</h2>")
            .expect("a fat-free chart");
        let matrix = page.find("<h2>Autumn 2026</h2>").expect("a matrix");
        assert!(chart < matrix, "the matrix comes after the charts");
    }
}
