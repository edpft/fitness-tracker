//! Body composition, as charts (#256).
//!
//! **One chart per component, date across, the figure up, one colour per
//! method.** Nothing joins two points, so a gap in the record is the empty
//! stretch it is (§ 37), and nothing joins two methods, since each one's
//! figures are its own series (§ 6). A method keeps its colour on every chart.
//!
//! **The trend is a line over each method's points** (#419), broken where the
//! record is too thin for one, and each method's last reading against 28 days
//! before is said above its chart.
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath each chart is the same figures.

use domain::{
    analytical::composition::{
        AGAINST_DAYS, Change, ComponentSeries, Direction, Method, TREND, Trend, Unit,
    },
    body::Component,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::{along, years},
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

/// The whole page.
pub fn page(components: &[ComponentSeries], run: &View) -> Markup {
    let methods = methods(components);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (crate::chart::STYLE) (crate::daily::STYLE) }
            }
            body {
                (crate::page::following(run))
                main.wide {
                    p { a href="/" { "Run next" } }
                    h1 { (TITLE) }
                    p.method {
                        "points: each weigh-in, coloured by what measured it · line: trend, the " (TREND)
                        " · against " (AGAINST_DAYS) " days before: rising or falling beyond 1.96 × the method's own scatter between consecutive days, maintaining within it"
                    }
                    @if components.is_empty() {
                        p { "Nothing has been recorded." }
                    }
                    @for component in components {
                        (chart(component, &methods))
                    }
                }
            }
        }
    }
}

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
            body { main { p.failed { "the store could not be read: " (error) } } }
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

/// One component's chart, its legend and its table.
fn chart(component: &ComponentSeries, methods: &[Method]) -> Markup {
    let readings = component.series.iter().flat_map(|one| one.readings.iter());
    let (Some(first), Some(last)) = (
        readings.clone().map(|reading| reading.on).min(),
        readings.clone().map(|reading| reading.on).max(),
    ) else {
        return html! {};
    };
    let low = readings
        .clone()
        .map(|reading| reading.value)
        .fold(f64::INFINITY, f64::min);
    let high = readings
        .map(|reading| reading.value)
        .fold(f64::NEG_INFINITY, f64::max);
    let (low, high, step) = value_axis(low, high);
    let frame = Frame {
        first: seconds(first),
        last: seconds(last),
        low,
        high,
        step,
    };
    let name = heading(component.component);
    let unit = component.unit;
    html! {
        h2 { (name) " (" (unit.as_str()) ")" }
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
                @for (year, at) in labelled_years(first, frame.first, frame.last) {
                    line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
                    text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (year) }
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
            @for one in &component.series {
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

/// Each new year inside the span, or the first one at the left edge where the
/// span holds no new year, so every chart says when it is.
fn labelled_years(first: Date, from: i64, to: i64) -> Vec<(i16, i64)> {
    let years = years(from, to);
    if years.is_empty() {
        vec![(first.year(), from)]
    } else {
        years
    }
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

/// A day's place on the date axis: its start, in UTC, so every day is the
/// same width.
fn seconds(on: Date) -> i64 {
    on.to_zoned(jiff::tz::TimeZone::UTC)
        .map_or(0, |zoned| zoned.timestamp().as_second())
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::composition::{ComponentSeries, Method, MethodSeries, Reading, Unit},
        body::Component,
    };

    use super::page;

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
        let drawn = page(&[fat], &crate::run::View::default()).into_string();

        assert!(drawn.contains("<h2>Fat (%)</h2>"));
        assert_eq!(drawn.matches(r#"<circle class="point d1""#).count(), 3);
        assert_eq!(drawn.matches(r#"<circle class="point d2""#).count(), 2);
        assert!(drawn.contains("2018-01-01 · 24.7% · spreadsheets"));
        assert!(drawn.contains("2026-10-16 · 21.1% · Body Scan, algorithm 218300672"));
        assert!(!drawn.contains("<polyline") && !drawn.contains("<path"));
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
        let muscle = ComponentSeries {
            component: Component::Muscle,
            unit: Unit::Kilograms,
            series: vec![MethodSeries::of(
                Method::BodyScan {
                    algorithm: 218_300_672,
                },
                readings,
            )],
        };
        let drawn = page(&[muscle], &crate::run::View::default()).into_string();

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
            page(&[], &crate::run::View::default())
                .into_string()
                .contains("Nothing has been recorded.")
        );
    }
}
