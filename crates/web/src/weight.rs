//! Body weight as one time series, as a chart (#250).
//!
//! **Date across, mass up, one point per weigh-in.** Nothing joins two points:
//! a line between two weigh-ins would claim something about the days between
//! them, and a gap in the record is drawn as the empty stretch it is (§ 37).
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath is the same figures.

use domain::{analytical::Weighed, canonical::Occurred, measure::Kg};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Body weight";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 2.5;
/// The mass axis is marked every 5 kg.
const STEP_GRAMS: u32 = 5_000;

/// The whole page.
pub fn page(series: &[Weighed]) -> Markup {
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
                main.wide {
                    p { a href="/" { "Run next" } }
                    h1 { (TITLE) }
                    (chart(series))
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

/// Where a day and a mass fall on the chart.
struct Frame {
    first: Date,
    days: i32,
    low: u32,
    high: u32,
}

impl Frame {
    fn x(&self, on: Date) -> f64 {
        let along = if self.days <= 0 {
            0.5
        } else {
            f64::from(days_between(self.first, on)) / f64::from(self.days)
        };
        (WIDTH - LEFT - RIGHT).mul_add(along, LEFT)
    }

    fn y(&self, grams: u32) -> f64 {
        let share = f64::from(grams.saturating_sub(self.low)) / f64::from(self.high - self.low);
        (FLOOR - TOP).mul_add(-share, FLOOR)
    }
}

fn chart(series: &[Weighed]) -> Markup {
    let (Some(first), Some(last)) = (series.first(), series.last()) else {
        return html! { p { "No weigh-in has been recorded." } };
    };
    let first = first.at.day();
    let last = last.at.day();
    let (low, high) = mass_range(series);
    let frame = Frame {
        first,
        days: days_between(first, last),
        low,
        high,
    };
    html! {
        div.plot { (plot(series, &frame, last)) }
        (table(series))
    }
}

fn plot(series: &[Weighed], frame: &Frame, last: Date) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Body weight, one point per weigh-in, by date" {
            @for grams in (frame.low..=frame.high).step_by(STEP_GRAMS as usize) {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(grams)) y2=(frame.y(grams)) {}
                text.axis x=(LEFT - 6.0) y=(frame.y(grams) + 4.0) text-anchor="end" {
                    (Kg::from_grams(u64::from(grams))) "kg"
                }
            }
            @for year in years(frame.first, last) {
                line.grid x1=(frame.x(year)) x2=(frame.x(year)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(year) + 3.0) y=(FLOOR + 14.0) { (year.year()) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
            @for weighed in series {
                circle.point cx=(frame.x(weighed.at.day())) cy=(frame.y(grams(weighed.mass)))
                    r=(RADIUS) { title { (when(&weighed.at)) " · " (weighed.mass) "kg" } }
            }
        }
    }
}

/// The same figures as a table.
fn table(series: &[Weighed]) -> Markup {
    html! {
        details {
            summary { "Every weigh-in as a table" }
            table.figures {
                thead { tr { th { "date" } th { "kg" } } }
                tbody {
                    @for weighed in series {
                        tr { td { (when(&weighed.at)) } td { (weighed.mass) } }
                    }
                }
            }
        }
    }
}

/// The day, and the time where a clock was read.
fn when(at: &Occurred) -> String {
    at.instant().map_or_else(
        || at.day().to_string(),
        |started| started.wall_clock().strftime("%Y-%m-%d %H:%M").to_string(),
    )
}

/// The masses the axis runs between, to the 5 kg either side.
fn mass_range(series: &[Weighed]) -> (u32, u32) {
    let grams = series.iter().map(|weighed| grams(weighed.mass));
    let low = grams.clone().min().unwrap_or(0) / STEP_GRAMS * STEP_GRAMS;
    let high = (grams.max().unwrap_or(0) / STEP_GRAMS + 1) * STEP_GRAMS;
    (low, high)
}

/// Each 1 January after the first day, through the last.
fn years(first: Date, last: Date) -> Vec<Date> {
    (first.year().saturating_add(1)..=last.year())
        .filter_map(|year| Date::new(year, 1, 1).ok())
        .collect()
}

/// A body's mass in grams, which a `u32` holds with room to spare.
fn grams(mass: Kg) -> u32 {
    u32::try_from(mass.as_grams()).unwrap_or(u32::MAX)
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
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::Weighed,
        canonical::Occurred,
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::civil::date;

    #[test]
    fn a_weigh_in_is_a_point_and_nothing_joins_two() {
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = date(2026, 10, 1)
            .at(7, 12, 0, 0)
            .to_zoned(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        let series = [
            Weighed {
                at: Occurred::On(date(2021, 3, 10)),
                mass: Kg::from_grams(79_000),
            },
            Weighed {
                at: Occurred::At(StartedAt::new(at, zone)),
                mass: Kg::from_grams(84_100),
            },
        ];
        let page = super::page(&series).into_string();
        assert_eq!(page.matches(r#"class="point""#).count(), 2);
        assert!(!page.contains("<polyline") && !page.contains("<path"));
        assert!(page.contains("2021-03-10 · 79kg"));
        assert!(page.contains("2026-10-01 07:12 · 84.1kg"));
    }

    #[test]
    fn no_weigh_ins_is_said_rather_than_drawn() {
        let page = super::page(&[]).into_string();
        assert!(page.contains("No weigh-in has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }
}
