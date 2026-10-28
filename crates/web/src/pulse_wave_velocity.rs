//! Pulse wave velocity, as a chart (#471).
//!
//! **Date across, one point per reading, and the slow line over them.** The
//! line is the whole record's, cut at the range's edges, and breaks where
//! `domain` gives none; nothing joins two points, so a gap in the record is the
//! empty stretch it is (§ 37). Nothing joins two algorithms either (§ 6).
//!
//! **No direction, no vascular age and no bands for age** (operator,
//! 2026-10-28): see [`domain::analytical::pulse_wave_velocity`].

use domain::analytical::{
    pulse_wave_velocity::{Day, METHOD, Series},
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::along,
    range::{seconds, ticks},
};

pub const PATH: &str = "/pulse-wave-velocity";
const TITLE: &str = "Pulse wave velocity";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 2.0;

/// The whole page.
pub fn page(series: &[Series], ranges: &Ranges, over: Over, run: &crate::run::View) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (STYLE) }
            }
            body {
                (crate::page::following(run))
                main.wide {
                    (crate::page::menu(PATH))
                    h1 { (TITLE) }
                    (crate::range::choices(PATH, over))
                    (body(series, ranges, over))
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
            body {
                main {
                    (crate::page::menu(PATH))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// The range chosen, or why there is none.
fn body(series: &[Series], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(
        series
            .iter()
            .flat_map(|one| one.readings.iter().map(|reading| reading.on)),
    );
    let range = match crate::range::chosen(
        ranges,
        over,
        recorded,
        "No pulse wave velocity has been recorded.",
    ) {
        Ok(range) => range,
        Err(said) => return said,
    };
    let series: Vec<Series> = series.iter().map(|one| one.within(range)).collect();
    let readings = series.iter().flat_map(|one| one.readings.iter());
    let (Some(low), Some(high)) = (
        readings
            .clone()
            .map(|reading| reading.value.as_millimetres_per_second())
            .min(),
        readings
            .map(|reading| reading.value.as_millimetres_per_second())
            .max(),
    ) else {
        return html! { p { "No reading in this range." } };
    };
    let step = step(high - low);
    let frame = Frame {
        range,
        first: seconds(range.from),
        last: seconds(range.to),
        low: low / step * step,
        high: (high / step + 1) * step,
    };
    html! {
        @for one in &series {
            @if let Some(day) = one.line.iter().flatten().next_back() {
                p.reading { (day.on) ": 90-day average " (metres(day.metres_per_second)) (algorithm(&series, one)) }
            }
        }
        div.plot { (plot(&series, &frame, step)) }
        p.legend { "points: each reading · line: the " (METHOD) }
        (table(&series))
    }
}

/// Where a day and a figure fall on the chart.
struct Frame {
    range: Range,
    first: i64,
    last: i64,
    low: u32,
    high: u32,
}

impl Frame {
    fn x(&self, on: Date) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(
            along(seconds(on) - self.first, self.last - self.first),
            LEFT,
        )
    }

    /// Where a figure in millimetres per second falls.
    fn y(&self, millimetres: f64) -> f64 {
        (FLOOR - TOP).mul_add(
            -(millimetres - f64::from(self.low)) / f64::from(self.high - self.low),
            FLOOR,
        )
    }
}

fn plot(series: &[Series], frame: &Frame, step: u32) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Pulse wave velocity, one point per reading, by date" {
            @for value in (frame.low..=frame.high).step_by(step as usize) {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(f64::from(value))) y2=(frame.y(f64::from(value))) {}
                text.axis x=(LEFT - 6.0) y=(frame.y(f64::from(value)) + 4.0) text-anchor="end" { (tick(value)) }
            }
            @for (tick, label) in ticks(frame.range) {
                line.grid x1=(frame.x(tick)) x2=(frame.x(tick)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(tick) + 3.0) y=(FLOOR + 14.0) { (label) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
            @for one in series {
                @for reading in &one.readings {
                    @let millimetres = reading.value.as_millimetres_per_second();
                    circle.point cx=(frame.x(reading.on)) cy=(frame.y(f64::from(millimetres))) r=(RADIUS) {
                        title { (reading.on) " · " (metres(f64::from(millimetres) / 1_000.0)) (algorithm(series, one)) }
                    }
                }
            }
            @for one in series {
                @for run in &one.line {
                    polyline.trend points=(points(run, frame)) {}
                }
            }
        }
    }
}

fn points(run: &[Day], frame: &Frame) -> String {
    run.iter()
        .map(|day| {
            format!(
                "{:.1},{:.1}",
                frame.x(day.on),
                frame.y(day.metres_per_second * 1_000.0)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The same figures as a table.
fn table(series: &[Series]) -> Markup {
    let mut rows: Vec<(Date, u32, u64)> = series
        .iter()
        .flat_map(|one| {
            one.readings.iter().map(|reading| {
                (
                    reading.on,
                    reading.value.as_millimetres_per_second(),
                    one.algorithm,
                )
            })
        })
        .collect();
    rows.sort_by_key(|(on, ..)| *on);
    html! {
        details {
            summary { "Every reading as a table" }
            table.figures {
                thead { tr { th { "date" } th { "m/s" } th { "algorithm" } } }
                tbody {
                    @for (on, millimetres, algorithm) in rows {
                        tr { td { (on) } td { (format!("{:.2}", f64::from(millimetres) / 1_000.0)) } td { (algorithm) } }
                    }
                }
            }
        }
    }
}

/// `7.39 m/s`.
fn metres(value: f64) -> String {
    format!("{value:.2} m/s")
}

/// The algorithm, named only where the page holds more than one.
fn algorithm(series: &[Series], one: &Series) -> String {
    if series.len() > 1 {
        format!(" · algorithm {}", one.algorithm)
    } else {
        String::new()
    }
}

/// A gridline's label, in metres per second.
fn tick(millimetres: u32) -> String {
    format!("{:.1}", f64::from(millimetres) / 1_000.0)
}

/// How often the axis is marked, in millimetres per second: every 0.1 m/s
/// across less than 0.6, every 0.2 across less than 1.5, and every 0.5 beyond.
const fn step(spread: u32) -> u32 {
    if spread < 600 {
        100
    } else if spread < 1_500 {
        200
    } else {
        500
    }
}

const STYLE: &str = "
main.wide { max-width: 60rem; }
:root { --surface: #fcfcfb; --ink: #0b0b0b; --muted: #898781; --grid: #e1e0d9; --axis: #c3c2b7; --mark: #2a78d6; }
@media (prefers-color-scheme: dark) { :root {
  --surface: #1a1a19; --ink: #ffffff; --grid: #2c2c2a; --axis: #383835; --mark: #3987e5;
} }
body { background: var(--surface); color: var(--ink); }
h1 { font-size: 1.25rem; }
div.plot { overflow-x: auto; }
svg.chart { width: 100%; min-width: 600px; height: auto; display: block; }
.grid { stroke: var(--grid); stroke-width: 1; }
.baseline { stroke: var(--axis); stroke-width: 1; }
.axis { fill: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
.point { fill: var(--mark); opacity: 0.6; }
.trend { fill: none; stroke: var(--ink); stroke-width: 2; stroke-linejoin: round; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
p.legend { color: var(--muted); font-size: 0.9rem; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
table.figures td { padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            pulse_wave_velocity::{Measured, Series, by_algorithm},
            range::{Over, Range, Ranges},
        },
        body::PulseWaveVelocity,
    };
    use jiff::civil::date;

    use super::page;

    /// A reading a day from 15 January to 28 October, 7.0 and 7.4 m/s in
    /// turn, with nothing from 26 July to 1 August.
    fn record() -> Vec<Series> {
        let measured: Vec<Measured> = date(2026, 1, 15)
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on <= date(2026, 10, 28))
            .filter(|on| !(date(2026, 7, 26)..=date(2026, 8, 1)).contains(on))
            .enumerate()
            .map(|(index, on)| Measured {
                on,
                algorithm: 0,
                velocity: PulseWaveVelocity::from_millimetres_per_second(if index % 2 == 0 {
                    7_000
                } else {
                    7_400
                }),
            })
            .collect();
        by_algorithm(&measured)
    }

    fn ranges() -> Ranges {
        let today = date(2026, 10, 28);
        Ranges {
            mesocycle: Some(Range {
                from: date(2026, 10, 19),
                to: today,
            }),
            macrocycle: Some(Range {
                from: date(2026, 5, 4),
                to: today,
            }),
        }
    }

    fn shown(series: &[Series], over: Over) -> String {
        page(series, &ranges(), over, &crate::run::View::default()).into_string()
    }

    #[test]
    fn each_reading_is_a_point_under_one_slow_line() {
        let page = shown(&record(), Over::All);
        assert_eq!(page.matches(r#"class="point""#).count(), 280);
        assert_eq!(
            page.matches("<polyline").count(),
            1,
            "a week's gap does not break it"
        );
        assert!(
            page.contains("2026-10-28: 90-day average 7.20 m/s"),
            "{page}"
        );
        assert!(page.contains("2026-10-28 · 7.40 m/s<") || page.contains("2026-10-28 · 7.00 m/s<"));
        assert!(!page.contains("algorithm 0"), "one algorithm is not named");
        assert!(!page.to_lowercase().contains("vascular age"));
        assert!(!page.contains("rising") && !page.contains("falling"));
    }

    #[test]
    fn a_range_shows_its_readings_and_the_whole_record_s_line() {
        let page = shown(&record(), Over::Mesocycle);
        assert_eq!(page.matches(r#"class="point""#).count(), 10);
        assert_eq!(page.matches("<polyline").count(), 1);
    }

    #[test]
    fn no_readings_is_said_rather_than_drawn() {
        let page = shown(&[], Over::All);
        assert!(page.contains("No pulse wave velocity has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }
}
