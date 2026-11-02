//! Overnight HRV, as a chart (#254), until it moves to the detail template
//! (#622).
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
        range::{Over, Range, Ranges},
    },
    measure::HeartRateVariability,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::along,
    range::{seconds, ticks},
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
    /// Where the page is served.
    pub path: &'static str,
    /// What one point is, said under the title.
    pub point: &'static str,
}

pub const OVERNIGHT_HRV: Shown = Shown {
    title: "Overnight HRV",
    path: "/hrv",
    point: "Last night's average, on the morning Garmin reports it under.",
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
}

/// One day of a trend.
pub struct Drawn {
    pub on: Date,
    pub value: f64,
    pub band: Option<(f64, f64)>,
    pub title: String,
}

/// The HRV page: each watch's nights and the trend through them, over the
/// range chosen.
pub fn overnight_hrv(trended: &[Trended], ranges: &Ranges, over: Over, run: &View) -> Markup {
    let recorded = recorded(trended.iter().map(|one| &one.series));
    let range = crate::range::chosen(ranges, over, recorded, NOTHING);
    let trended: Vec<Trended> = range.as_ref().map_or_else(
        |_| Vec::new(),
        |range| trended.iter().map(|one| one.within(*range)).collect(),
    );
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
    page(OVERNIGHT_HRV, &series, Some(&trends), over, range, run)
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
    Overlay { runs, latest }
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

pub const NOTHING: &str = "Nothing has been recorded.";

/// Every day any of the series has a figure on.
pub fn recorded<'a, T: 'a>(series: impl Iterator<Item = &'a DeviceSeries<T>>) -> Option<Range> {
    Range::spanning(series.flat_map(|one| one.readings.iter().map(|reading| reading.on)))
}

/// The whole page, over `range` or saying why there is none.
///
/// `trends` is drawn over `series` where the page has one.
fn page<T: Figure>(
    shown: Shown,
    series: &[DeviceSeries<T>],
    trends: Option<&Trends>,
    over: Over,
    range: Result<Range, Markup>,
    run: &View,
) -> Markup {
    let overlays = trends.map_or(&[][..], |trends| trends.overlays.as_slice());
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (crate::page::head(shown.title, &[crate::chart::STYLE, STYLE]))
            }
            body {
                (crate::page::following(run))
                main.wide {
                    (crate::page::menu(shown.path))
                    h1 { (shown.title) }
                    p.method { (shown.point) }
                    (crate::range::choices(shown.path, over))
                    @match range {
                        Err(said) => (said),
                        Ok(range) => {
                            @for overlay in overlays {
                                @if let Some(latest) = &overlay.latest { p.reading { (latest) } }
                            }
                            (chart(shown, series, overlays, range))
                            @if let Some(trends) = trends { p.method { (trends.method) } }
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
                (crate::page::head(shown.title, &[]))
            }
            body {
                main {
                    (crate::page::menu(shown.path))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
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

/// The chart over `range`, its legend and its table.
///
/// **A watch with nothing in the range keeps its place**, so it keeps its
/// colour, and is left out of the legend.
fn chart<T: Figure>(
    shown: Shown,
    series: &[DeviceSeries<T>],
    overlays: &[Overlay],
    range: Range,
) -> Markup {
    let days = series.iter().flat_map(|one| one.readings.iter());
    if days.clone().next().is_none() {
        return html! { p { "Nothing recorded in this range." } };
    }
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
        range,
        first: seconds(range.from),
        last: seconds(range.to),
        low: low / 10 * 10,
        high: (high / 10).saturating_add(1).saturating_mul(10),
    };
    html! {
        div.plot { (plot(shown, series, overlays, &frame)) }
        ul.legend {
            @for (index, one) in series.iter().enumerate().filter(|(_, one)| !one.readings.is_empty()) {
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
            @for (tick, label) in ticks(frame.range) {
                @let at = seconds(tick);
                line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (label) }
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

pub fn mark(class: &str, sent_by: &SentBy, x: f64, y: f64, title: &str) -> Markup {
    html! {
        @match sent_by {
            SentBy::Device(_) => circle class=(format!("point {class}")) cx=(x) cy=(y) r=(RADIUS) { title { (title) } },
            SentBy::Unknown => circle class="ring ink" cx=(x) cy=(y) r=(RADIUS + 0.5) { title { (title) } },
        }
    }
}

pub fn class(index: usize, sent_by: &SentBy) -> &'static str {
    match sent_by {
        SentBy::Device(_) => COLOURS.get(index % COLOURS.len()).copied().unwrap_or("ink"),
        SentBy::Unknown => "ink",
    }
}

pub fn name(sent_by: &SentBy) -> String {
    match sent_by {
        SentBy::Device(device) => device.name().to_string(),
        SentBy::Unknown => "device unknown".to_owned(),
    }
}

/// The watches' colours: the per-zone chart's palette, validated in both
/// modes, taken in an order that keeps neighbours apart.
pub const STYLE: &str = "
.d1 { fill: var(--z1); } .d2 { fill: var(--z6); } .d3 { fill: var(--z5); } .d4 { fill: var(--z2); }
.point.d1, .point.d2, .point.d3, .point.d4 { stroke: none; }
.trend { fill: none; stroke: var(--ink); stroke-width: 2; stroke-linejoin: round; }
.band { fill: var(--ink); fill-opacity: 0.08; stroke: none; }
.hover { fill: transparent; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            hrv::Trended,
            per_device::{DeviceSeries, Reading, SentBy},
            range::{Over, Range, Ranges},
        },
        measure::HeartRateVariability,
    };

    use super::{OVERNIGHT_HRV, chart};

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn december() -> Range {
        Range {
            from: jiff::civil::date(2024, 12, 1),
            to: jiff::civil::date(2024, 12, 31),
        }
    }

    fn reading(day: i8, milliseconds: u32) -> Reading<HeartRateVariability> {
        Reading {
            on: jiff::civil::date(2024, 12, day),
            value: HeartRateVariability::from_milliseconds(milliseconds).expect("a night"),
        }
    }

    #[test]
    fn a_day_is_a_point_and_nothing_joins_two() {
        let series = [DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: vec![reading(21, 63), reading(22, 53)],
        }];
        let drawn = chart(OVERNIGHT_HRV, &series, &[], december()).into_string();

        assert_eq!(drawn.matches("<circle class=\"ring ink\"").count(), 3);
        assert!(drawn.contains("2024-12-21 · 63ms · device unknown"));
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
        let page =
            super::overnight_hrv(&trended, &NO_PLAN, Over::All, &crate::run::View::default())
                .into_string();

        assert_eq!(page.matches(r#"class="trend""#).count(), 1);
        assert_eq!(page.matches(r#"class="band""#).count(), 1);
        assert!(page.contains("device unknown, 2026-03-11: last 7 nights 53 ms, within the band"));
        assert!(page.contains("at least 26 of them recorded"));
    }
}
