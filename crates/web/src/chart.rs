//! Heart rate per power zone, as a chart (#152).
//!
//! **Date across, heart rate up, the zone as colour and the device as shape**
//! (operator, 2026-10-14). A ride is one point per zone it held, so a column
//! of points is one ride. Nothing joins two points: a device's points are its
//! own series, and a line drawn between two shapes would stitch them (§ 6).
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each mark's `title` and
//! the table beneath is the same figures for whoever cannot tell two colours
//! apart.

use std::{collections::BTreeSet, fmt::Write as _};

use domain::{
    analytical::heart_rate::{ByZone, METHOD, RideHeartRate, SentBy, ZoneHeartRate},
    cycling::{PowerZone, clock},
    device::DeviceIdentifier,
    measure::PositiveDuration,
};
use maud::{DOCTYPE, Markup, html};

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 400.0;
const LEFT: f64 = 92.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
/// The plot's floor, above the two rows of rides that have no points.
const FLOOR: f64 = 320.0;
const NO_HEART_RATE_ROW: f64 = 352.0;
const NO_POINT_ROW: f64 = 376.0;
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
pub fn page(rides: &[RideHeartRate]) -> Markup {
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
                main.wide {
                    p { a href="/" { "Run next" } }
                    h1 { "Heart rate per power zone" }
                    (chart(rides))
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
            body { main { p.failed { "the store could not be read: " (error) } } }
        }
    }
}

/// Where a date and a heart rate fall on the chart.
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

    fn y(&self, beats: u32) -> f64 {
        (FLOOR - TOP).mul_add(
            -f64::from(beats - self.low) / f64::from(self.high - self.low),
            FLOOR,
        )
    }
}

/// The chart, its legend and its table.
pub fn chart(rides: &[RideHeartRate]) -> Markup {
    let Some((first, last)) = span(rides) else {
        return html! { p { "No main ride has been recorded." } };
    };
    let (low, high) = beats_range(rides);
    let frame = Frame {
        first,
        last,
        low,
        high,
    };
    let series = series(rides);
    let zones = zones_held(rides);
    html! {
        p.method { (METHOD) }
        div.plot { (plot(rides, &series, &frame)) }
        (legend(&zones, &series))
        (table(rides, &series, &zones))
    }
}

fn plot(rides: &[RideHeartRate], series: &[Series<'_>], frame: &Frame) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Average heart rate in each power zone, per main ride, by date" {
            @for beats in (frame.low..=frame.high).step_by(10) {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(beats)) y2=(frame.y(beats)) {}
                text.axis x=(LEFT - 6.0) y=(frame.y(beats) + 4.0) text-anchor="end" { (beats) }
            }
            @for (year, at) in years(frame.first, frame.last) {
                line.grid x1=(frame.x(at)) x2=(frame.x(at)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(at) + 3.0) y=(FLOOR + 14.0) { (year) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
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
    let identifier = match sent_by {
        SentBy::Device(device) => device.identifiers().first(),
        SentBy::Unknown => None,
    };
    series.iter().find(|one| one.identifier == identifier)
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

/// The first and last ride's instants, a day either side so no mark sits on
/// an edge.
fn span(rides: &[RideHeartRate]) -> Option<(i64, i64)> {
    let first = rides.iter().map(seconds).min()?;
    let last = rides.iter().map(seconds).max()?;
    Some((first - DAY * 7, last + DAY * 7))
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

/// Each new year's first instant inside the span, by the operator's calendar
/// as the first ride's zone has it.
pub fn years(first: i64, last: i64) -> Vec<(i16, i64)> {
    let Ok(start) = jiff::Timestamp::from_second(first) else {
        return Vec::new();
    };
    let zone = jiff::tz::TimeZone::UTC;
    let mut year = start.to_zoned(zone.clone()).year();
    let mut years = Vec::new();
    loop {
        year = year.saturating_add(1);
        let Ok(at) = jiff::civil::date(year, 1, 1).to_zoned(zone.clone()) else {
            break;
        };
        let at = at.timestamp().as_second();
        if at > last {
            break;
        }
        years.push((year, at));
    }
    years
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
.ink { fill: var(--ink); } svg .ring.ink { stroke: var(--ink); }
ul.legend { display: flex; flex-wrap: wrap; gap: 0.25rem 1rem; padding: 0; list-style: none; margin: 0.5rem 0; }
svg.key { width: 12px; height: 12px; margin-right: 0.35rem; vertical-align: -1px; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::heart_rate::{ByZone, RideHeartRate, SentBy, ZoneHeartRate},
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
        let page = super::page(&rides).into_string();
        assert_eq!(page.matches(r#"class="ring z2""#).count(), 1);
        assert_eq!(page.matches(r#"class="ring z3""#).count(), 1);
        assert_eq!(page.matches(r#"class="missing""#).count(), 1);
        assert!(page.contains("2026-10-04 · 45 min Power Zone Ride · device unknown · Z3 139bpm"));
        assert!(page.contains("no heart rate recorded"));
    }

    #[test]
    fn no_rides_is_said_rather_than_drawn() {
        let page = super::page(&[]).into_string();
        assert!(page.contains("No main ride has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }
}
