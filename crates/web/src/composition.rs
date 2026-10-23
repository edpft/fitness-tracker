//! Body composition, as charts (#256).
//!
//! **One chart per component, date across, the figure up, one colour per
//! method** (#499). A component charted in more than one unit is one chart
//! with a choice between them, the first the default (operator, 2026-10-20),
//! switched in the page so no round trip is made. Nothing joins two points, so a gap in the record is the empty
//! stretch it is (§ 37), and nothing joins two methods, since each one's
//! figures are its own series (§ 6). A method keeps its colour on every chart.
//!
//! **The trend is a line over each method's points** (#419), broken where the
//! record is too thin for one, and each method's last reading against 28 days
//! before is said above its chart.
//!
//! **The page leads with the current macrocycle against its nutrition
//! phases** (#489): each period's cell, per method, against its phase's
//! region and, beneath it, the goal's half-plane, whatever the phase (#519).
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath each chart is the same figures.

use application::composition::AgainstPhase;
use domain::{
    analytical::{
        composition::{
            AGAINST_DAYS, Change, ComponentSeries, Direction, Method, TREND, Trend, Unit,
        },
        range::{Over, Range, Ranges},
        recomposition::{Cell, GOAL, MesocycleReading, NutritionPhase},
    },
    body::Component,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::along,
    range::{seconds, ticks},
    run::View,
};

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 300.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 276.0;
const RADIUS: f64 = 2.0;

/// A method's colour, in the order the methods first appear on the page.
const COLOURS: [&str; 4] = ["d1", "d2", "d3", "d4"];

/// The steps a value axis may be divided in, finest first.
const STEPS: [f64; 9] = [0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0];

/// The most gridlines a chart's value axis is divided into.
const MOST_GRIDLINES: f64 = 8.0;

const TITLE: &str = "Body composition";

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

/// A figure as it is printed.
fn figure(value: f64, unit: Unit) -> String {
    match unit {
        Unit::Percentage => format!("{value:.1}%"),
        Unit::Kilograms => format!("{value:.2} kg"),
    }
}

/// A change as it is printed, with its sign, to one place more than a figure
/// so that a change just inside the noise does not read as one just outside.
fn change(change: Change, unit: Unit) -> String {
    let direction = match change.direction {
        Direction::Rising => "rising",
        Direction::Maintaining => "maintaining",
        Direction::Falling => "falling",
    };
    let (by, noise) = match unit {
        Unit::Percentage => (
            format!("{:+.2}%", change.by),
            format!("±{:.2}%", change.noise),
        ),
        Unit::Kilograms => (
            format!("{:+.2} kg", change.by),
            format!("±{:.2} kg", change.noise),
        ),
    };
    format!(
        "{} against {AGAINST_DAYS} days before, noise {noise}: {direction}",
        by.replace('-', "−")
    )
}

/// Every method on the page, in the order it first appears.
fn methods(components: &[ComponentSeries]) -> Vec<Method> {
    let mut methods: Vec<(Date, Method)> = Vec::new();
    for one in components.iter().flat_map(|component| &component.series) {
        let Some(first) = one.readings.first().map(|reading| reading.on) else {
            continue;
        };
        match methods.iter_mut().find(|(_, method)| *method == one.method) {
            Some((earliest, _)) => *earliest = (*earliest).min(first),
            None => methods.push((first, one.method)),
        }
    }
    methods.sort();
    methods.into_iter().map(|(_, method)| method).collect()
}

fn class(methods: &[Method], method: Method) -> &'static str {
    methods
        .iter()
        .position(|one| *one == method)
        .and_then(|index| COLOURS.get(index % COLOURS.len()).copied())
        .unwrap_or("ink")
}

/// The whole page, over the range chosen.
///
/// **A method keeps its colour over every range**, so the colours are the
/// whole record's.
pub fn page(
    components: &[ComponentSeries],
    against: &AgainstPhase,
    ranges: &Ranges,
    over: Over,
    run: &View,
) -> Markup {
    let methods = methods(components);
    let recorded = Range::spanning(
        components
            .iter()
            .flat_map(|component| &component.series)
            .flat_map(|one| one.readings.iter().map(|reading| reading.on)),
    );
    let range = crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.");
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (crate::chart::STYLE) (crate::daily::STYLE) (MATRIX) (UNITS) }
            }
            body {
                (crate::page::following(run))
                main.wide {
                    (crate::page::menu("/body-composition"))
                    h1 { (TITLE) }
                    (against_phase(against, &methods))
                    (crate::range::choices("/body-composition", over))
                    p.method {
                        "points: each weigh-in, coloured by what measured it · line: trend, the " (TREND)
                        " · against " (AGAINST_DAYS) " days before: rising or falling beyond 1.96 × the method's own scatter between consecutive days, maintaining within it"
                    }
                    @match range {
                        Err(said) => (said),
                        Ok(range) => (charts(components, &methods, range)),
                    }
                }
            }
        }
    }
}

/// The current macrocycle, one chart per nutrition phase and a point per
/// mesocycle.
fn against_phase(against: &AgainstPhase, methods: &[Method]) -> Markup {
    html! {
        @match against {
            AgainstPhase::Unreadable(error) => {
                p.failed { "the school calendar could not be read: " (error) }
            }
            AgainstPhase::NoMacrocycle => {}
            AgainstPhase::Read(read) => {
                h2 { (capitalised(&read.macrocycle.to_string())) }
                @for phase in [NutritionPhase::Cut, NutritionPhase::Maintenance, NutritionPhase::Bulk] {
                    @let mesocycles: Vec<&MesocycleReading> = read
                        .mesocycles
                        .iter()
                        .filter(|one| one.phase == phase)
                        .collect();
                    @if mesocycles.is_empty() {
                        // No macrocycle phase assumes a bulk, so none is due.
                        @if phase != NutritionPhase::Bulk {
                            p.method { (capitalised(&phase.to_string())) ": no mesocycle yet" }
                        }
                    } @else {
                        (matrix(phase, &mesocycles, methods))
                    }
                }
            }
        }
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
    cell: Cell,
    fat: f64,
    fat_free: f64,
    fat_noise: f64,
    fat_free_noise: f64,
}

impl<'a> Plotted<'a> {
    fn of(mesocycle: &'a MesocycleReading, method: Method, cell: Cell) -> Option<Self> {
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

    /// What the point is called on the chart: the day its mesocycle starts.
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
    format!("{value:+.2} ±{noise:.2} kg a week").replace('-', "−")
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
/// each in kilograms a week with its noise either way.
fn matrix(phase: NutritionPhase, mesocycles: &[&MesocycleReading], methods: &[Method]) -> Markup {
    let plotted: Vec<Plotted<'_>> = mesocycles
        .iter()
        .flat_map(|mesocycle| {
            mesocycle.by_method.iter().filter_map(|one| {
                one.cell
                    .and_then(|cell| Plotted::of(mesocycle, one.method, cell))
            })
        })
        .collect();
    let unread: Vec<&MesocycleReading> = mesocycles
        .iter()
        .copied()
        .filter(|one| one.by_method.iter().all(|by| by.cell.is_none()))
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
    let named_methods = plotted
        .iter()
        .map(|one| one.method)
        .collect::<std::collections::BTreeSet<_>>();
    html! {
        h3 { (name) }
        div.matrix {
            svg.matrix viewBox=(format!("0 0 {SIDE} {:.0}", y(-extent) + 38.0)) role="img"
                aria-label=(format!("{name}: each mesocycle's change in fat against fat-free mass, in kilograms a week")) {
                (regions(phase, extent, &x, &y))
                @for value in &lines {
                    line.grid x1=(x(*value)) x2=(x(*value)) y1=(y(extent)) y2=(y(-extent)) {}
                    line.grid x1=(x(-extent)) x2=(x(extent)) y1=(y(*value)) y2=(y(*value)) {}
                }
                line.baseline x1=(x(0.0)) x2=(x(0.0)) y1=(y(extent)) y2=(y(-extent)) {}
                line.baseline x1=(x(-extent)) x2=(x(extent)) y1=(y(0.0)) y2=(y(0.0)) {}
                @for value in &lines {
                    text.axis x=(x(*value)) y=(y(-extent) + 14.0) text-anchor="middle" { (tick(*value, step)) }
                    text.axis x=(x(-extent) - 4.0) y=(y(*value) + 4.0) text-anchor="end" { (tick(*value, step)) }
                }
                text.axis x=(x(0.0)) y=(y(-extent) + 30.0) text-anchor="middle" { "fat, kg a week" }
                text.axis transform=(format!("translate(10 {}) rotate(-90)", y(0.0))) text-anchor="middle" { "fat-free mass, kg a week" }
                @for one in &plotted {
                    g {
                        title { (one.detail()) }
                        line.spread x1=(x(one.fat - one.fat_noise)) x2=(x(one.fat + one.fat_noise)) y1=(y(one.fat_free)) y2=(y(one.fat_free)) {}
                        line.spread x1=(x(one.fat)) x2=(x(one.fat)) y1=(y(one.fat_free - one.fat_free_noise)) y2=(y(one.fat_free + one.fat_free_noise)) {}
                        circle class=(format!("point {}", class(methods, one.method))) cx=(x(one.fat)) cy=(y(one.fat_free)) r="5" {}
                        text.label x=(x(one.fat) + 8.0) y=(y(one.fat_free) - 8.0) { (one.label()) }
                    }
                }
            }
        }
        @if named_methods.len() > 1 {
            ul.legend {
                @for method in &named_methods {
                    li {
                        svg.key viewBox="0 0 12 12" {
                            circle class=(format!("point {}", class(methods, *method))) cx="6" cy="6" r="5" {}
                        }
                        (method)
                    }
                }
            }
        }
        @for one in unread {
            p.method { (one.span.start().strftime("%-d %b")) ": too few weigh-ins yet" }
        }
        @if !plotted.is_empty() {
            details {
                summary { "Every mesocycle as a table" }
                table.figures {
                    thead { tr { th { "mesocycle from" } th { "fat" } th { "fat-free mass" } } }
                    tbody {
                        @for one in &plotted {
                            tr {
                                td { (one.mesocycle.span.start()) }
                                td { (per_week(one.fat, one.fat_noise)) }
                                td { (per_week(one.fat_free, one.fat_free_noise)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The goal's half-plane and, over it, the phase's region, each named.
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

/// How the matrix is drawn: the goal and, over it, the phase's region in the
/// status colour for good, never a series colour, each named in the chart so
/// it is not colour alone.
const MATRIX: &str = "
:root { --good: #0ca30c; }
div.matrix { max-width: 28rem; }
svg.matrix { width: 100%; height: auto; display: block; }
.goal { fill: var(--good); fill-opacity: 0.08; }
.phase { fill: var(--good); fill-opacity: 0.16; }
.region { fill: var(--muted); font-size: 11px; }
.spread { stroke: var(--ink); stroke-opacity: 0.55; stroke-width: 1.5; }
.label { fill: var(--ink); font-size: 11px; }
";

/// Only the chosen unit's chart is shown.
const UNITS: &str = "
div.units label { margin: 0 0.75rem 0 0.25rem; }
div.units input.kilograms:checked ~ div.unit:not(.kilograms),
div.units input.percentage:checked ~ div.unit:not(.percentage) { display: none; }
";

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) }
            }
            body {
                main {
                    (crate::page::menu("/body-composition"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// Where a day and a figure fall on the chart.
struct Frame {
    first: i64,
    last: i64,
    low: f64,
    high: f64,
    step: f64,
}

impl Frame {
    fn x(&self, at: i64) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(along(at - self.first, self.last - self.first), LEFT)
    }

    fn y(&self, value: f64) -> f64 {
        (FLOOR - TOP).mul_add(-(value - self.low) / (self.high - self.low), FLOOR)
    }

    /// Every gridline's value, lowest first.
    fn gridlines(&self) -> Vec<f64> {
        (0_u32..=20)
            .map(|line| f64::from(line).mul_add(self.step, self.low))
            .take_while(|value| *value <= self.high + self.step / 2.0)
            .collect()
    }
}

/// The value axis: the coarsest step that still divides the range into enough
/// lines, its ends on whole steps either side of every figure.
fn value_axis(low: f64, high: f64) -> (f64, f64, f64) {
    let range = (high - low).max(f64::EPSILON);
    let step = STEPS
        .iter()
        .copied()
        .find(|step| range / step <= MOST_GRIDLINES)
        .unwrap_or(100.0);
    let low = (low / step).floor() * step;
    let high = ((high / step).floor() + 1.0) * step;
    (low, high, step)
}

/// Every component measured in `range`, each over it, one chart per
/// component whatever its units.
fn charts(components: &[ComponentSeries], methods: &[Method], range: Range) -> Markup {
    let within: Vec<ComponentSeries> = components
        .iter()
        .map(|component| component.within(range))
        .filter(|component| component.series.iter().any(|one| !one.readings.is_empty()))
        .collect();
    let mut grouped: Vec<Vec<&ComponentSeries>> = Vec::new();
    for one in &within {
        match grouped.iter_mut().find(|group| {
            group
                .first()
                .is_some_and(|first| first.component == one.component)
        }) {
            Some(group) => group.push(one),
            None => grouped.push(vec![one]),
        }
    }
    html! {
        @if within.is_empty() {
            p { "Nothing recorded in this range." }
        }
        @for group in &grouped {
            @match group.as_slice() {
                [one] => {
                    h2 { (heading(one.component)) " (" (one.unit.as_str()) ")" }
                    (chart(one, methods, range))
                }
                units => (choice(units, methods, range)),
            }
        }
    }
}

/// What a unit is called in a page's markup.
const fn key(unit: Unit) -> &'static str {
    match unit {
        Unit::Percentage => "percentage",
        Unit::Kilograms => "kilograms",
    }
}

/// One component in each of its units, the first shown and the rest a choice
/// away.
fn choice(units: &[&ComponentSeries], methods: &[Method], range: Range) -> Markup {
    let Some(first) = units.first() else {
        return html! {};
    };
    let name = heading(first.component);
    let group = format!("unit-{}", name.to_lowercase().replace(' ', "-"));
    html! {
        h2 { (name) }
        div.units {
            @for (index, one) in units.iter().enumerate() {
                @let id = format!("{group}-{}", key(one.unit));
                input type="radio" name=(group) id=(id) class=(key(one.unit)) checked[index == 0];
                label for=(id) { (one.unit.as_str()) }
            }
            @for one in units {
                div class=(format!("unit {}", key(one.unit))) {
                    (chart(one, methods, range))
                }
            }
        }
    }
}

/// A component's chart over `range`, its legend and its table.
fn chart(component: &ComponentSeries, methods: &[Method], range: Range) -> Markup {
    let readings = component.series.iter().flat_map(|one| one.readings.iter());
    let low = readings
        .clone()
        .map(|reading| reading.value)
        .fold(f64::INFINITY, f64::min);
    let high = readings
        .map(|reading| reading.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let (low, high, step) = value_axis(low, high);
    let frame = Frame {
        first: seconds(range.from),
        last: seconds(range.to),
        low,
        high,
        step,
    };
    let name = heading(component.component);
    let unit = component.unit;
    html! {
        @for one in &component.series {
            @if let Some(last) = one.changes.last() {
                p.reading { (one.method) " · " (last.on) ": " (change(*last, unit)) }
            }
        }
        div.plot {
            svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
                aria-label=(format!("{name}, one point per weigh-in, by date")) {
                @for value in frame.gridlines() {
                    line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.axis x=(LEFT - 6.0) y=(frame.y(value) + 4.0) text-anchor="end" { (gridline(value, step)) }
                }
                @for (tick, label) in ticks(range) {
                    @let at = seconds(tick);
                    line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
                    text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (label) }
                }
                line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
                @for one in &component.series {
                    @for run in &one.trend {
                        polyline.trend points=(points(run, &frame)) {}
                        @for day in run {
                            circle.hover cx=(frame.x(seconds(day.on))) cy=(frame.y(day.value)) r=(RADIUS) {
                                title {
                                    (day.on) " · trend " (figure(day.value, unit)) " · " (one.method)
                                    @if let Some(at) = one.changes.iter().find(|at| at.on == day.on) {
                                        " · " (change(*at, unit))
                                    }
                                }
                            }
                        }
                    }
                    @for reading in &one.readings {
                        circle class=(format!("point {}", class(methods, one.method)))
                            cx=(frame.x(seconds(reading.on))) cy=(frame.y(reading.value)) r=(RADIUS) {
                            title { (reading.on) " · " (figure(reading.value, unit)) " · " (one.method) }
                        }
                    }
                }
            }
        }
        ul.legend {
            @for one in component.series.iter().filter(|one| !one.readings.is_empty()) {
                li {
                    svg.key viewBox="0 0 12 12" {
                        circle class=(format!("point {}", class(methods, one.method))) cx="6" cy="6" r=(RADIUS) {}
                    }
                    (one.method) " · " (weigh_ins(one.readings.len()))
                }
            }
        }
        (table(component))
    }
}

/// A run of the trend as SVG polyline points.
fn points(run: &[Trend], frame: &Frame) -> String {
    run.iter()
        .map(|day| format!("{:.1},{:.1}", frame.x(seconds(day.on)), frame.y(day.value)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn weigh_ins(count: usize) -> String {
    if count == 1 {
        "1 weigh-in".to_owned()
    } else {
        format!("{count} weigh-ins")
    }
}

/// A gridline's label, to as many places as its step needs.
fn gridline(value: f64, step: f64) -> String {
    if step < 1.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

/// The same figures, for whoever cannot tell two colours apart.
fn table(component: &ComponentSeries) -> Markup {
    let mut rows: Vec<(Date, f64, Method)> = component
        .series
        .iter()
        .flat_map(|one| {
            one.readings
                .iter()
                .map(|reading| (reading.on, reading.value, one.method))
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)));
    html! {
        details {
            summary { "Every weigh-in as a table" }
            table.figures {
                thead { tr { th { "date" } th { (component.unit.as_str()) } th { "measured by" } } }
                tbody {
                    @for (on, value, method) in rows {
                        tr { td { (on) } td { (figure(value, component.unit)) } td { (method) } }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            composition::{ComponentSeries, Method, MethodSeries, Reading, Unit},
            range::{Over, Range, Ranges},
        },
        body::Component,
    };

    use std::num::NonZeroU8;

    use application::composition::AgainstPhase;
    use domain::{
        analytical::{
            composition::{Measured, Stated},
            recomposition::read,
        },
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

    fn reading(year: i16, month: i8, day: i8, value: f64) -> Reading<f64> {
        Reading {
            on: jiff::civil::date(year, month, day),
            value,
        }
    }

    #[test]
    fn each_method_is_its_own_colour_and_nothing_joins_two_points() {
        let fat = ComponentSeries {
            component: Component::Fat,
            unit: Unit::Percentage,
            series: vec![
                MethodSeries::of(
                    Method::Spreadsheets,
                    vec![reading(2018, 1, 1, 24.7), reading(2018, 1, 2, 24.6)],
                ),
                MethodSeries::of(
                    Method::BodyScan {
                        algorithm: 218_300_672,
                    },
                    vec![reading(2026, 10, 16, 17_610.0 / 83_648.0 * 100.0)],
                ),
            ],
        };
        let drawn = page(
            &[fat],
            &NONE,
            &NO_PLAN,
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();

        assert!(drawn.contains("<h2>Fat (%)</h2>"));
        assert_eq!(drawn.matches(r#"<circle class="point d1""#).count(), 3);
        assert_eq!(drawn.matches(r#"<circle class="point d2""#).count(), 2);
        assert!(drawn.contains("2018-01-01 · 24.7% · spreadsheets"));
        assert!(drawn.contains("2026-10-16 · 21.1% · Body Scan, algorithm 218300672"));
        assert!(!drawn.contains("<polyline") && !drawn.contains("<path"));
    }

    #[test]
    fn a_component_in_two_units_is_one_chart_with_a_choice() {
        let scan = Method::BodyScan {
            algorithm: 218_300_672,
        };
        let fat = |unit, value| ComponentSeries {
            component: Component::Fat,
            unit,
            series: vec![MethodSeries::of(scan, vec![reading(2026, 10, 16, value)])],
        };
        let drawn = page(
            &[fat(Unit::Kilograms, 17.61), fat(Unit::Percentage, 21.05)],
            &NONE,
            &NO_PLAN,
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();

        assert_eq!(drawn.matches("<h2>").count(), 1, "{drawn}");
        assert!(drawn.contains("<h2>Fat</h2>"), "{drawn}");
        assert!(
            drawn.contains(r#"id="unit-fat-kilograms" class="kilograms" checked>"#),
            "kg first and chosen: {drawn}"
        );
        assert!(drawn.contains(r#"id="unit-fat-percentage" class="percentage">"#));
        assert!(drawn.contains("2026-10-16 · 17.61 kg ·"));
        assert!(drawn.contains("2026-10-16 · 21.1% ·"));
    }

    #[test]
    fn a_trend_is_a_line_and_its_last_reading_is_said() {
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
            series: vec![MethodSeries::of(
                Method::BodyScan {
                    algorithm: 218_300_672,
                },
                readings,
            )],
        };
        let drawn = page(
            &[fat_free],
            &NONE,
            &NO_PLAN,
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();

        assert_eq!(drawn.matches("<polyline").count(), 1, "one unbroken run");
        assert!(
            drawn.contains(
                "Body Scan, algorithm 218300672 · 2026-03-11: +0.00 kg against 28 days before, noise ±"
            ),
            "{drawn}"
        );
        assert!(
            drawn.contains(": rising</title>"),
            "the step up reads as rising"
        );
    }

    #[test]
    fn nothing_recorded_is_said_rather_than_drawn() {
        assert!(
            page(
                &[],
                &NONE,
                &NO_PLAN,
                Over::All,
                &crate::run::View::default()
            )
            .into_string()
            .contains("Nothing has been recorded.")
        );
    }

    #[test]
    fn a_range_shows_only_its_days_and_a_method_keeps_its_colour() {
        let fat = ComponentSeries {
            component: Component::Fat,
            unit: Unit::Percentage,
            series: vec![
                MethodSeries::of(Method::Spreadsheets, vec![reading(2018, 1, 1, 24.7)]),
                MethodSeries::of(
                    Method::BodyScan {
                        algorithm: 218_300_672,
                    },
                    vec![reading(2026, 10, 16, 21.1)],
                ),
            ],
        };
        let ranges = Ranges {
            mesocycle: Some(Range {
                from: jiff::civil::date(2026, 9, 28),
                to: jiff::civil::date(2026, 10, 19),
            }),
            macrocycle: None,
        };
        let drawn = page(
            &[fat],
            &NONE,
            &ranges,
            Over::Mesocycle,
            &crate::run::View::default(),
        )
        .into_string();

        assert!(!drawn.contains("2018-01-01 ·"), "before this mesocycle");
        assert!(!drawn.contains("spreadsheets ·"), "nor in the legend");
        assert_eq!(drawn.matches(r#"<circle class="point d2""#).count(), 2);
        assert!(drawn.contains(r#"href="/body-composition?over=all""#));
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
    fn each_mesocycle_is_a_point_on_its_phases_chart() {
        let measured: Vec<Measured> = jiff::civil::date(2026, 9, 7)
            .series(jiff::Span::new().days(1))
            .take(43)
            .zip(0_u64..)
            .map(|(on, day)| {
                let wobble = if day % 2 == 0 { 100 } else { 0 };
                let fat = 19_100 - day * 25 - wobble;
                Measured {
                    on,
                    method: Method::BodyScan {
                        algorithm: 218_300_672,
                    },
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
        let drawn = page(
            &[],
            &against,
            &NO_PLAN,
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();

        assert!(drawn.contains("<h2>Autumn 2026</h2>"), "{drawn}");
        assert!(drawn.contains("<h3>Cut</h3>"), "{drawn}");
        assert_eq!(drawn.matches(r#"<polygon class="phase""#).count(), 1);
        assert_eq!(
            drawn.matches(r#"<polygon class="goal""#).count(),
            1,
            "the goal is drawn whatever the phase"
        );
        assert!(drawn.contains(">cut</text>"), "{drawn}");
        assert!(drawn.contains(">goal</text>"), "{drawn}");
        assert!(!drawn.contains("Bulk"), "no bulk is due: {drawn}");
        assert!(drawn.contains(">14 Sep</text>"), "{drawn}");
        assert!(!drawn.contains("failure"), "no verdict");
        assert!(drawn.contains(">28 Sep, so far</text>"), "{drawn}");
        assert!(drawn.contains("Maintenance: no mesocycle yet"), "{drawn}");
        assert!(!drawn.contains(r#"<ul class="legend"><li><svg class="key" viewBox="0 0 12 12"><circle class="point d1" cx="6" cy="6" r="5">"#), "one method needs no legend");
    }
}
