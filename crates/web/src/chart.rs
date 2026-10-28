//! Heart rate per power zone, as a chart (#152).
//!
//! **Date across, heart rate up, the zone as colour and the device as shape**
//! (operator, 2026-10-14). A ride is one point per zone it held, so a column
//! of points is one ride. Nothing joins two points: a device's points are its
//! own series, and a line drawn between two shapes would stitch them (§ 6).
//!
//! **A line through each device's rides in each zone, one per FTP period**
//! (#415), in the zone's colour, and the class's Z3 intervals beneath as a
//! chart of their own with each line's band. Each line says which way it goes.
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each mark's `title` and
//! the table beneath is the same figures for whoever cannot tell two colours
//! apart.

use std::{collections::BTreeSet, fmt::Write as _};

use domain::{
    analytical::{
        body_weight::Direction,
        heart_rate::{ByZone, INTERVALS_METHOD, METHOD, RideHeartRate, SentBy, ZoneHeartRate},
        heart_rate_trend::{CONFIDENCE, DeviceTrends, Period, TREND, Trended, trends},
        range::{Over, Range, Ranges},
    },
    cycling::{PowerZone, clock},
    device::DeviceIdentifier,
    measure::PositiveDuration,
};
use maud::{DOCTYPE, Markup, html};

use crate::run::View;

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 400.0;
const LEFT: f64 = 92.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
/// The plot's floor, above the two rows of rides that have no points.
const FLOOR: f64 = 320.0;
const NO_HEART_RATE_ROW: f64 = 352.0;
const NO_POINT_ROW: f64 = 376.0;
/// The height of a chart with no rows beneath its floor.
const FLOOR_ONLY: f64 = 340.0;
const RADIUS: f64 = 4.5;
/// A square's side, a little inside the circle's diameter so the two read as one size.
const SIDE: f64 = 8.0;

/// What a ride's point is drawn as: one shape per device, in the order the
/// devices first appear, and a ring for a ride whose device is not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Circle,
    Square,
    Triangle,
    Diamond,
    Ring,
}

const SHAPES: [Shape; 4] = [
    Shape::Circle,
    Shape::Square,
    Shape::Triangle,
    Shape::Diamond,
];

/// A series: one device, or the rides whose device is not known.
struct Series<'a> {
    identifier: Option<&'a DeviceIdentifier>,
    name: String,
    shape: Shape,
}

/// The whole page.
pub fn page(rides: &[RideHeartRate], ranges: &Ranges, over: Over, run: &View) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Heart rate per power zone" }
                style { (crate::page::STYLE) (STYLE) }
            }
            body {
                (crate::page::following(run))
                main.wide {
                    (crate::page::menu("/heart-rate"))
                    h1 { "Heart rate per power zone" }
                    (crate::range::choices("/heart-rate", over))
                    (chart(rides, ranges, over))
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
                title { "Heart rate per power zone" }
                style { (crate::page::STYLE) }
            }
            body {
                main {
                    (crate::page::menu("/heart-rate"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// Where a date and a heart rate fall on the chart.
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

    fn y(&self, beats: u32) -> f64 {
        self.at(f64::from(beats))
    }

    fn at(&self, beats: f64) -> f64 {
        (FLOOR - TOP).mul_add(
            -(beats - f64::from(self.low)) / f64::from(self.high - self.low),
            FLOOR,
        )
    }

    /// Where a day falls: its middle, since a ride's day is all a line knows.
    fn day(&self, on: jiff::civil::Date) -> f64 {
        self.x(crate::range::seconds(on) + DAY / 2)
    }
}

/// The chart over the range chosen, its legend and its table.
///
/// **A device keeps its shape over every range**, so the shapes are the whole
/// record's and the legend names only the devices the range holds.
fn chart(rides: &[RideHeartRate], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(rides.iter().map(RideHeartRate::on));
    let range =
        match crate::range::chosen(ranges, over, recorded, "No main ride has been recorded.") {
            Ok(range) => range,
            Err(said) => return said,
        };
    let every = series(rides);
    let lines: Vec<DeviceTrends> = trends(rides)
        .iter()
        .map(|device| device.within(range))
        .collect();
    let rides: Vec<RideHeartRate> = rides
        .iter()
        .filter(|ride| range.covers(ride.on()))
        .cloned()
        .collect();
    if rides.is_empty() {
        return html! { p { "No main ride in this range." } };
    }
    let series: Vec<Series<'_>> = every
        .into_iter()
        .filter(|one| {
            rides
                .iter()
                .any(|ride| keyed(&ride.sent_by) == one.identifier)
        })
        .collect();
    let rides = rides.as_slice();
    let (low, high) = beats_range(rides);
    let frame = Frame {
        range,
        first: crate::range::seconds(range.from) - DAY,
        last: crate::range::seconds(range.to) + DAY * 2,
        low,
        high,
    };
    let zones = zones_held(rides);
    html! {
        p.method { (METHOD) " · line: " (TREND) }
        div.plot { (plot(rides, &series, &frame, &lines)) }
        (legend(&zones, &series))
        (lines_table(&lines))
        (table(rides, &series, &zones))
        (intervals(&lines, &series, range))
    }
}

/// The class's Z3 intervals, one point per ride, each line with its band.
fn intervals(lines: &[DeviceTrends], series: &[Series<'_>], range: Range) -> Markup {
    let shown: Vec<&DeviceTrends> = lines
        .iter()
        .filter(|device| !device.intervals.is_empty())
        .collect();
    let beats: Vec<u32> = shown
        .iter()
        .flat_map(|device| device.intervals.periods.iter())
        .flat_map(|period| period.points.iter().map(|point| point.beats))
        .collect();
    html! {
        h2 { "Heart rate in Z3 intervals" }
        p.method { (INTERVALS_METHOD) " · line: " (TREND) ", band " (CONFIDENCE) }
        @if shown.is_empty() {
            p { "No ride in this range read a Z3 interval." }
        } @else {
            @let frame = Frame {
                range,
                first: crate::range::seconds(range.from) - DAY,
                last: crate::range::seconds(range.to) + DAY * 2,
                low: (beats.iter().min().copied().unwrap_or(100) / 10).saturating_sub(1) * 10,
                high: (beats.iter().max().copied().unwrap_or(180) / 10 + 2) * 10,
            };
            div.plot {
                svg.chart viewBox=(format!("0 0 {WIDTH} {FLOOR_ONLY}")) role="img"
                    aria-label="Average heart rate over the class's Z3 intervals, per main ride, by date" {
                    (grid(&frame))
                    @for device in &shown {
                        @let shape = shape_of(series, &SentBy::Device(device.device.clone()));
                        @for period in &device.intervals.periods {
                            @if let Some(drawn) = Drawn::of(period, range) {
                                polygon.band points=(drawn.band(period, &frame)) {}
                                (drawn.line(period, &frame, "trend z3", &reading(device.device.name().as_str(), "Z3 intervals", period)))
                            }
                            @for point in &period.points {
                                (mark(shape, frame.day(point.on), frame.y(point.beats), "z3",
                                    &format!("{} · {} · Z3 intervals {}bpm", point.on, device.device.name(), point.beats)))
                            }
                        }
                    }
                }
            }
            ul {
                @for device in &shown {
                    @for period in &device.intervals.periods {
                        @if period.line.is_some() {
                            li { (reading(device.device.name().as_str(), "Z3 intervals", period)) }
                        }
                    }
                }
            }
        }
    }
}

/// The part of a period's line inside the range, in days since its test.
struct Drawn {
    first: i32,
    last: i32,
}

impl Drawn {
    fn of(period: &Period, range: Range) -> Option<Self> {
        let line = period.line?;
        let since = period.since()?;
        let days = |on: jiff::civil::Date| since.until(on).ok().map(|span| span.get_days());
        let first = line.first().max(days(range.from)?);
        let last = line.last().min(days(range.to)?);
        (first <= last).then_some(Self { first, last })
    }

    fn line(&self, period: &Period, frame: &Frame, class: &str, title: &str) -> Markup {
        let Some(line) = period.line else {
            return html! {};
        };
        let at = |day: i32| {
            period
                .day(day)
                .map_or((0.0, 0.0), |on| (frame.day(on), frame.at(line.at(day))))
        };
        let ((x1, y1), (x2, y2)) = (at(self.first), at(self.last));
        html! {
            line class=(class) x1=(x1) y1=(y1) x2=(x2) y2=(y2) { title { (title) } }
        }
    }

    /// Along the top from the first day to the last, and back along the
    /// bottom, cut at the plot's edges: three rides give a band far wider
    /// than any heart rate.
    fn band(&self, period: &Period, frame: &Frame) -> String {
        let Some(line) = period.line else {
            return String::new();
        };
        let days: Vec<i32> = (self.first..=self.last).collect();
        let upper = days
            .iter()
            .map(|day| (*day, line.at(*day) + line.band(*day)));
        let lower = days
            .iter()
            .rev()
            .map(|day| (*day, line.at(*day) - line.band(*day)));
        upper
            .chain(lower)
            .filter_map(|(day, beats)| {
                period.day(day).map(|on| {
                    format!(
                        "{:.1},{:.1}",
                        frame.day(on),
                        frame.at(beats).clamp(TOP, FLOOR)
                    )
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What a line says, in words.
fn reading(device: &str, what: &str, period: &Period) -> String {
    let under = period.ftp.map_or_else(
        || "before any FTP test".to_owned(),
        |ftp| format!("FTP {} from {}", ftp.watts(), ftp.from()),
    );
    let Some(line) = period.line else {
        return format!(
            "{device} · {what} · {under}: {}, too few for a line",
            rides(period.rides)
        );
    };
    let direction = match period.direction() {
        Some(Direction::Rising) => "rising",
        Some(Direction::Falling) => "falling",
        Some(Direction::Maintaining) | None => "maintaining",
    };
    let week = line.per_session() * 7.0;
    let sign = if week < -0.05 { "−" } else { "+" };
    format!(
        "{device} · {what} · {under}: {}, {:.0} to {:.0} bpm, {direction} ({sign}{:.1} ± {:.1} bpm a week)",
        rides(period.rides),
        line.at(line.first()),
        line.at(line.last()),
        week.abs(),
        line.slope_margin() * 7.0,
    )
}

fn rides(count: usize) -> String {
    if count == 1 {
        "1 ride".to_owned()
    } else {
        format!("{count} rides")
    }
}

/// Every line on the chart above, in words.
fn lines_table(lines: &[DeviceTrends]) -> Markup {
    let rows: Vec<String> = lines
        .iter()
        .flat_map(|device| {
            device
                .zones
                .iter()
                .flat_map(move |(zone, trended): &(PowerZone, Trended)| {
                    trended
                        .periods
                        .iter()
                        .filter(|period| period.line.is_some())
                        .map(move |period| {
                            reading(
                                device.device.name().as_str(),
                                &format!("Z{}", zone.as_u8()),
                                period,
                            )
                        })
                })
        })
        .collect();
    html! {
        @if !rows.is_empty() {
            details {
                summary { "Every line in words" }
                ul { @for row in &rows { li { (row) } } }
            }
        }
    }
}

/// The heart rate and date grid, without the rows beneath the floor.
fn grid(frame: &Frame) -> Markup {
    html! {
        @for beats in (frame.low..=frame.high).step_by(10) {
            line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(beats)) y2=(frame.y(beats)) {}
            text.axis x=(LEFT - 6.0) y=(frame.y(beats) + 4.0) text-anchor="end" { (beats) }
        }
        @for (tick, label) in crate::range::ticks(frame.range) {
            @let at = crate::range::seconds(tick);
            line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
            text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (label) }
        }
        line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
    }
}

fn plot(
    rides: &[RideHeartRate],
    series: &[Series<'_>],
    frame: &Frame,
    lines: &[DeviceTrends],
) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Average heart rate in each power zone, per main ride, by date, and a line through each device's rides in each zone" {
            (grid(frame))
            @for device in lines {
                @for (zone, trended) in &device.zones {
                    @for period in &trended.periods {
                        @if let Some(drawn) = Drawn::of(period, frame.range) {
                            (drawn.line(period, frame, &format!("trend z{}", zone.as_u8()),
                                &reading(device.device.name().as_str(), &format!("Z{}", zone.as_u8()), period)))
                        }
                    }
                }
            }
            text.axis x=(LEFT - 6.0) y=(NO_HEART_RATE_ROW + 4.0) text-anchor="end" { "no heart rate" }
            text.axis x=(LEFT - 6.0) y=(NO_POINT_ROW + 4.0) text-anchor="end" { "no zones" }
            @for ride in rides {
                @let x = frame.x(seconds(ride));
                @match &ride.by_zone {
                    ByZone::Zones(zones) => {
                        @for zone in zones.iter() {
                            (mark(shape_of(series, &ride.sent_by), x, frame.y(zone.average.as_u32()),
                                  &format!("z{}", zone.zone.as_u8()), &described(ride, series, Some(zone))))
                        }
                    }
                    ByZone::NoHeartRate => {
                        line.missing x1=(x) x2=(x)
                            y1=(NO_HEART_RATE_ROW - 6.0) y2=(NO_HEART_RATE_ROW + 6.0) {
                            title { (described(ride, series, None)) }
                        }
                    }
                    ByZone::NoFtp | ByZone::NoZoneHeld => {
                        line.unplaced x1=(x) x2=(x)
                            y1=(NO_POINT_ROW - 6.0) y2=(NO_POINT_ROW + 6.0) {
                            title { (described(ride, series, None)) }
                        }
                    }
                }
            }
        }
    }
}

fn legend(zones: &[PowerZone], series: &[Series<'_>]) -> Markup {
    html! {
        ul.legend {
            @for zone in zones {
                li {
                    svg.key viewBox="0 0 12 12" { circle class=(format!("z{}", zone.as_u8())) cx="6" cy="6" r="5" {} }
                    "Z" (zone.as_u8()) " " (zone.purpose())
                }
            }
        }
        ul.legend {
            @for one in series {
                li {
                    svg.key viewBox="0 0 12 12" { (mark(one.shape, 6.0, 6.0, "ink", "")) }
                    (one.name)
                }
            }
        }
    }
}

/// The same figures, for whoever cannot tell two colours apart.
fn table(rides: &[RideHeartRate], series: &[Series<'_>], zones: &[PowerZone]) -> Markup {
    html! {
        details {
            summary { "Every ride as a table" }
            table.figures {
                thead {
                    tr {
                        th { "date" } th { "class" } th { "device" }
                        @for zone in zones { th { "Z" (zone.as_u8()) } }
                    }
                }
                tbody {
                    @for ride in rides {
                        tr {
                            td { (ride.started_at.wall_clock().date()) }
                            td { (ride.venue.called()) }
                            td { (device_name(series, &ride.sent_by)) }
                            @match &ride.by_zone {
                                ByZone::Zones(held) => {
                                    @for zone in zones {
                                        td {
                                            @if let Some(figure) = held.iter().find(|held| held.zone == *zone) {
                                                (figure.average.as_u32())
                                            }
                                        }
                                    }
                                }
                                other => td colspan=(zones.len().max(1)) { (why(other)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One point.
fn mark(shape: Shape, x: f64, y: f64, class: &str, title: &str) -> Markup {
    let r = RADIUS;
    let points = |corners: &[(f64, f64)]| {
        corners
            .iter()
            .map(|(dx, dy)| format!("{:.1},{:.1}", x + dx, y + dy))
            .collect::<Vec<_>>()
            .join(" ")
    };
    html! {
        @match shape {
            Shape::Circle => circle class=(format!("point {class}")) cx=(x) cy=(y) r=(r) { title { (title) } },
            Shape::Ring => circle class=(format!("ring {class}")) cx=(x) cy=(y) r=(r - 1.0) { title { (title) } },
            Shape::Square => rect class=(format!("point {class}")) x=(x - r + 0.5) y=(y - r + 0.5)
                width=(SIDE) height=(SIDE) { title { (title) } },
            Shape::Triangle => polygon class=(format!("point {class}"))
                points=(points(&[(0.0, -r - 1.5), (r + 1.0, r), (-r - 1.0, r)])) { title { (title) } },
            Shape::Diamond => polygon class=(format!("point {class}"))
                points=(points(&[(0.0, -r - 0.5), (r + 0.5, 0.0), (0.0, r + 0.5), (-r - 0.5, 0.0)])) { title { (title) } },
        }
    }
}

/// Every device that sent a ride's heart rate, in the order they first did,
/// then the rides whose device is not known.
fn series(rides: &[RideHeartRate]) -> Vec<Series<'_>> {
    let mut series: Vec<Series<'_>> = Vec::new();
    let mut unknown = false;
    let mut shapes = SHAPES.iter().copied().cycle();
    for ride in rides {
        match &ride.sent_by {
            SentBy::Device(device) => {
                let identifier = device.identifiers().first();
                if !series.iter().any(|one| one.identifier == identifier) {
                    series.push(Series {
                        identifier,
                        name: device.name().to_string(),
                        shape: shapes.next().unwrap_or(Shape::Circle),
                    });
                }
            }
            SentBy::Unknown => unknown = true,
        }
    }
    if unknown {
        series.push(Series {
            identifier: None,
            name: "device unknown".to_owned(),
            shape: Shape::Ring,
        });
    }
    series
}

fn shape_of(series: &[Series<'_>], sent_by: &SentBy) -> Shape {
    find(series, sent_by).map_or(Shape::Ring, |one| one.shape)
}

fn device_name(series: &[Series<'_>], sent_by: &SentBy) -> String {
    find(series, sent_by).map_or_else(|| "device unknown".to_owned(), |one| one.name.clone())
}

fn find<'s, 'a>(series: &'s [Series<'a>], sent_by: &SentBy) -> Option<&'s Series<'a>> {
    let identifier = keyed(sent_by);
    series.iter().find(|one| one.identifier == identifier)
}

/// What a series is keyed on: a device's first identifier, or nothing.
fn keyed(sent_by: &SentBy) -> Option<&DeviceIdentifier> {
    match sent_by {
        SentBy::Device(device) => device.identifiers().first(),
        SentBy::Unknown => None,
    }
}

/// What a mark's tooltip says.
fn described(ride: &RideHeartRate, series: &[Series<'_>], zone: Option<&ZoneHeartRate>) -> String {
    let mut said = format!(
        "{} · {} · {}",
        ride.started_at.wall_clock().date(),
        ride.venue.called(),
        device_name(series, &ride.sent_by),
    );
    if let Some(zone) = zone {
        let over = PositiveDuration::from_seconds(zone.over.as_seconds())
            .map_or_else(|_| "\u{2014}".to_owned(), clock);
        let _ = write!(
            said,
            " · Z{} {} over {over}",
            zone.zone.as_u8(),
            zone.average
        );
    } else {
        let _ = write!(said, " · {}", why(&ride.by_zone));
    }
    if let Some(ftp) = ride.ftp {
        let _ = write!(said, " · FTP {}", ftp.watts());
    }
    said
}

const fn why(by_zone: &ByZone) -> &'static str {
    match by_zone {
        ByZone::Zones(_) => "",
        ByZone::NoHeartRate => "no heart rate recorded",
        ByZone::NoFtp => "no FTP test precedes it, so no zones",
        ByZone::NoZoneHeld => "no zone held for 120 s with heart rate recorded",
    }
}

/// Every zone any ride held, lowest first.
fn zones_held(rides: &[RideHeartRate]) -> Vec<PowerZone> {
    let held: BTreeSet<PowerZone> = rides
        .iter()
        .filter_map(|ride| match &ride.by_zone {
            ByZone::Zones(zones) => Some(zones.iter().map(|zone| zone.zone).collect::<Vec<_>>()),
            _ => None,
        })
        .flatten()
        .collect();
    held.into_iter().collect()
}

fn seconds(ride: &RideHeartRate) -> i64 {
    ride.started_at.instant().as_second()
}

const DAY: i64 = 86_400;

/// The heart rates the axis runs between, to the tens either side.
fn beats_range(rides: &[RideHeartRate]) -> (u32, u32) {
    let beats: Vec<u32> = rides
        .iter()
        .filter_map(|ride| match &ride.by_zone {
            ByZone::Zones(zones) => Some(
                zones
                    .iter()
                    .map(|zone| zone.average.as_u32())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    let low = beats.iter().min().copied().unwrap_or(100);
    let high = beats.iter().max().copied().unwrap_or(180);
    let low = low / 10 * 10;
    let high = (high / 10 + 1) * 10;
    (low, high.max(low + 10))
}

/// How far along the date axis an instant is, in minutes: fine enough for
/// a chart, and small enough to hold exactly.
pub fn along(part: i64, whole: i64) -> f64 {
    let minutes = |seconds: i64| f64::from(i32::try_from(seconds / 60).unwrap_or(i32::MAX));
    if whole <= 0 {
        return 0.5;
    }
    minutes(part) / minutes(whole)
}

/// The zones' colours, validated as a set in both modes. The one pair that
/// sits close in the dark is zone one and zone five, which are never near one
/// another in heart rate.
pub const STYLE: &str = "
main.wide { max-width: 60rem; }
:root {
  --surface: #fcfcfb; --ink: #0b0b0b; --muted: #898781; --grid: #e1e0d9; --axis: #c3c2b7;
  --z1: #2a78d6; --z2: #008300; --z3: #eda100; --z4: #e87ba4; --z5: #4a3aa7; --z6: #eb6834; --z7: #e34948;
}
@media (prefers-color-scheme: dark) { :root {
  --surface: #1a1a19; --ink: #ffffff; --grid: #2c2c2a; --axis: #383835;
  --z1: #3987e5; --z2: #008300; --z3: #c98500; --z4: #d55181; --z5: #6a5acd; --z6: #d95926; --z7: #e66767;
} }
body { background: var(--surface); color: var(--ink); }
h1 { font-size: 1.25rem; }
.method { color: var(--muted); }
div.plot { overflow-x: auto; }
svg.chart { width: 100%; min-width: 600px; height: auto; display: block; }
.grid { stroke: var(--grid); stroke-width: 1; }
.baseline { stroke: var(--axis); stroke-width: 1; }
.axis { fill: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
.point { stroke: var(--surface); stroke-width: 1.5; }
svg .ring { fill: none; stroke-width: 2; }
.missing { stroke: var(--muted); stroke-width: 2; }
.unplaced { stroke: var(--muted); stroke-width: 2; stroke-dasharray: 3 2; }
.z1 { fill: var(--z1); } .z2 { fill: var(--z2); } .z3 { fill: var(--z3); } .z4 { fill: var(--z4); }
.z5 { fill: var(--z5); } .z6 { fill: var(--z6); } .z7 { fill: var(--z7); }
.ring.z1 { stroke: var(--z1); } .ring.z2 { stroke: var(--z2); } .ring.z3 { stroke: var(--z3); }
.ring.z4 { stroke: var(--z4); } .ring.z5 { stroke: var(--z5); } .ring.z6 { stroke: var(--z6); }
.ring.z7 { stroke: var(--z7); }
line.trend { stroke-width: 2; stroke-linecap: round; }
line.trend.z1 { stroke: var(--z1); } line.trend.z2 { stroke: var(--z2); } line.trend.z3 { stroke: var(--z3); }
line.trend.z4 { stroke: var(--z4); } line.trend.z5 { stroke: var(--z5); } line.trend.z6 { stroke: var(--z6); }
line.trend.z7 { stroke: var(--z7); }
.band { fill: var(--z3); fill-opacity: 0.15; stroke: none; }
.ink { fill: var(--ink); } svg .ring.ink { stroke: var(--ink); }
ul.legend { display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; padding: 0; list-style: none; margin: 0.5rem 0; }
svg.key { width: 12px; height: 12px; margin-right: 0.35rem; vertical-align: -1px; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            heart_rate::{ByZone, RideHeartRate, SentBy, ZoneHeartRate},
            range::{Over, Range, Ranges},
        },
        cycling::{PowerZone, RideVenue},
        measure::{BeatsPerMinute, Duration},
        normalised::{OperatorZone, StartedAt},
        sequence::NonEmpty,
    };

    fn ride(day: i8, by_zone: ByZone) -> RideHeartRate {
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = jiff::civil::date(2026, 10, day)
            .at(19, 0, 0, 0)
            .to_zoned(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        RideHeartRate {
            started_at: StartedAt::new(at, zone),
            venue: RideVenue::new("class", "45 min Power Zone Ride").expect("a venue"),
            ftp: None,
            sent_by: SentBy::Unknown,
            by_zone,
            in_intervals: None,
        }
    }

    /// This mesocycle from Monday 2026-10-05, in a macrocycle from 2026-09-14.
    fn ranges() -> Ranges {
        let to = jiff::civil::date(2026, 10, 19);
        Ranges {
            mesocycle: Some(Range {
                from: jiff::civil::date(2026, 10, 5),
                to,
            }),
            macrocycle: Some(Range {
                from: jiff::civil::date(2026, 9, 14),
                to,
            }),
        }
    }

    fn held(zone: PowerZone, beats: u32) -> ZoneHeartRate {
        ZoneHeartRate {
            zone,
            average: BeatsPerMinute::new(beats).expect("a heart rate"),
            over: Duration::from_seconds(300),
        }
    }

    #[test]
    fn a_ride_is_a_point_per_zone_and_a_ride_with_no_heart_rate_is_a_mark_below() {
        let rides = [
            ride(
                4,
                ByZone::Zones(NonEmpty::of(
                    held(PowerZone::Two, 125),
                    vec![held(PowerZone::Three, 139)],
                )),
            ),
            ride(7, ByZone::NoHeartRate),
        ];
        let page =
            super::page(&rides, &ranges(), Over::All, &crate::run::View::default()).into_string();
        assert_eq!(page.matches(r#"class="ring z2""#).count(), 1);
        assert_eq!(page.matches(r#"class="ring z3""#).count(), 1);
        assert_eq!(page.matches(r#"class="missing""#).count(), 1);
        assert!(page.contains("2026-10-04 · 45 min Power Zone Ride · device unknown · Z3 139bpm"));
        assert!(page.contains("no heart rate recorded"));
    }

    /// A Forerunner 165, as the register would hold it.
    fn forerunner() -> domain::device::Device {
        use domain::device::{
            DeviceIdentifier, DeviceName, Measurand, Role, SerialNumber, Sighting,
        };
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = jiff::civil::date(2026, 9, 1)
            .at(12, 0, 0, 0)
            .to_zoned(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        let sighting = Sighting {
            device: DeviceIdentifier::Serial(
                SerialNumber::try_from("3483897389".to_owned()).expect("a serial"),
            ),
            also: Vec::new(),
            name: DeviceName::try_from("Forerunner 165").expect("a name"),
            role: Role::Recorder,
            measured: std::collections::BTreeSet::from([Measurand::HeartRate]),
            sent: std::collections::BTreeSet::from([Measurand::HeartRate]),
            at: StartedAt::new(at, zone),
            source: "Garmin",
            worn: false,
        };
        let register = domain::device::register(&[sighting], domain::device::Wear::default());
        register.devices().first().cloned().expect("a device")
    }

    #[test]
    fn each_device_s_rides_under_one_ftp_are_a_line_that_says_which_way_it_goes() {
        let ftp = domain::cycling::Ftp::new(
            domain::cycling::Watts::from_u32(177),
            jiff::civil::date(2026, 9, 23),
            domain::cycling::FtpProvenance::Estimated,
        )
        .expect("an FTP");
        let rides: Vec<RideHeartRate> = [(1, 140), (4, 133), (8, 135), (11, 134)]
            .into_iter()
            .map(|(day, beats)| {
                let mut ride = ride(
                    day,
                    ByZone::Zones(NonEmpty::of(held(PowerZone::Three, beats), vec![])),
                );
                ride.ftp = Some(ftp);
                ride.sent_by = SentBy::Device(forerunner());
                ride.in_intervals = Some(domain::analytical::heart_rate::IntervalHeartRate {
                    average: BeatsPerMinute::new(beats).expect("a heart rate"),
                    over: Duration::from_seconds(240),
                    intervals: 2,
                });
                ride
            })
            .collect();
        let page =
            super::page(&rides, &ranges(), Over::All, &crate::run::View::default()).into_string();
        assert_eq!(
            page.matches(r#"class="trend z3""#).count(),
            2,
            "a zone's line and the intervals'"
        );
        assert_eq!(page.matches(r#"class="band""#).count(), 1);
        assert!(
            page.contains("Forerunner 165 · Z3 intervals · FTP 177W from 2026-09-23: 4 rides,")
        );
        assert!(page.contains("maintaining"));
    }

    #[test]
    fn no_rides_is_said_rather_than_drawn() {
        let page =
            super::page(&[], &ranges(), Over::All, &crate::run::View::default()).into_string();
        assert!(page.contains("No main ride has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }

    #[test]
    fn a_range_shows_only_its_rides() {
        let zones = || ByZone::Zones(NonEmpty::of(held(PowerZone::Two, 125), vec![]));
        let rides = [ride(4, zones()), ride(7, zones())];
        let page = super::page(
            &rides,
            &ranges(),
            Over::Mesocycle,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(page.contains("2026-10-07 ·"));
        assert!(!page.contains("2026-10-04 ·"), "before this mesocycle");
        assert!(page.contains("<strong>this mesocycle</strong>"));
        assert!(page.contains(r#"href="/heart-rate?over=all""#));
    }
}
