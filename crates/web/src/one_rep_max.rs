//! Estimated 1RM as a trend, from one test to the next, as a chart (#420),
//! and the same over body weight and over fat-free mass (#253).
//!
//! **Sessions since the test across, the figure up**, so each stretch starts
//! at its test on the left and the stretches stack. The latest is drawn in
//! colour and the earlier ones in grey.
//!
//! **A test is a square with no band; an estimate is a point.** The line and
//! its band run through the estimates only, and the projection is a whisker at
//! the session the next test falls on: where the line puts it, and the range a
//! single test could land in. Nothing reads a verdict off either (the
//! operator, 2026-10-14). A ratio is drawn exactly as the mass is (the
//! operator, 2026-10-27), and a session with no weigh-in that day is a gap.
//!
//! **Server-rendered SVG** (§ 20), so the tooltip is each mark's `title`, and
//! the table beneath is the same figures.

use application::one_rep_max::{Lift, View};
use domain::analytical::one_rep_max::{
    CONFIDENCE, Figure, Line, Point, Projection, Reading, Relative, Stretch,
};
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Estimated 1RM";
const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 360.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 24.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 336.0;
const RADIUS: f64 = 3.0;

/// The whole page.
pub fn page(view: &View, run: &crate::run::View) -> Markup {
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
                    (crate::page::menu("/estimated-1rm"))
                    h1 { (TITLE) }
                    p.legend {
                        "estimator: " (view.estimator)
                        " · points: each session's highest estimate · squares: tests, beneath a failed attempt"
                        " · line: least squares through the estimates, band " (CONFIDENCE)
                        " · whisker: where the test could fall, " (CONFIDENCE)
                        " · a ratio is over that day's weigh-in"
                    }
                    @for lift in &view.lifts {
                        (lift_section(lift))
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
            body {
                main {
                    (crate::page::menu("/estimated-1rm"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// What a chart is drawn in.
#[derive(Debug, Clone, Copy)]
enum Scale {
    /// Grams, shown as kilograms.
    Kg,
    /// Basis points, shown as a multiple.
    Ratio,
}

impl Scale {
    /// A value as a reading: `96.7kg`, `1.15×`.
    fn show(self, value: f64) -> String {
        match self {
            Self::Kg => format!("{:.1}kg", (value / 100.0).round() / 10.0),
            Self::Ratio => format!("{:.2}×", value / 10_000.0),
        }
    }

    /// A value on the axis.
    fn axis(self, value: f64) -> String {
        match self {
            Self::Kg => format!("{:.0}kg", value / 1_000.0),
            Self::Ratio => format!("{:.2}×", value / 10_000.0),
        }
    }

    /// A slope, with its sign: none where it rounds to nothing.
    fn signed(self, value: f64) -> String {
        let shown = self.show(value.abs());
        if shown == self.show(0.0) {
            shown
        } else if value < 0.0 {
            format!("−{shown}")
        } else {
            format!("+{shown}")
        }
    }

    /// How often the axis is marked across `spread`.
    fn step(self, spread: f64) -> f64 {
        match self {
            Self::Kg if spread > 8_000.0 => 5_000.0,
            Self::Kg => 1_000.0,
            Self::Ratio if spread > 2_000.0 => 500.0,
            Self::Ratio => 100.0,
        }
    }

    /// The table's heading for the figure.
    const fn heading(self) -> &'static str {
        match self {
            Self::Kg => "kg",
            Self::Ratio => "×",
        }
    }
}

/// A figure as the page shows it.
trait Shown: Figure {
    const SCALE: Scale;
    /// The figure itself, as `domain` states it.
    fn shown(&self) -> String;
    /// What it was read from.
    fn read(&self) -> String;
}

impl Shown for Point {
    const SCALE: Scale = Scale::Kg;

    fn shown(&self) -> String {
        format!("{}kg", self.estimate.one_rep_max)
    }

    fn read(&self) -> String {
        from(self)
    }
}

impl Shown for Relative {
    const SCALE: Scale = Scale::Ratio;

    fn shown(&self) -> String {
        self.ratio.to_string()
    }

    /// The set, and the body mass to the nearest 0.1 kg as a weight is read.
    fn read(&self) -> String {
        let body = u32::try_from(self.body.as_grams()).map_or(f64::MAX, f64::from);
        let over = format!("over {}", Scale::Kg.show(body));
        match self.point.reading {
            Reading::Estimated => format!(
                "{}kg from {} · {over}",
                self.point.estimate.one_rep_max,
                from(&self.point)
            ),
            Reading::Tested { .. } => format!("{} · {over}", from(&self.point)),
        }
    }
}

fn lift_section(lift: &Lift) -> Markup {
    html! {
        h2 { (lift.lift.name()) }
        @if lift.absolute.is_empty() {
            p { "No test has been recorded." }
        } @else {
            (chart_section(lift.lift.name(), "in kilograms", "no estimate", &lift.absolute))
            @for (against, stretches) in &lift.relative {
                (chart_section(
                    lift.lift.name(),
                    &format!("over {}", against.name()),
                    &format!("no {} that day", against.name()),
                    stretches,
                ))
            }
        }
    }
}

/// One chart, its readings and its table. `missing` is what a test with no
/// figure reads.
fn chart_section<P: Shown>(lift: &str, over: &str, missing: &str, stretches: &[Stretch<P>]) -> Markup {
    html! {
        h3 { "Estimated 1RM " (over) }
        @for (index, stretch) in stretches.iter().enumerate() {
            div.(class(index, stretches.len())) {
                p.reading { (opening(stretch, missing)) }
                @if let Some(reading) = prediction(stretch, missing) {
                    p.reading { (reading) }
                }
            }
        }
        (chart(&format!("{lift} estimated 1RM {over} by session since the test"), stretches))
        (table(stretches))
    }
}

/// The latest stretch in colour, the earlier ones in grey.
const fn class(index: usize, count: usize) -> &'static str {
    if index + 1 == count {
        "current"
    } else {
        "earlier"
    }
}

/// The test a stretch starts from, and how many estimates follow it.
fn opening<P: Shown>(stretch: &Stretch<P>, missing: &str) -> String {
    let estimates = match stretch.estimates.len() {
        0 => "no estimate since".to_owned(),
        1 => "1 estimate since".to_owned(),
        count => format!("{count} estimates since"),
    };
    let line = match (&stretch.line, stretch.estimates.len()) {
        (None, 1 | 2) => " · a line needs at least 3",
        _ => "",
    };
    let test = stretch.test.map_or_else(|| missing.to_owned(), |test| test_reading(&test));
    format!("test on {} · {test} · {estimates}{line}", stretch.from)
}

/// A test's figure beside what it was read from, once each: a mass's reading
/// already names it.
fn test_reading<P: Shown>(test: &P) -> String {
    match P::SCALE {
        Scale::Kg => test.read(),
        Scale::Ratio => format!("{} · {}", test.shown(), test.read()),
    }
}

/// What the line predicts for the test that ends the stretch, and that test
/// where it has been performed.
fn prediction<P: Shown>(stretch: &Stretch<P>, missing: &str) -> Option<String> {
    let projection = stretch.projection?;
    let scale = P::SCALE;
    let predicted = format!(
        "predicted {} ({} – {})",
        scale.show(projection.value),
        scale.show(projection.low),
        scale.show(projection.high)
    );
    Some(match stretch.to {
        Some(_) => format!(
            "test on {}, session {}: {}; {predicted}",
            projection.on,
            projection.session,
            stretch
                .closed_by
                .map_or_else(|| missing.to_owned(), |(_, test)| test.shown())
        ),
        None => format!(
            "next test {}, session {}: {predicted}",
            projection.on, projection.session
        ),
    })
}

/// Where a session since the test and a value fall on the chart.
struct Frame {
    sessions: f64,
    low: f64,
    high: f64,
}

impl Frame {
    fn x(&self, session: u32) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(f64::from(session) / self.sessions, LEFT)
    }

    fn y(&self, value: f64) -> f64 {
        let share = (value - self.low) / (self.high - self.low);
        (FLOOR - TOP).mul_add(-share, FLOOR)
    }
}

/// The last session a stretch reaches: its projection, its closing test or its
/// last estimate.
fn reach<P>(stretch: &Stretch<P>) -> u32 {
    let projected = stretch
        .projection
        .map_or(0, |projection| projection.session);
    let closed = stretch.closed_by.as_ref().map_or(0, |(session, _)| *session);
    let estimated = stretch.estimates.last().map_or(0, |(session, _)| *session);
    projected.max(closed).max(estimated)
}

fn chart<P: Shown>(label: &str, stretches: &[Stretch<P>]) -> Markup {
    let sessions = stretches.iter().map(reach).max().unwrap_or(1).max(1);
    let Some((low, high, step)) = value_range(stretches) else {
        return html! { p { "No session has a figure." } };
    };
    let frame = Frame {
        sessions: f64::from(sessions),
        low,
        high,
    };
    html! {
        div.plot {
            svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img" aria-label=(label) {
                @for value in (0..).map(|n| f64::from(n).mul_add(step, low)).take_while(|value| *value <= high) {
                    line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.axis x=(LEFT - 6.0) y=(frame.y(value) + 4.0) text-anchor="end" {
                        (P::SCALE.axis(value))
                    }
                }
                @for session in 0..=sessions {
                    line.grid x1=(frame.x(session)) x2=(frame.x(session)) y1=(TOP) y2=(FLOOR) {}
                    text.axis x=(frame.x(session)) y=(FLOOR + 14.0) text-anchor="middle" {
                        @if session == 0 { "test" } @else { (session) }
                    }
                }
                line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
                @for (index, stretch) in stretches.iter().enumerate() {
                    g.(class(index, stretches.len())) { (marks(stretch, &frame)) }
                }
            }
        }
    }
}

/// A stretch's band, line, projection and points.
fn marks<P: Shown>(stretch: &Stretch<P>, frame: &Frame) -> Markup {
    html! {
        @if let Some(line) = &stretch.line {
            polygon.band points=(band(line, frame)) {}
            line.trend x1=(frame.x(first(line))) y1=(frame.y(line.at(line.first())))
                x2=(frame.x(last(line))) y2=(frame.y(line.at(line.last()))) {
                title { (P::SCALE.signed(line.per_session())) " a session" }
            }
            @if let Some(projection) = &stretch.projection {
                (whisker(line, projection, P::SCALE, frame))
            }
        }
        @if let Some(test) = &stretch.test {
            (test_mark(0, test, frame))
        }
        @for (session, point) in &stretch.estimates {
            @if let Some(value) = point.value() {
                circle.point cx=(frame.x(*session)) cy=(frame.y(value)) r=(RADIUS) {
                    title { "session " (session) " · " (point.on()) " · " (point.shown()) " · " (point.read()) }
                }
            }
        }
        @if let Some((session, test)) = &stretch.closed_by {
            (test_mark(*session, test, frame))
        }
    }
}

/// A test, as a square.
fn test_mark<P: Shown>(session: u32, test: &P, frame: &Frame) -> Markup {
    let Some(value) = test.value() else {
        return html! {};
    };
    let (x, y) = (frame.x(session), frame.y(value));
    html! {
        rect.point x=(RADIUS.mul_add(-1.5, x)) y=(RADIUS.mul_add(-1.5, y))
            width=(RADIUS * 3.0) height=(RADIUS * 3.0) {
            title { (test.on()) " · " (test_reading(test)) }
        }
    }
}

/// The line carried on to the test, and the range the test could fall in.
fn whisker(line: &Line, projection: &Projection, scale: Scale, frame: &Frame) -> Markup {
    let x = frame.x(projection.session);
    let title = format!(
        "{}, session {}: predicted {} ({} – {})",
        projection.on,
        projection.session,
        scale.show(projection.value),
        scale.show(projection.low),
        scale.show(projection.high)
    );
    html! {
        line.projected x1=(frame.x(last(line))) y1=(frame.y(line.at(line.last())))
            x2=(x) y2=(frame.y(projection.value)) {}
        line.whisker x1=(x) x2=(x) y1=(frame.y(projection.low)) y2=(frame.y(projection.high)) {}
        line.whisker x1=(x - 6.0) x2=(x + 6.0) y1=(frame.y(projection.low)) y2=(frame.y(projection.low)) {}
        line.whisker x1=(x - 6.0) x2=(x + 6.0) y1=(frame.y(projection.high)) y2=(frame.y(projection.high)) {}
        circle.predicted cx=(x) cy=(frame.y(projection.value)) r=(RADIUS + 1.0) { title { (title) } }
    }
}

fn first(line: &Line) -> u32 {
    u32::try_from(line.first()).unwrap_or(0)
}

fn last(line: &Line) -> u32 {
    u32::try_from(line.last()).unwrap_or(0)
}

/// The band as one polygon: along the top from the first estimate to the last,
/// and back along the bottom.
fn band(line: &Line, frame: &Frame) -> String {
    let sessions: Vec<i32> = (line.first()..=line.last()).collect();
    let upper = sessions
        .iter()
        .map(|session| (*session, line.at(*session) + line.band(*session)));
    let lower = sessions
        .iter()
        .rev()
        .map(|session| (*session, line.at(*session) - line.band(*session)));
    upper
        .chain(lower)
        .map(|(session, value)| {
            format!(
                "{:.1},{:.1}",
                frame.x(u32::try_from(session).unwrap_or(0)),
                frame.y(value)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The values the axis runs between, and how often it is marked. `None` with
/// nothing to draw.
fn value_range<P: Shown>(stretches: &[Stretch<P>]) -> Option<(f64, f64, f64)> {
    let points = stretches.iter().flat_map(|stretch| {
        stretch
            .test
            .iter()
            .chain(stretch.estimates.iter().map(|(_, point)| point))
            .chain(stretch.closed_by.iter().map(|(_, point)| point))
            .filter_map(Figure::value)
    });
    let bands = stretches
        .iter()
        .filter_map(|stretch| stretch.line)
        .flat_map(|line| {
            [line.first(), line.last()]
                .into_iter()
                .flat_map(move |at| [line.at(at) - line.band(at), line.at(at) + line.band(at)])
        });
    let projections = stretches
        .iter()
        .filter_map(|stretch| stretch.projection)
        .flat_map(|projection| [projection.low, projection.high]);
    let (min, max) =
        points
            .chain(bands)
            .chain(projections)
            .fold(None, |range: Option<(f64, f64)>, value| {
                Some(range.map_or((value, value), |(min, max)| {
                    (min.min(value), max.max(value))
                }))
            })?;
    let step = P::SCALE.step(max - min);
    Some((
        (min / step).floor() * step,
        ((max / step).floor() + 1.0) * step,
        step,
    ))
}

/// The same figures as a table.
fn table<P: Shown>(stretches: &[Stretch<P>]) -> Markup {
    html! {
        details {
            summary { "Every session as a table" }
            table.figures {
                thead { tr { th { "test on" } th { "session" } th { "date" } th { (P::SCALE.heading()) } th { "from" } } }
                tbody {
                    @for stretch in stretches {
                        @if let Some(test) = &stretch.test {
                            (row(stretch, 0, test))
                        }
                        @for (session, point) in &stretch.estimates {
                            (row(stretch, *session, point))
                        }
                        @if let Some((session, test)) = &stretch.closed_by {
                            (row(stretch, *session, test))
                        }
                    }
                }
            }
        }
    }
}

fn row<P: Shown>(stretch: &Stretch<P>, session: u32, point: &P) -> Markup {
    html! {
        tr {
            td { (stretch.from) }
            td { (session) }
            td { (point.on()) }
            td { (point.shown()) }
            td { (point.read()) }
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
.current { --series: var(--mark); }
.earlier { --series: var(--muted); }
.point { fill: var(--series); }
.trend { stroke: var(--series); stroke-width: 2; }
.projected { stroke: var(--series); stroke-width: 2; stroke-dasharray: 4 4; }
.whisker { stroke: var(--series); stroke-width: 1.5; }
.predicted { fill: var(--surface); stroke: var(--series); stroke-width: 2; }
.band { fill: var(--series); fill-opacity: 0.15; stroke: none; }
p.reading { font-size: 1.1rem; font-variant-numeric: tabular-nums; margin: 0.25rem 0; }
div.earlier p.reading { color: var(--muted); }
p.legend { color: var(--muted); font-size: 0.9rem; }
table.figures { font-variant-numeric: tabular-nums; font-size: 0.9rem; }
table.figures th { text-align: left; padding-right: 0.75rem; }
table.figures td { padding-right: 0.75rem; }
";

#[cfg(test)]
mod tests {
    use application::one_rep_max::{Lift, View};
    use domain::{
        analytical::{
            Estimate, RelativeStrength,
            one_rep_max::{Against, LIFTS, NextTest, Point, Reading, Relative, stretches, tests},
        },
        gym::Rir,
        measure::{Kg, RepCount},
    };
    use jiff::civil::{Date, date};

    use super::page;

    fn estimate_on(on: Date, grams: u64) -> Option<Point> {
        Some(Point {
            on,
            estimate: Estimate {
                one_rep_max: Kg::from_grams(grams),
                load: Kg::from_grams(80_000),
                reps: RepCount::new(3).ok()?,
                rir: Rir::Two,
            },
            reading: Reading::Estimated,
        })
    }

    fn test_on(on: Date, grams: u64) -> Option<Point> {
        Some(Point {
            on,
            estimate: Estimate {
                one_rep_max: Kg::from_grams(grams),
                load: Kg::from_grams(grams),
                reps: RepCount::new(1).ok()?,
                rir: Rir::Zero,
            },
            reading: Reading::Tested {
                failed: Kg::from_grams(95_000),
            },
        })
    }

    /// The entry test and the five sessions since, as the store held them on
    /// 2026-10-14.
    fn autumn() -> Option<Vec<Point>> {
        let mut points = vec![test_on(date(2026, 9, 25), 92_500)?];
        for (on, grams) in [
            (date(2026, 9, 29), 90_625),
            (date(2026, 10, 2), 96_774),
            (date(2026, 10, 5), 94_117),
            (date(2026, 10, 9), 94_117),
            (date(2026, 10, 12), 94_594),
        ] {
            points.push(estimate_on(on, grams)?);
        }
        Some(points)
    }

    fn view(points: &[Point], next: Option<NextTest>) -> View {
        let days: Vec<Date> = points.iter().map(|point| point.on).collect();
        let tests = tests(points);
        View {
            estimator: "rts",
            lifts: LIFTS
                .into_iter()
                .map(|lift| Lift {
                    lift,
                    absolute: stretches(points, &tests, &days, next),
                    relative: Against::ALL
                        .into_iter()
                        .map(|against| {
                            let relative: Vec<Relative> = points
                                .iter()
                                .filter_map(|point| {
                                    let body = Kg::from_grams(match against {
                                        Against::BodyWeight => 84_000,
                                        Against::FatFreeMass => 66_000,
                                    });
                                    Some(Relative {
                                        point: *point,
                                        body,
                                        ratio: RelativeStrength::of(
                                            point.estimate.one_rep_max,
                                            body,
                                        )?,
                                    })
                                })
                                // 2026-10-05 was not weighed.
                                .filter(|relative| relative.point.on != date(2026, 10, 5))
                                .collect();
                            (against, stretches(&relative, &tests, &days, next))
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    const NEXT: NextTest = NextTest {
        on: date(2026, 10, 23),
        sessions: 3,
    };

    #[test]
    fn the_prediction_reads_as_a_mass_and_its_range() {
        let page = page(
            &view(&autumn().expect("points"), Some(NEXT)),
            &crate::run::View::default(),
        )
        .into_string();
        assert!(
            page.contains(
                "test on 2026-09-25 · tested: 92.5kg × 1, 95kg failed · 5 estimates since"
            ),
            "{page}"
        );
        assert!(
            page.contains("next test 2026-10-23, session 8: predicted 96.7kg (82.3kg – 111.1kg)"),
            "{page}"
        );
        assert_eq!(
            page.matches(r#"class="point""#).count(),
            6 + 5 + 5,
            "an unweighed session is a gap in each ratio"
        );
        assert_eq!(page.matches("<rect").count(), 3, "the test is a square");
        assert_eq!(page.matches(r#"class="band""#).count(), 3);
    }

    #[test]
    fn a_ratio_reads_as_a_multiple_over_that_days_weigh_in() {
        let page = page(
            &view(&autumn().expect("points"), Some(NEXT)),
            &crate::run::View::default(),
        )
        .into_string();
        assert!(page.contains("Estimated 1RM over body weight"), "{page}");
        assert!(page.contains("Estimated 1RM over fat-free mass"), "{page}");
        assert!(
            page.contains(
                "test on 2026-09-25 · 1.10× · tested: 92.5kg × 1, 95kg failed · over 84.0kg · 4 estimates since"
            ),
            "{page}"
        );
        assert!(page.contains("next test 2026-10-23, session 8: predicted 1."), "{page}");
    }

    #[test]
    fn a_performed_test_reads_beside_its_prediction() {
        let mut points = autumn().expect("points");
        points.push(test_on(date(2026, 10, 23), 97_500).expect("a test"));
        let page = page(&view(&points, None), &crate::run::View::default()).into_string();
        assert!(
            page.contains("test on 2026-10-23, session 6: 97.5kg; predicted 95.6kg"),
            "{page}"
        );
    }

    #[test]
    fn two_estimates_are_points_without_a_line() {
        let points: Vec<Point> = autumn().expect("points").into_iter().take(3).collect();
        let page = page(&view(&points, Some(NEXT)), &crate::run::View::default()).into_string();
        assert!(
            page.contains("2 estimates since · a line needs at least 3"),
            "{page}"
        );
        assert!(!page.contains(r#"class="band""#) && !page.contains("next test"));
    }

    #[test]
    fn no_test_is_said_rather_than_drawn() {
        let points = vec![estimate_on(date(2026, 9, 29), 90_625).expect("a point")];
        let page = page(&view(&points, Some(NEXT)), &crate::run::View::default()).into_string();
        assert!(page.contains("No test has been recorded."));
        assert!(!page.contains("<svg"));
    }
}
