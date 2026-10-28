//! The FTP tests, in watts and over body weight, as a chart (#469).
//!
//! **Date across, one point per test, nothing joining them.** A test is a
//! measurement, so each says which way it moved from the test before and no
//! line claims anything about the days between.
//!
//! **One chart, the unit a choice**: watts, or watts a kilo where the test was
//! weighed. A test with no weigh-in that day is named beneath the per-kilo
//! chart rather than drawn.
//!
//! **The range only chooses which tests are shown**: each one's reading is
//! against the test before it, whether or not that one is in the range.

use domain::analytical::{
    body_weight::Direction,
    ftp_test::{Moved, PowerToWeight, Test, within},
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

pub const PATH: &str = "/ftp";
const TITLE: &str = "FTP tests";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 64.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 4.5;
/// How far inside the chart's edges the first and last days fall, so a test on
/// either is drawn whole.
const INSET: f64 = 12.0;
/// The width the days are spread across.
const ACROSS: f64 = WIDTH - LEFT - RIGHT - INSET - INSET;

/// The whole page.
pub fn page(tests: &[Test], ranges: &Ranges, over: Over, run: &crate::run::View) -> Markup {
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
                    (body(tests, ranges, over))
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
fn body(tests: &[Test], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(tests.iter().map(Test::on));
    let range = match crate::range::chosen(ranges, over, recorded, "No FTP test has been recorded.")
    {
        Ok(range) => range,
        Err(said) => return said,
    };
    let tests = within(tests, range);
    let Some(last) = tests.last() else {
        return html! { p { "No FTP test in this range." } };
    };
    let unweighed: Vec<Date> = tests
        .iter()
        .filter(|test| test.per_kilo.is_none())
        .map(Test::on)
        .collect();
    html! {
        p.reading { (last.on()) ": " (watts(last)) @if last.per_kilo.is_some() { " · " (per_kilo(last)) } }
        p.legend { "points: each test's FTP · against the test before it, by any difference · per kilo over that day's weigh-in, against the test before only where both were weighed" }
        div.units {
            input type="radio" name="unit" id="unit-watts" class="watts" checked;
            label for="unit-watts" { "W" }
            input type="radio" name="unit" id="unit-per-kilo" class="per-kilo";
            label for="unit-per-kilo" { "W/kg" }
            div.unit.watts {
                (chart(&tests, range, Unit::Watts))
            }
            div."unit"."per-kilo" {
                (chart(&tests, range, Unit::PerKilo))
                @if !unweighed.is_empty() {
                    p.legend {
                        "Not weighed that day: "
                        @for (index, on) in unweighed.iter().enumerate() {
                            @if index > 0 { ", " }
                            (on)
                        }
                    }
                }
            }
        }
        (table(&tests))
    }
}

#[derive(Debug, Clone, Copy)]
enum Unit {
    Watts,
    PerKilo,
}

impl Unit {
    /// The test's figure, in watts or hundredths of a watt a kilo.
    fn of(self, test: &Test) -> Option<u32> {
        match self {
            Self::Watts => Some(test.ftp.watts().as_u32()),
            Self::PerKilo => test.per_kilo.map(PowerToWeight::as_hundredths),
        }
    }

    fn label(self, value: u32) -> String {
        match self {
            Self::Watts => format!("{value}W"),
            Self::PerKilo => format!("{}.{:02}", value / 100, value % 100),
        }
    }

    fn reading(self, test: &Test) -> String {
        match self {
            Self::Watts => watts(test),
            Self::PerKilo => per_kilo(test),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Watts => "FTP in watts by date",
            Self::PerKilo => "FTP in watts a kilo by date",
        }
    }
}

/// Where a day and a figure fall on the chart.
struct Frame {
    range: Range,
    days: i32,
    low: u32,
    high: u32,
}

impl Frame {
    fn x(&self, on: Date) -> f64 {
        let along = if self.days <= 0 {
            0.5
        } else {
            f64::from(days_between(self.range.from, on)) / f64::from(self.days)
        };
        ACROSS.mul_add(along, LEFT + INSET)
    }

    fn y(&self, value: u32) -> f64 {
        let share = f64::from(value.saturating_sub(self.low)) / f64::from(self.high - self.low);
        (FLOOR - TOP).mul_add(-share, FLOOR)
    }
}

fn chart(tests: &[Test], range: Range, unit: Unit) -> Markup {
    let values: Vec<u32> = tests.iter().filter_map(|test| unit.of(test)).collect();
    let (Some(&min), Some(&max)) = (values.iter().min(), values.iter().max()) else {
        return html! { p { "No test in this range was weighed that day." } };
    };
    let step = step(max - min);
    let frame = Frame {
        range,
        days: days_between(range.from, range.to),
        low: min.saturating_sub(step / 2) / step * step,
        high: (max + step / 2) / step * step + step,
    };
    html! {
        div.plot {
            svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img" aria-label=(unit.name()) {
                @for value in (frame.low..=frame.high).step_by(step as usize) {
                    line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.axis x=(LEFT - 6.0) y=(frame.y(value) + 4.0) text-anchor="end" { (unit.label(value)) }
                }
                @for (tick, label) in crate::range::ticks(range) {
                    line.grid x1=(frame.x(tick)) x2=(frame.x(tick)) y1=(TOP) y2=(FLOOR) {}
                    text.axis x=(frame.x(tick) + 3.0) y=(FLOOR + 14.0) { (label) }
                }
                line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
                @for test in tests {
                    @if let Some(value) = unit.of(test) {
                        circle.point cx=(frame.x(test.on())) cy=(frame.y(value)) r=(RADIUS) {
                            title { (test.on()) " · " (unit.reading(test)) }
                        }
                    }
                }
            }
        }
    }
}

/// The FTP and how it moved: `182W, +5W, rising`.
fn watts(test: &Test) -> String {
    let moved = test
        .watts_moved
        .map(|moved| format!(", {}W, {}", signed(moved.by), direction(moved)))
        .unwrap_or_default();
    format!("{}{moved}", test.ftp.watts())
}

/// Per kilo and how it moved: `2.16W/kg, +0.05W/kg, rising`.
fn per_kilo(test: &Test) -> String {
    let Some(ratio) = test.per_kilo else {
        return "not weighed that day".to_owned();
    };
    let moved = test
        .per_kilo_moved
        .map(|moved| format!(", {}W/kg, {}", hundredths(moved.by), direction(moved)))
        .unwrap_or_default();
    format!("{ratio}{moved}")
}

const fn direction(moved: Moved) -> &'static str {
    match moved.direction {
        Direction::Rising => "rising",
        Direction::Maintaining => "maintaining",
        Direction::Falling => "falling",
    }
}

const fn sign(by: i64) -> &'static str {
    match by.signum() {
        1 => "+",
        -1 => "−",
        _ => "",
    }
}

fn signed(by: i64) -> String {
    format!("{}{}", sign(by), by.unsigned_abs())
}

fn hundredths(by: i64) -> String {
    let abs = by.unsigned_abs();
    format!("{}{}.{:02}", sign(by), abs / 100, abs % 100)
}

/// The same figures as a table.
fn table(tests: &[Test]) -> Markup {
    html! {
        details {
            summary { "Every test as a table" }
            table.figures {
                thead { tr { th { "date" } th { "FTP" } th { "weigh-in" } th { "per kilo" } } }
                tbody {
                    @for test in tests {
                        tr {
                            td { (test.on()) }
                            td { (watts(test)) }
                            td { @if let Some(body) = test.body { (body) "kg" } }
                            td { (per_kilo(test)) }
                        }
                    }
                }
            }
        }
    }
}

/// How often the axis is marked across a spread, in watts or hundredths of a
/// watt a kilo alike: every 10, or every 20 across more than 60.
const fn step(spread: u32) -> u32 {
    if spread > 60 { 20 } else { 10 }
}

fn days_between(from: Date, to: Date) -> i32 {
    from.until(to).map_or(0, |span| span.get_days())
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
.point { fill: var(--mark); }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
p.legend { color: var(--muted); font-size: 0.9rem; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
table.figures td { padding-right: 0.75rem; }
div.units label { margin: 0 0.75rem 0 0.25rem; }
div.units input.watts:checked ~ div.unit:not(.watts),
div.units input.per-kilo:checked ~ div.unit:not(.per-kilo) { display: none; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            Weighed,
            ftp_test::{Test, tests},
            range::{Over, Range, Ranges},
        },
        canonical::Occurred,
        cycling::{Ftp, FtpProvenance, Watts},
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::civil::{Date, date};

    use super::page;

    type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

    fn tested(on: Date, watts: u32) -> Fallible<(Ftp, StartedAt)> {
        let zone = OperatorZone::try_from("Europe/London".to_owned())?;
        let instant = on.at(8, 0, 0, 0).to_zoned(zone.as_time_zone())?.timestamp();
        Ok((
            Ftp::new(Watts::from_u32(watts), on, FtpProvenance::Estimated)?,
            StartedAt::new(instant, zone),
        ))
    }

    const fn weighed(on: Date, grams: u64) -> Weighed {
        Weighed {
            at: Occurred::On(on),
            mass: Kg::from_grams(grams),
            fat_free: None,
        }
    }

    fn autumn() -> Fallible<Vec<Test>> {
        Ok(tests(
            &[
                tested(date(2026, 5, 10), 155)?,
                tested(date(2026, 7, 22), 172)?,
                tested(date(2026, 9, 23), 177)?,
                tested(date(2026, 10, 25), 182)?,
            ],
            &[
                weighed(date(2026, 7, 22), 84_362),
                weighed(date(2026, 9, 23), 83_877),
                weighed(date(2026, 10, 25), 84_191),
            ],
        ))
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

    fn shown(tests: &[Test], over: Over) -> String {
        page(tests, &ranges(), over, &crate::run::View::default()).into_string()
    }

    #[test]
    fn each_test_is_a_point_in_each_unit_it_has() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert_eq!(
            page.matches(r#"class="point""#).count(),
            7,
            "four in watts, three weighed"
        );
        assert!(!page.contains("<polyline"), "nothing joins two tests");
        assert!(page.contains("2026-10-25: 182W, +5W, rising · 2.16W/kg, +0.05W/kg, rising"));
        assert!(page.contains("Not weighed that day: 2026-05-10"));
    }

    #[test]
    fn per_kilo_has_no_direction_after_an_unweighed_test() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert!(
            page.contains("2026-07-22 · 2.03W/kg<"),
            "10 May was not weighed: {page}"
        );
        assert!(page.contains("2026-07-22 · 172W, +17W, rising<"));
    }

    #[test]
    fn a_range_keeps_each_test_s_reading_against_the_one_before() {
        let page = shown(&autumn().expect("the tests"), Over::Mesocycle);
        assert_eq!(page.matches(r#"class="point""#).count(), 2);
        assert!(page.contains("2026-10-25 · 182W, +5W, rising"));
    }

    #[test]
    fn no_tests_is_said_rather_than_drawn() {
        let page = page(&[], &ranges(), Over::All, &crate::run::View::default()).into_string();
        assert!(page.contains("No FTP test has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }
}
