//! Estimated 1RM as a trend, one planned macrocycle at a time, as a chart
//! (#420).
//!
//! **Weeks into the macrocycle across, estimate up**, so each macrocycle's line
//! starts at the left: as one improves on the last, the lines stack. The
//! current macrocycle is drawn in colour and the earlier ones in grey.
//!
//! **A point per session, a line per macrocycle, and the band around it.** The
//! band is how sure the line is; with few or noisy sessions it is wide, and
//! nothing reads a verdict off it (the operator, 2026-10-14). Nothing joins two
//! macrocycles, and a stretch with no session is empty (§ 37).
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each point's `title`, and
//! the table beneath is the same figures.

use application::one_rep_max::View;
use domain::{
    analytical::one_rep_max::{CONFIDENCE, Line, Planned, Point, Reading},
    gym::exercise::RepsExercise,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Estimated 1RM";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 3.0;

/// The whole page.
pub fn page(view: &View) -> Markup {
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
                    p.legend {
                        "estimator: " (view.estimator)
                        " · points: each session's highest estimate · squares: tested, beneath a failed attempt · line: least squares over the macrocycle · band: "
                        (CONFIDENCE) " confidence"
                    }
                    @for (lift, planned) in &view.lifts {
                        (lift_section(*lift, planned))
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

fn lift_section(lift: RepsExercise, planned: &[Planned]) -> Markup {
    html! {
        h2 { (lift.as_str()) }
        @if planned.is_empty() {
            p { "No macrocycle has been planned." }
        } @else {
            @for (index, each) in planned.iter().enumerate() {
                p.reading.(class(index, planned.len())) { (reading(each)) }
            }
            (chart(lift, planned))
            (table(planned))
        }
    }
}

/// The current macrocycle in colour, the earlier ones in grey.
const fn class(index: usize, count: usize) -> &'static str {
    if index + 1 == count {
        "current"
    } else {
        "earlier"
    }
}

/// A macrocycle's days, sessions and slope.
fn reading(planned: &Planned) -> String {
    let days = format!("{} to {}", planned.span.start(), last_day(planned));
    let sessions = match planned.points.len() {
        0 => "no session gave an estimate".to_owned(),
        1 => "1 session".to_owned(),
        count => format!("{count} sessions"),
    };
    let slope = match (&planned.line, planned.points.len()) {
        (Some(line), _) => {
            let (low, high) = line.per_week_interval();
            format!(
                " · {} kg a week ({CONFIDENCE}: {} to {})",
                tenths(line.per_week()),
                tenths(low),
                tenths(high)
            )
        }
        (None, 1 | 2) => " · a line needs at least 3".to_owned(),
        (None, _) => String::new(),
    };
    format!("{} · {days} · {sessions}{slope}", planned.name)
}

/// The last day a macrocycle's span occupies.
fn last_day(planned: &Planned) -> Date {
    planned
        .span
        .end()
        .yesterday()
        .unwrap_or_else(|_| planned.span.end())
}

/// Where a day into a macrocycle and a mass fall on the chart.
struct Frame {
    days: f64,
    low: f64,
    high: f64,
}

impl Frame {
    fn x(&self, day: i32) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(f64::from(day) / self.days, LEFT)
    }

    fn y(&self, grams: f64) -> f64 {
        let share = (grams - self.low) / (self.high - self.low);
        (FLOOR - TOP).mul_add(-share, FLOOR)
    }
}

fn chart(lift: RepsExercise, planned: &[Planned]) -> Markup {
    let weeks = planned
        .iter()
        .map(|each| each.span.calendar_weeks())
        .max()
        .unwrap_or(1)
        .max(1);
    let Some((low, high, step)) = mass_range(planned) else {
        return html! {};
    };
    let frame = Frame {
        days: f64::from(weeks) * 7.0,
        low,
        high,
    };
    html! {
        div.plot {
            svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
                aria-label=(format!("{} estimated 1RM by week into the macrocycle", lift.as_str())) {
                @for kg in (0..).map(|n| f64::from(n).mul_add(step, low)).take_while(|kg| *kg <= high) {
                    line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(kg)) y2=(frame.y(kg)) {}
                    text.axis x=(LEFT - 6.0) y=(frame.y(kg) + 4.0) text-anchor="end" {
                        (format!("{:.0}", kg / 1_000.0)) "kg"
                    }
                }
                @for week in 0..weeks {
                    @let day = i32::try_from(week * 7).unwrap_or(0);
                    line.grid x1=(frame.x(day)) x2=(frame.x(day)) y1=(TOP) y2=(FLOOR) {}
                    text.axis x=(frame.x(day) + 3.0) y=(FLOOR + 14.0) { "week " (week + 1) }
                }
                line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
                @for (index, each) in planned.iter().enumerate() {
                    g.(class(index, planned.len())) { (marks(each, &frame)) }
                }
            }
        }
    }
}

/// A macrocycle's band, line and points.
fn marks(planned: &Planned, frame: &Frame) -> Markup {
    let start = planned.span.start();
    html! {
        @if let Some(line) = &planned.line {
            polygon.band points=(band(line, frame)) {}
            line.trend x1=(frame.x(line.first())) y1=(frame.y(line.at(line.first())))
                x2=(frame.x(line.last())) y2=(frame.y(line.at(line.last()))) {
                title { (planned.name) " · " (tenths(line.per_week())) " kg a week" }
            }
        }
        @for point in &planned.points {
            @if let Some(day) = day_of(start, point.on) {
                @let (x, y) = (frame.x(day), frame.y(grams(point)));
                @let title = format!("{} · {}kg · {}", point.on, point.estimate.one_rep_max, from(point));
                @match point.reading {
                    Reading::Estimated => circle.point cx=(x) cy=(y) r=(RADIUS) { title { (title) } },
                    Reading::Tested { .. } => rect.point.tested x=(RADIUS.mul_add(-1.5, x)) y=(RADIUS.mul_add(-1.5, y))
                        width=(RADIUS * 3.0) height=(RADIUS * 3.0) { title { (title) } },
                }
            }
        }
    }
}

/// The band as one polygon: along the top from the first session to the last,
/// and back along the bottom.
fn band(line: &Line, frame: &Frame) -> String {
    let days: Vec<i32> = (line.first()..=line.last()).collect();
    let upper = days
        .iter()
        .map(|day| (*day, line.at(*day) + line.band(*day)));
    let lower = days
        .iter()
        .rev()
        .map(|day| (*day, line.at(*day) - line.band(*day)));
    upper
        .chain(lower)
        .map(|(day, grams)| format!("{:.1},{:.1}", frame.x(day), frame.y(grams)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The masses the axis runs between, in grams, and how often it is marked:
/// every 5 kg across a wide spread, every 1 kg across a narrow one. `None` with
/// nothing to draw.
fn mass_range(planned: &[Planned]) -> Option<(f64, f64, f64)> {
    let points = planned
        .iter()
        .flat_map(|each| each.points.iter().map(grams));
    let bands = planned
        .iter()
        .filter_map(|each| each.line)
        .flat_map(|line| {
            [line.first(), line.last()]
                .into_iter()
                .flat_map(move |day| [line.at(day) - line.band(day), line.at(day) + line.band(day)])
        });
    let (min, max) = points
        .chain(bands)
        .fold(None, |range: Option<(f64, f64)>, grams| {
            Some(range.map_or((grams, grams), |(min, max)| {
                (min.min(grams), max.max(grams))
            }))
        })?;
    let step = if max - min > 8_000.0 {
        5_000.0
    } else {
        1_000.0
    };
    Some((
        (min / step).floor() * step,
        ((max / step).floor() + 1.0) * step,
        step,
    ))
}

/// The same figures as a table.
fn table(planned: &[Planned]) -> Markup {
    html! {
        details {
            summary { "Every session as a table" }
            table.figures {
                thead { tr { th { "macrocycle" } th { "date" } th { "kg" } th { "from" } } }
                tbody {
                    @for each in planned {
                        @for point in &each.points {
                            tr {
                                td { (each.name) }
                                td { (point.on) }
                                td { (point.estimate.one_rep_max) }
                                td { (from(point)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The set a point was read from, and how.
fn from(point: &Point) -> String {
    match point.reading {
        Reading::Estimated => point.estimate.to_string(),
        Reading::Tested { failed } => format!(
            "tested: {}kg × {}, {failed}kg failed",
            point.estimate.load, point.estimate.reps
        ),
    }
}

fn day_of(start: Date, on: Date) -> Option<i32> {
    start.until(on).ok().map(|span| span.get_days())
}

fn grams(point: &Point) -> f64 {
    u32::try_from(point.estimate.one_rep_max.as_grams()).map_or(f64::MAX, f64::from)
}

/// Grams as kilograms to the nearest 0.1, with its sign.
fn tenths(grams: f64) -> String {
    let kg = format!("{:.1}", (grams / 100.0).round() / 10.0);
    match kg.strip_prefix('-') {
        Some("0.0") => "0.0".to_owned(),
        Some(magnitude) => format!("−{magnitude}"),
        None if kg == "0.0" => kg,
        None => format!("+{kg}"),
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
h2 { font-size: 1.1rem; }
div.plot { overflow-x: auto; }
svg.chart { width: 100%; min-width: 600px; height: auto; display: block; }
.grid { stroke: var(--grid); stroke-width: 1; }
.baseline { stroke: var(--axis); stroke-width: 1; }
.axis { fill: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
g.current { --series: var(--mark); }
g.earlier { --series: var(--muted); }
.point { fill: var(--series); }
.trend { stroke: var(--series); stroke-width: 2; }
.band { fill: var(--series); fill-opacity: 0.15; stroke: none; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; }
p.reading.earlier { color: var(--muted); }
p.legend { color: var(--muted); font-size: 0.9rem; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
table.figures td { padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use application::one_rep_max::View;
    use domain::{
        analytical::{
            Estimate,
            one_rep_max::{LIFTS, Point, Reading, by_macrocycle},
        },
        gym::Rir,
        measure::{Kg, RepCount},
        plan::{PlanName, PlanWindow, Span},
    };
    use jiff::civil::{Date, date};

    use super::page;

    /// A session's estimate on each day, read off 80 kg × 3 @ 2.
    fn points(days: &[(Date, u64)]) -> Option<Vec<Point>> {
        days.iter()
            .map(|(on, grams)| {
                Some(Point {
                    on: *on,
                    estimate: Estimate {
                        one_rep_max: Kg::from_grams(*grams),
                        load: Kg::from_grams(80_000),
                        reps: RepCount::new(3).ok()?,
                        rir: Rir::Two,
                    },
                    reading: Reading::Estimated,
                })
            })
            .collect()
    }

    fn view(points: &[Point], plans: &[PlanWindow]) -> View {
        View {
            estimator: "rts",
            lifts: LIFTS
                .into_iter()
                .map(|lift| (lift, by_macrocycle(points, plans)))
                .collect(),
        }
    }

    fn autumn() -> Option<PlanWindow> {
        Some(PlanWindow::new(
            PlanName::try_from("2026-autumn".to_owned()).ok()?,
            Span::new(date(2026, 9, 14), 6),
        ))
    }

    #[test]
    fn a_macrocycle_reads_its_slope_and_its_interval() {
        let points = points(&[
            (date(2026, 9, 7), 90_000),
            (date(2026, 9, 29), 90_625),
            (date(2026, 10, 2), 96_774),
            (date(2026, 10, 5), 94_117),
            (date(2026, 10, 9), 94_117),
            (date(2026, 10, 12), 94_594),
        ])
        .expect("points");
        let page = page(&view(&points, &[autumn().expect("a plan")])).into_string();
        assert!(
            page.contains(
                "2026-autumn · 2026-09-14 to 2026-10-25 · 5 sessions · +1.1 kg a week (95%: −4.0 to +6.1)"
            ),
            "{page}"
        );
        assert_eq!(page.matches(r#"class="point""#).count(), 5);
        assert_eq!(page.matches(r#"class="band""#).count(), 1);
        assert!(page.contains("front-squat-barbell"));
        assert!(page.contains("2026-10-02 · 96.774kg · 80kg × 3 @ 2"));
    }

    #[test]
    fn two_sessions_are_points_without_a_line() {
        let points =
            points(&[(date(2026, 9, 29), 90_625), (date(2026, 10, 2), 96_774)]).expect("points");
        let page = page(&view(&points, &[autumn().expect("a plan")])).into_string();
        assert!(
            page.contains("2 sessions · a line needs at least 3"),
            "{page}"
        );
        assert!(!page.contains(r#"class="band""#) && !page.contains(r#"class="trend""#));
    }

    #[test]
    fn no_plan_is_said_rather_than_drawn() {
        let points = points(&[(date(2026, 9, 29), 90_625)]).expect("points");
        let page = page(&view(&points, &[])).into_string();
        assert!(page.contains("No macrocycle has been planned."));
        assert!(!page.contains("<svg"));
    }
}
