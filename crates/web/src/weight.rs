//! Body weight as one time series, as a chart (#250).
//!
//! **Date across, mass up, one point per weigh-in.** Nothing joins two points:
//! a line between two weigh-ins would claim something about the days between
//! them, and a gap in the record is drawn as the empty stretch it is (§ 37).
//!
//! **The trend is the line** (#252): an estimate of a continuous mass, drawn
//! as one line per unbroken run `domain` gives, so where it has no trend the
//! line breaks.
//!
//! **The range is the one choice** (#252, #497): `?over=` the current
//! mesocycle or macrocycle from its start to today, or everything, as on every
//! chart. It only
//! chooses which days are shown: the trend, and each day's reading of it as
//! rising, falling or maintaining, are `domain`'s and the same over any range.
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath is the same figures.

use domain::{
    analytical::{
        Weighed,
        body_weight::{Change, Direction, METHOD, Series, Trend},
        range::{Over, Range, Ranges},
    },
    canonical::Occurred,
    measure::Kg,
};
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

/// The whole page.
pub fn page(series: &Series, ranges: &Ranges, over: Over, run: &crate::run::View) -> Markup {
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
                    (crate::page::menu("/body-weight"))
                    h1 { (TITLE) }
                    (crate::range::choices("/body-weight", over))
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
                    (crate::page::menu("/body-weight"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// The range chosen, or why there is none.
fn body(series: &Series, ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(series.weigh_ins.iter().map(|weighed| weighed.at.day()));
    let range = match crate::range::chosen(ranges, over, recorded, "No weigh-in has been recorded.")
    {
        Ok(range) => range,
        Err(said) => return said,
    };
    let series = series.within(range);
    html! {
        p.reading {
            (range.from) " to " (range.to)
            @if let Some(summary) = series.summary() {
                " · average " (to_tenth(summary.average)) "kg"
                @if let Some(difference) = summary.difference {
                    " · trend " (signed(difference)) " kg"
                }
            }
        }
        @if let Some(change) = series.changes.last() {
            p.reading { "7 days to " (change.on) ": " (reading(*change)) }
        }
        (chart(&series, range))
    }
}

/// Where a day and a mass fall on the chart.
struct Frame {
    range: Range,
    days: i32,
    low: u32,
    high: u32,
    step: u32,
}

impl Frame {
    fn x(&self, on: Date) -> f64 {
        let along = if self.days <= 0 {
            0.5
        } else {
            f64::from(days_between(self.range.from, on)) / f64::from(self.days)
        };
        (WIDTH - LEFT - RIGHT).mul_add(along, LEFT)
    }

    fn y(&self, grams: u32) -> f64 {
        let share = f64::from(grams.saturating_sub(self.low)) / f64::from(self.high - self.low);
        (FLOOR - TOP).mul_add(-share, FLOOR)
    }
}

fn chart(series: &Series, range: Range) -> Markup {
    let weigh_ins = series.weigh_ins.as_slice();
    if weigh_ins.is_empty() {
        return html! { p { "No weigh-in in this range." } };
    }
    let (low, high, step) = mass_range(weigh_ins);
    let frame = Frame {
        range,
        days: days_between(range.from, range.to),
        low,
        high,
        step,
    };
    html! {
        p.legend { "points: each weigh-in · line: trend, the " (METHOD) " · against 7 days before: rising or falling beyond 0.125 kg, maintaining within it" }
        div.plot { (plot(&frame, &daily(series, &frame))) }
        (table(weigh_ins))
    }
}

/// Each weigh-in as a point, and the trend as a line.
fn daily(series: &Series, frame: &Frame) -> Markup {
    let changes: std::collections::BTreeMap<Date, Change> = series
        .changes
        .iter()
        .map(|change| (change.on, *change))
        .collect();
    html! {
        @for run in &series.trend {
            polyline.trend points=(points(run, frame)) {}
            @for day in run {
                circle.hover cx=(frame.x(day.on)) cy=(frame.y(grams(day.mass))) r=(RADIUS) {
                    title {
                        (day.on) " · trend " (to_tenth(day.mass)) "kg"
                        @if let Some(change) = changes.get(&day.on) {
                            " · " (reading(*change)) " over 7 days"
                        }
                    }
                }
            }
        }
        @for weighed in &series.weigh_ins {
            circle.point cx=(frame.x(weighed.at.day())) cy=(frame.y(grams(weighed.mass)))
                r=(RADIUS) { title { (when(&weighed.at)) " · " (weighed.mass) "kg" } }
        }
    }
}

fn plot(frame: &Frame, marks: &Markup) -> Markup {
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Body weight by date" {
            @for grams in (frame.low..=frame.high).step_by(frame.step as usize) {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(grams)) y2=(frame.y(grams)) {}
                text.axis x=(LEFT - 6.0) y=(frame.y(grams) + 4.0) text-anchor="end" {
                    (Kg::from_grams(u64::from(grams))) "kg"
                }
            }
            @for (tick, label) in crate::range::ticks(frame.range) {
                line.grid x1=(frame.x(tick)) x2=(frame.x(tick)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(frame.x(tick) + 3.0) y=(FLOOR + 14.0) { (label) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
            (marks)
        }
    }
}

/// A change in grams to the nearest 0.01 kg, with its sign.
fn signed(grams: i64) -> String {
    let hundredths = (grams.abs() + 5) / 10;
    let sign = if grams < 0 && hundredths > 0 {
        "−"
    } else if grams > 0 && hundredths > 0 {
        "+"
    } else {
        ""
    };
    format!("{sign}{}.{:02}", hundredths / 100, hundredths % 100)
}

/// A run of the trend as SVG polyline points.
fn points(run: &[Trend], frame: &Frame) -> String {
    run.iter()
        .map(|day| format!("{:.1},{:.1}", frame.x(day.on), frame.y(grams(day.mass))))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A change to the nearest 0.01 kg, and which way it went.
///
/// **Hundredths, not the trend's tenths**: the line between maintaining and
/// not is 0.125 kg, and −0.14 kg shown as −0.1 would read as maintaining.
fn reading(change: Change) -> String {
    let direction = match change.direction {
        Direction::Rising => "rising",
        Direction::Maintaining => "maintaining",
        Direction::Falling => "falling",
    };
    format!("{} kg, {direction}", signed(change.grams))
}

/// A trend to the nearest 0.1 kg: it is a mean, and its grams are arithmetic.
const fn to_tenth(mass: Kg) -> Kg {
    Kg::from_grams((mass.as_grams() + 50) / 100 * 100)
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

/// The masses the axis runs between, and how often it is marked: every 5 kg
/// across a wide spread, every 1 kg across a narrow one.
fn mass_range(series: &[Weighed]) -> (u32, u32, u32) {
    let grams = series.iter().map(|weighed| grams(weighed.mass));
    let (min, max) = (grams.clone().min().unwrap_or(0), grams.max().unwrap_or(0));
    let step = if max.saturating_sub(min) > 8_000 {
        5_000
    } else {
        1_000
    };
    (min / step * step, (max / step + 1) * step, step)
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
:root { --surface: #fcfcfb; --ink: #0b0b0b; --muted: #898781; --grid: #e1e0d9; --axis: #c3c2b7; --mark: #2a78d6; --trend: #0b0b0b; }
@media (prefers-color-scheme: dark) { :root {
  --surface: #1a1a19; --ink: #ffffff; --grid: #2c2c2a; --axis: #383835; --mark: #3987e5; --trend: #ffffff;
} }
body { background: var(--surface); color: var(--ink); }
h1 { font-size: 1.25rem; }
div.plot { overflow-x: auto; }
svg.chart { width: 100%; min-width: 600px; height: auto; display: block; }
.grid { stroke: var(--grid); stroke-width: 1; }
.baseline { stroke: var(--axis); stroke-width: 1; }
.axis { fill: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
.point { fill: var(--mark); }
.trend { fill: none; stroke: var(--trend); stroke-width: 2; stroke-linejoin: round; }
.hover { fill: transparent; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
p.legend { color: var(--muted); font-size: 0.9rem; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            Weighed,
            body_weight::Series,
            range::{Over, Range, Ranges},
        },
        canonical::Occurred,
        measure::Kg,
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::civil::{Date, date};

    use super::page;

    fn day(d: i8, grams: u64) -> Weighed {
        Weighed {
            at: Occurred::On(date(2026, 9, d)),
            mass: Kg::from_grams(grams),
            fat_free: None,
        }
    }

    /// The ranges on `today`, in a macrocycle from Monday 2026-09-07 and a
    /// mesocycle from Monday 2026-09-14.
    fn ranges(today: Date) -> Ranges {
        let range = |from| Range { from, to: today };
        Ranges {
            mesocycle: Some(range(date(2026, 9, 14))),
            macrocycle: Some(range(date(2026, 9, 7))),
        }
    }

    #[test]
    fn a_weigh_in_is_a_point_and_nothing_joins_two() {
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = date(2026, 10, 1)
            .at(7, 12, 0, 0)
            .to_zoned(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        let weigh_ins = vec![
            day(10, 79_000),
            Weighed {
                at: Occurred::At(StartedAt::new(at, zone)),
                mass: Kg::from_grams(84_100),
                fat_free: None,
            },
        ];
        let page = page(
            &Series::of(weigh_ins),
            &ranges(date(2026, 10, 1)),
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();
        assert_eq!(page.matches(r#"class="point""#).count(), 2);
        assert!(!page.contains("<polyline") && !page.contains("<path"));
        assert!(page.contains("2026-09-10 · 79kg"));
        assert!(page.contains("2026-10-01 07:12 · 84.1kg"));
    }

    #[test]
    fn no_weigh_ins_is_said_rather_than_drawn() {
        let page = page(
            &Series::of(Vec::new()),
            &ranges(date(2026, 9, 23)),
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(page.contains("No weigh-in has been recorded."));
        assert!(!page.contains("<svg class=\"chart\""));
    }

    #[test]
    fn the_trend_is_one_line_per_unbroken_run() {
        // Weigh-ins on the 1st to 7th and the 22nd to 28th, none between.
        let weigh_ins = (1..=7).chain(22..=28).map(|d| day(d, 84_043)).collect();
        let page = page(
            &Series::of(weigh_ins),
            &ranges(date(2026, 9, 28)),
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();
        assert_eq!(page.matches(r#"class="trend""#).count(), 2);
        assert_eq!(page.matches(r#"class="point""#).count(), 14);
        assert!(
            page.contains("2026-09-07 · trend 84kg"),
            "to the nearest 0.1 kg"
        );
    }

    #[test]
    fn the_range_shows_the_latest_seven_days_reading() {
        // A week at 84 kg from Monday 2026-09-07, then a week at 83.75 kg.
        let weigh_ins = (7..=13)
            .map(|d| day(d, 84_000))
            .chain((14..=20).map(|d| day(d, 83_750)))
            .collect();
        let page = page(
            &Series::of(weigh_ins),
            &ranges(date(2026, 9, 20)),
            Over::Macrocycle,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(
            page.contains("7 days to 2026-09-20: −0.25 kg, falling"),
            "{page}"
        );
        assert!(page.contains("2026-09-20 · trend 83.8kg · −0.25 kg, falling over 7 days"));
    }

    #[test]
    fn a_range_runs_from_the_start_of_its_cycle_to_today() {
        // Daily from 2026-09-01; this mesocycle began on Monday the 14th.
        let weigh_ins = (1..=23).map(|d| day(d, 84_000)).collect();
        let page = page(
            &Series::of(weigh_ins),
            &ranges(date(2026, 9, 23)),
            Over::Mesocycle,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(
            page.contains("2026-09-14 to 2026-09-23 · average 84kg · trend 0.00 kg"),
            "{page}"
        );
        assert_eq!(page.matches(r#"class="point""#).count(), 10);
    }

    #[test]
    fn a_day_with_no_plan_has_no_macrocycle() {
        let mut ranges = ranges(date(2026, 9, 23));
        ranges.macrocycle = None;
        let page = page(
            &Series::of(vec![day(1, 84_000)]),
            &ranges,
            Over::Macrocycle,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(
            page.contains("No macrocycle is planned for today."),
            "{page}"
        );
        assert!(
            page.contains("this mesocycle"),
            "the other ranges are still offered"
        );
    }
}
