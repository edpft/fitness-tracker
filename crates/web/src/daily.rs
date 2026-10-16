//! Overnight HRV and resting heart rate, as charts (#254).
//!
//! **Date across, the figure up, one point per day and one colour per
//! watch.** Nothing joins two points: a line would claim something about the
//! days between them, and a gap in the record is drawn as the empty stretch it
//! is (§ 37). Nothing joins two watches either, since each one's figures are
//! its own series (§ 6).
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath is the same figures.

use domain::{
    analytical::per_device::{DeviceSeries, SentBy},
    measure::{BeatsPerMinute, HeartRateVariability},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::chart::{along, years};

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 2.0;

/// The colours a watch's points are drawn in, in the order the watches first
/// appear; the days no watch can be placed on are rings in ink.
const COLOURS: [&str; 4] = ["d1", "d2", "d3", "d4"];

/// What a page plots: a whole number in a unit.
pub trait Figure: Copy {
    const UNIT: &'static str;
    fn number(self) -> u32;
}

impl Figure for BeatsPerMinute {
    const UNIT: &'static str = "bpm";
    fn number(self) -> u32 {
        self.as_u32()
    }
}

impl Figure for HeartRateVariability {
    const UNIT: &'static str = "ms";
    fn number(self) -> u32 {
        self.as_milliseconds()
    }
}

/// Which page this is.
#[derive(Debug, Clone, Copy)]
pub struct Shown {
    pub title: &'static str,
    /// What one point is, said under the title.
    pub point: &'static str,
}

pub const OVERNIGHT_HRV: Shown = Shown {
    title: "Overnight HRV",
    point: "Last night's average, on the morning Garmin reports it under.",
};

pub const RESTING_HEART_RATE: Shown = Shown {
    title: "Resting heart rate",
    point: "Garmin's resting heart rate for the day.",
};

/// The whole page.
pub fn page<T: Figure>(shown: Shown, series: &[DeviceSeries<T>]) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (shown.title) }
                style { (crate::page::STYLE) (crate::chart::STYLE) (STYLE) }
            }
            body {
                main.wide {
                    p { a href="/" { "Run next" } }
                    h1 { (shown.title) }
                    p.method { (shown.point) }
                    (chart(shown, series))
                }
            }
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(shown: Shown, error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (shown.title) }
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
    low: u32,
    high: u32,
}

impl Frame {
    fn x(&self, at: i64) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(along(at - self.first, self.last - self.first), LEFT)
    }

    fn y(&self, value: u32) -> f64 {
        (FLOOR - TOP).mul_add(
            -f64::from(value - self.low) / f64::from(self.high - self.low),
            FLOOR,
        )
    }
}

/// The chart, its legend and its table.
fn chart<T: Figure>(shown: Shown, series: &[DeviceSeries<T>]) -> Markup {
    let days = series.iter().flat_map(|one| one.readings.iter());
    let (Some(first), Some(last)) = (
        days.clone().map(|reading| reading.on).min(),
        days.clone().map(|reading| reading.on).max(),
    ) else {
        return html! { p { "Nothing has been recorded." } };
    };
    let low = days
        .clone()
        .map(|reading| reading.value.number())
        .min()
        .unwrap_or(0);
    let high = days
        .map(|reading| reading.value.number())
        .max()
        .unwrap_or(0);
    // Whole tens either side, so the gridlines fall on round figures.
    let frame = Frame {
        first: seconds(first),
        last: seconds(last),
        low: low / 10 * 10,
        high: (high / 10).saturating_add(1).saturating_mul(10),
    };
    html! {
        div.plot { (plot(shown, series, &frame)) }
        ul.legend {
            @for (index, one) in series.iter().enumerate() {
                li {
                    svg.key viewBox="0 0 12 12" { (mark(class(index, &one.sent_by), &one.sent_by, 6.0, 6.0, "")) }
                    (name(&one.sent_by)) " · " (one.readings.len()) " days"
                }
            }
        }
        (table(series))
    }
}

fn plot<T: Figure>(shown: Shown, series: &[DeviceSeries<T>], frame: &Frame) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label=(format!("{}, one point per day, by date", shown.title)) {
            @for value in (frame.low..=frame.high).step_by(10) {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(value)) y2=(frame.y(value)) {}
                text.axis x=(LEFT - 6.0) y=(frame.y(value) + 4.0) text-anchor="end" { (value) }
            }
            @for (year, at) in years(frame.first, frame.last) {
                line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (year) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
            @for (index, one) in series.iter().enumerate() {
                @for reading in &one.readings {
                    (mark(
                        class(index, &one.sent_by),
                        &one.sent_by,
                        frame.x(seconds(reading.on)),
                        frame.y(reading.value.number()),
                        &format!("{} · {}{} · {}", reading.on, reading.value.number(), T::UNIT, name(&one.sent_by)),
                    ))
                }
            }
        }
    }
}

/// The same figures, for whoever cannot tell two colours apart.
fn table<T: Figure>(series: &[DeviceSeries<T>]) -> Markup {
    let mut rows: Vec<(Date, u32, String)> = series
        .iter()
        .flat_map(|one| {
            one.readings
                .iter()
                .map(|reading| (reading.on, reading.value.number(), name(&one.sent_by)))
        })
        .collect();
    rows.sort_by_key(|(on, ..)| *on);
    html! {
        details {
            summary { "Every day as a table" }
            table.figures {
                thead { tr { th { "date" } th { (T::UNIT) } th { "device" } } }
                tbody {
                    @for (on, value, device) in rows {
                        tr { td { (on) } td { (value) } td { (device) } }
                    }
                }
            }
        }
    }
}

fn mark(class: &str, sent_by: &SentBy, x: f64, y: f64, title: &str) -> Markup {
    html! {
        @match sent_by {
            SentBy::Device(_) => circle class=(format!("point {class}")) cx=(x) cy=(y) r=(RADIUS) { title { (title) } },
            SentBy::Unknown => circle class="ring ink" cx=(x) cy=(y) r=(RADIUS + 0.5) { title { (title) } },
        }
    }
}

fn class(index: usize, sent_by: &SentBy) -> &'static str {
    match sent_by {
        SentBy::Device(_) => COLOURS.get(index % COLOURS.len()).copied().unwrap_or("ink"),
        SentBy::Unknown => "ink",
    }
}

fn name(sent_by: &SentBy) -> String {
    match sent_by {
        SentBy::Device(device) => device.name().to_string(),
        SentBy::Unknown => "device unknown".to_owned(),
    }
}

/// A day's place on the date axis: its start, in UTC, so every day is the
/// same width.
fn seconds(on: Date) -> i64 {
    on.to_zoned(jiff::tz::TimeZone::UTC)
        .map_or(0, |zoned| zoned.timestamp().as_second())
}

/// The watches' colours: the per-zone chart's palette, validated in both
/// modes, taken in an order that keeps neighbours apart.
const STYLE: &str = "
.d1 { fill: var(--z1); } .d2 { fill: var(--z6); } .d3 { fill: var(--z5); } .d4 { fill: var(--z2); }
.point.d1, .point.d2, .point.d3, .point.d4 { stroke: none; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::per_device::{DeviceSeries, Reading, SentBy},
        measure::BeatsPerMinute,
    };

    use super::{RESTING_HEART_RATE, chart};

    fn reading(day: i8, beats: u32) -> Reading<BeatsPerMinute> {
        Reading {
            on: jiff::civil::date(2024, 12, day),
            value: BeatsPerMinute::new(beats).expect("a rate"),
        }
    }

    #[test]
    fn a_day_is_a_point_and_nothing_joins_two() {
        let series = [DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: vec![reading(21, 63), reading(22, 53)],
        }];
        let drawn = chart(RESTING_HEART_RATE, &series).into_string();

        assert_eq!(drawn.matches("<circle class=\"ring ink\"").count(), 3);
        assert!(drawn.contains("2024-12-21 · 63bpm · device unknown"));
        assert!(!drawn.contains("<path") && !drawn.contains("<polyline"));
    }

    #[test]
    fn nothing_recorded_is_said_rather_than_drawn() {
        let series: [DeviceSeries<BeatsPerMinute>; 0] = [];
        assert!(
            chart(RESTING_HEART_RATE, &series)
                .into_string()
                .contains("Nothing has been recorded.")
        );
    }
}
