//! Overnight HRV and resting heart rate, as charts (#254).
//!
//! **Date across, the figure up, one point per day and one colour per
//! watch.** Nothing joins two points: a line would claim something about the
//! days between them, and a gap in the record is drawn as the empty stretch it
//! is (§ 37). Nothing joins two watches either, since each one's figures are
//! its own series (§ 6).
//!
//! **A trend is a line over the points** (#417): an estimate of a continuous
//! value, so it breaks where `domain` gives none rather than joining across.
//! HRV's is drawn over the band it is read against, shaded.
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath is the same figures.

use domain::{
    analytical::{
        hrv::{self, Against, Trended},
        per_device::{DeviceSeries, SentBy},
        resting,
    },
    measure::{BeatsPerMinute, HeartRateVariability},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::{along, years},
    run::View,
};

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

/// The trends a page draws, one for each device that has one.
pub struct Trends {
    pub overlays: Vec<Overlay>,
    /// What the line and the band are, said under the chart.
    pub method: String,
}

/// A trend drawn over one device's points, and what its last day reads.
pub struct Overlay {
    pub runs: Vec<Vec<Drawn>>,
    pub latest: Option<String>,
    /// Nights ringed, and listed beneath the chart.
    pub flags: Vec<Marked>,
}

/// A night ringed on the chart.
pub struct Marked {
    pub on: Date,
    pub value: u32,
    pub title: String,
}

/// One day of a trend.
pub struct Drawn {
    pub on: Date,
    pub value: f64,
    pub band: Option<(f64, f64)>,
    pub title: String,
}

/// The HRV page: each watch's nights and the trend through them.
pub fn overnight_hrv(trended: &[Trended], run: &View) -> Markup {
    let series: Vec<DeviceSeries<HeartRateVariability>> =
        trended.iter().map(|one| one.series.clone()).collect();
    let trends = Trends {
        overlays: trended.iter().map(hrv_overlay).collect(),
        method: format!(
            "points: each night · line: the {} · shaded: the band, {}",
            hrv::METHOD,
            hrv::BAND
        ),
    };
    page(OVERNIGHT_HRV, &series, Some(&trends), run)
}

fn hrv_overlay(trended: &Trended) -> Overlay {
    let runs: Vec<Vec<Drawn>> = trended
        .trend
        .iter()
        .map(|run| {
            run.iter()
                .map(|day| Drawn {
                    on: day.on,
                    value: day.milliseconds,
                    band: day.band.map(|band| (band.low, band.high)),
                    title: format!("{} · {}", day.on, hrv_reading(day)),
                })
                .collect()
        })
        .collect();
    let latest = trended.trend.iter().flatten().next_back().map(|day| {
        format!(
            "{}, {}: {}",
            name(&trended.series.sent_by),
            day.on,
            hrv_reading(day)
        )
    });
    Overlay {
        runs,
        latest,
        flags: Vec::new(),
    }
}

/// The resting heart rate page: each watch's days, the slow line through
/// them, and the nights flagged.
pub fn resting_heart_rate(trended: &[resting::Trended], run: &View) -> Markup {
    let series: Vec<DeviceSeries<BeatsPerMinute>> =
        trended.iter().map(|one| one.series.clone()).collect();
    let trends = Trends {
        overlays: trended.iter().map(resting_overlay).collect(),
        method: format!(
            "points: each day · line: the {} · ringed: flagged, {}",
            resting::METHOD,
            resting::FLAG
        ),
    };
    page(RESTING_HEART_RATE, &series, Some(&trends), run)
}

fn resting_overlay(trended: &resting::Trended) -> Overlay {
    let runs = trended
        .line
        .iter()
        .map(|run| {
            run.iter()
                .map(|day| Drawn {
                    on: day.on,
                    value: day.bpm,
                    band: None,
                    title: format!("{} · 90-day average {:.1} bpm", day.on, day.bpm),
                })
                .collect()
        })
        .collect();
    let flagged = |flag: &resting::Flag| {
        format!(
            "{} bpm against a normal of {:.0}",
            flag.rate.as_u32(),
            flag.normal
        )
    };
    let latest = trended.line.iter().flatten().next_back().map(|day| {
        let last_flag = trended.flags.last().map_or_else(
            || "never flagged".to_owned(),
            |flag| format!("last flagged {}, {}", flag.on, flagged(flag)),
        );
        format!(
            "{}, {}: 90-day average {:.1} bpm · {last_flag}",
            name(&trended.series.sent_by),
            day.on,
            day.bpm
        )
    });
    let flags = trended
        .flags
        .iter()
        .map(|flag| Marked {
            on: flag.on,
            value: flag.rate.as_u32(),
            title: format!(
                "{} · flagged · {} · {}",
                flag.on,
                flagged(flag),
                name(&trended.series.sent_by)
            ),
        })
        .collect();
    Overlay {
        runs,
        latest,
        flags,
    }
}

/// A day's trend, where it sits against the band, and the week's scatter.
fn hrv_reading(day: &hrv::Day) -> String {
    let against = match (day.band, day.against()) {
        (Some(band), Some(against)) => format!(
            "{} the band ({:.0}–{:.0} ms)",
            match against {
                Against::Below => "below",
                Against::Within => "within",
                Against::Above => "above",
            },
            band.low,
            band.high
        ),
        _ => "no band yet".to_owned(),
    };
    format!(
        "last 7 nights {:.0} ms, {against} · day-to-day variation {:.1}%",
        day.milliseconds, day.variation
    )
}

/// The whole page.
///
/// `trends` is drawn over `series` where the page has one.
pub fn page<T: Figure>(
    shown: Shown,
    series: &[DeviceSeries<T>],
    trends: Option<&Trends>,
    run: &View,
) -> Markup {
    let overlays = trends.map_or(&[][..], |trends| trends.overlays.as_slice());
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
                (crate::page::following(run))
                main.wide {
                    p { a href="/" { "Run next" } }
                    h1 { (shown.title) }
                    p.method { (shown.point) }
                    @for overlay in overlays {
                        @if let Some(latest) = &overlay.latest { p.reading { (latest) } }
                    }
                    (chart(shown, series, overlays))
                    @if let Some(trends) = trends { p.method { (trends.method) } }
                    @if overlays.iter().any(|overlay| !overlay.flags.is_empty()) {
                        details {
                            summary { "Every night flagged" }
                            ul.flags {
                                @for flag in overlays.iter().flat_map(|overlay| &overlay.flags) {
                                    li { (flag.title) }
                                }
                            }
                        }
                    }
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
        self.y_of(f64::from(value))
    }

    fn y_of(&self, value: f64) -> f64 {
        (FLOOR - TOP).mul_add(
            -(value - f64::from(self.low)) / f64::from(self.high - self.low),
            FLOOR,
        )
    }
}

/// The chart, its legend and its table.
fn chart<T: Figure>(shown: Shown, series: &[DeviceSeries<T>], overlays: &[Overlay]) -> Markup {
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
        div.plot { (plot(shown, series, overlays, &frame)) }
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

fn plot<T: Figure>(
    shown: Shown,
    series: &[DeviceSeries<T>],
    overlays: &[Overlay],
    frame: &Frame,
) -> Markup {
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
            @for overlay in overlays {
                @for stretch in overlay.runs.iter().flat_map(|run| banded(run)) {
                    polygon.band points=(band_points(stretch, frame)) {}
                }
            }
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
            @for overlay in overlays {
                @for flag in &overlay.flags {
                    circle.flag cx=(frame.x(seconds(flag.on))) cy=(frame.y(flag.value)) r=(RADIUS + 3.0) {
                        title { (flag.title) }
                    }
                }
                @for run in &overlay.runs {
                    polyline.trend points=(line_points(run, frame)) {}
                    @for day in run {
                        circle.hover cx=(frame.x(seconds(day.on))) cy=(frame.y_of(day.value)) r=(RADIUS) {
                            title { (day.title) }
                        }
                    }
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

/// The unbroken stretches of a run that have a band.
fn banded(run: &[Drawn]) -> Vec<&[Drawn]> {
    run.split(|day| day.band.is_none())
        .filter(|stretch| !stretch.is_empty())
        .collect()
}

/// A stretch of band as one polygon: along its top, and back along its foot.
fn band_points(stretch: &[Drawn], frame: &Frame) -> String {
    let top = stretch.iter().filter_map(|day| {
        day.band
            .map(|(_, high)| (frame.x(seconds(day.on)), frame.y_of(high)))
    });
    let foot = stretch.iter().rev().filter_map(|day| {
        day.band
            .map(|(low, _)| (frame.x(seconds(day.on)), frame.y_of(low)))
    });
    top.chain(foot)
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn line_points(run: &[Drawn], frame: &Frame) -> String {
    run.iter()
        .map(|day| {
            format!(
                "{:.1},{:.1}",
                frame.x(seconds(day.on)),
                frame.y_of(day.value)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
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
pub const STYLE: &str = "
.d1 { fill: var(--z1); } .d2 { fill: var(--z6); } .d3 { fill: var(--z5); } .d4 { fill: var(--z2); }
.point.d1, .point.d2, .point.d3, .point.d4 { stroke: none; }
.trend { fill: none; stroke: var(--ink); stroke-width: 2; stroke-linejoin: round; }
.band { fill: var(--ink); fill-opacity: 0.08; stroke: none; }
.flag { fill: none; stroke: var(--ink); stroke-width: 1.5; }
.hover { fill: transparent; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            hrv::Trended,
            per_device::{DeviceSeries, Reading, SentBy},
            resting,
        },
        measure::{BeatsPerMinute, HeartRateVariability},
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
        let drawn = chart(RESTING_HEART_RATE, &series, &[]).into_string();

        assert_eq!(drawn.matches("<circle class=\"ring ink\"").count(), 3);
        assert!(drawn.contains("2024-12-21 · 63bpm · device unknown"));
        assert!(!drawn.contains("<path") && !drawn.contains("<polyline"));
    }

    #[test]
    fn hrv_is_a_line_over_its_band_and_says_where_the_last_day_sits() {
        let nights = (0..70)
            .map(|index| Reading {
                on: jiff::civil::date(2026, 1, 1)
                    .checked_add(jiff::Span::new().days(index))
                    .expect("a day"),
                value: HeartRateVariability::from_milliseconds(if index % 2 == 0 {
                    50
                } else {
                    56
                })
                .expect("a night"),
            })
            .collect();
        let trended = [Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: nights,
        })];
        let page = super::overnight_hrv(&trended, &crate::run::View::default()).into_string();

        assert_eq!(page.matches(r#"class="trend""#).count(), 1);
        assert_eq!(page.matches(r#"class="band""#).count(), 1);
        assert!(page.contains("device unknown, 2026-03-11: last 7 nights 53 ms, within the band"));
        assert!(page.contains("at least 26 of them recorded"));
    }

    #[test]
    fn resting_heart_rate_is_a_line_and_a_ring_on_each_flagged_night() {
        let rates = (0..40)
            .map(|index| if index % 2 == 0 { 50 } else { 52 })
            .chain([57, 58]);
        let readings = rates
            .enumerate()
            .map(|(index, beats)| Reading {
                on: jiff::civil::date(2026, 1, 1)
                    .checked_add(jiff::Span::new().days(i64::try_from(index).expect("small")))
                    .expect("a day"),
                value: BeatsPerMinute::new(beats).expect("a rate"),
            })
            .collect();
        let trended = [resting::Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings,
        })];
        let page = super::resting_heart_rate(&trended, &crate::run::View::default()).into_string();

        assert_eq!(page.matches(r#"class="trend""#).count(), 1);
        assert_eq!(page.matches(r#"class="flag""#).count(), 1);
        assert!(page.contains("last flagged 2026-02-11, 58 bpm against a normal of 51"));
    }

    #[test]
    fn nothing_recorded_is_said_rather_than_drawn() {
        let series: [DeviceSeries<BeatsPerMinute>; 0] = [];
        assert!(
            chart(RESTING_HEART_RATE, &series, &[])
                .into_string()
                .contains("Nothing has been recorded.")
        );
    }
}
