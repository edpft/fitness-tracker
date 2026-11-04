//! Estimated 1RM, from one test to the next (#420), and the same over body
//! weight and over fat-free mass (#253), on the detail template (#626).
//!
//! **The summary is the line and the next test**, on the mocks the operator
//! reviewed on 2026-10-28: where the line through the estimates since the
//! test stands now, with its direction in kg a session, and where it puts the
//! next planned test, with its range and its direction against the last
//! test. Neither has a noise bar: a line through sessions and a prediction
//! carry their own ranges (#614). The test's own reading against its noise
//! is a line beneath, since rep-max tests have a chart of their own (#706).
//!
//! **Sessions since the test across, the figure up**, so each stretch starts
//! at its test on the left and the stretches stack, the latest in the
//! metric's colour and the earlier ones in grey. A test is a square with no
//! band; an estimate is a dot. The line and its band run through the
//! estimates only, and the projection is a whisker at the session the next
//! test falls on. One chart, the unit a choice (#253).
//!
//! **Opens on this macrocycle** (#626): the range chooses which stretches are
//! shown, each one that overlaps it.

use application::one_rep_max::{Lift, View};
use domain::analytical::{
    body_weight::Direction,
    one_rep_max::{
        Against, CONFIDENCE, Figure, Line, Point, Projection, Reading, Relative, Stretch, TestLine,
        TestRead,
    },
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Cell, Metric},
    run::View as RunView,
};

pub const PATH: &str = "/estimated-1rm";
const TITLE: &str = "Estimated 1RM";

/// The template's chart dimensions, so this chart sits in its card as the
/// others do.
const WIDTH: f64 = 412.0;
const HEIGHT: f64 = 230.0;
const LEFT: f64 = 12.0;
const RIGHT: f64 = 44.0;
const TOP: f64 = 20.0;
const BOTTOM: f64 = 26.0;
const RADIUS: f64 = 3.0;

/// The whole page.
pub fn page(view: &View, ranges: &Ranges, over: Over, run: &RunView) -> Markup {
    detail::page(PATH, TITLE, over, run, &body(view, ranges, over))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::menu(PATH))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

fn body(view: &View, ranges: &Ranges, over: Over) -> Markup {
    html! {
        @for lift in &view.lifts {
            (lift_body(lift, ranges, over))
        }
        (detail::foot(&format!(
            "Dots: each session's estimate ({}); squares: the tests, each its heaviest completed set. Line: least squares through the estimates since the test, band {CONFIDENCE}; whisker: where the next test could fall, {CONFIDENCE}. A ratio is over that day's weigh-in.",
            view.estimator
        )))
    }
}

/// One lift: its summary, its chart and its latest sessions, over the range.
fn lift_body(lift: &Lift, ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(lift.absolute.iter().flat_map(|stretch| {
        std::iter::once(stretch.from).chain(stretch.estimates.iter().map(|(_, point)| point.on))
    }));
    let range = match crate::range::chosen(ranges, over, recorded, "No test has been recorded.") {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let absolute = within(&lift.absolute, range);
    html! {
        (summary(lift, today))
        section class="card choice one-rep-max" {
            div.head {
                h2 { (lift.lift.name()) }
                div.toggle {
                    input type="radio" name="one-rep-max" id="one-rep-max-0" class="o0" checked;
                    label for="one-rep-max-0" { "kg" }
                    @for (index, (against, _)) in lift.relative.iter().enumerate() {
                        @let option = format!("one-rep-max-{}", index + 1);
                        input type="radio" name="one-rep-max" id=(option) class=(format!("o{}", index + 1));
                        label for=(option) { (short(*against)) }
                    }
                }
            }
            div.option.o0 { (drawn("kg", &absolute, today)) }
            @for (index, (against, stretches)) in lift.relative.iter().enumerate() {
                div class=(format!("option o{}", index + 1)) {
                    (drawn(&format!("over {}", against.name()), &within(stretches, range), today))
                }
            }
        }
        (latest(&absolute, today))
    }
}

/// What the toggle calls each ratio.
const fn short(against: Against) -> &'static str {
    match against {
        Against::BodyWeight => "× weight",
        Against::FatFreeMass => "× fat-free",
    }
}

/// The stretches that overlap `range`.
fn within<P: Clone>(stretches: &[Stretch<P>], range: Range) -> Vec<Stretch<P>> {
    stretches
        .iter()
        .filter(|stretch| stretch.from <= range.to && stretch.to.is_none_or(|to| to >= range.from))
        .cloned()
        .collect()
}

/// The line now and the next test, in kilograms, for the latest stretch, and
/// beneath them the test it runs from.
fn summary(lift: &Lift, today: Date) -> Markup {
    let Some(stretch) = lift.absolute.last() else {
        return html! {};
    };
    let estimates = counted(stretch.estimates.len(), "estimate", " since the test");
    let now = stretch.estimates.last();
    let on_the_line = stretch
        .line
        .zip(now)
        .and_then(|(line, (session, _))| Some((line, line.at(i32::try_from(*session).ok()?))));
    // Without a line, the latest estimate, so the card still says where the
    // sessions since the test stand.
    let line_cell = match (on_the_line, now) {
        (Some((line, value)), _) => Cell {
            metric: Metric::OneRepMax,
            label: "Now, on the line",
            figure: kg(value),
            unit: "kg",
            margin: None,
            readings: vec![slope(&line, &estimates)],
        },
        (None, Some((_, point))) => Cell {
            metric: Metric::OneRepMax,
            label: "Latest estimate",
            figure: kg(f64::from(
                u32::try_from(point.estimate.one_rep_max.as_grams()).unwrap_or(u32::MAX),
            )),
            unit: "kg",
            margin: None,
            readings: vec![detail::Reading::NoRule(format!(
                "{estimates}; a line needs at least 3."
            ))],
        },
        (None, None) => Cell {
            metric: Metric::OneRepMax,
            label: "Since the test",
            figure: "0".to_owned(),
            unit: "estimates",
            margin: None,
            readings: vec![detail::Reading::NoRule(
                "No estimate since the test.".to_owned(),
            )],
        },
    };
    let mut cells = vec![line_cell];
    if let Some(projection) = stretch.projection.filter(|_| stretch.to.is_none()) {
        cells.push(next_test(&projection, stretch.test.as_ref(), today));
    }
    let test = stretch.test.as_ref().map_or_else(String::new, |test| {
        let read = lift
            .tests
            .iter()
            .find(|read| read.on == test.on)
            .map_or_else(String::new, against_noise);
        format!(
            " · tested {}: {}{read}",
            detail::day(test.on, today),
            from(test)
        )
    });
    let line = lift
        .macrocycle
        .map_or_else(String::new, |line| over_the_macrocycle(&line));
    detail::summary(&cells, &format!("{}{test}{line}", lift.lift.name()))
}

/// The line's slope, with a direction where it is clear of zero (#420).
fn slope(line: &Line, estimates: &str) -> detail::Reading {
    let per = line.per_session();
    let margin = line.slope_margin();
    let said = format!("{} a session", signed(per));
    let over = format!("± {}, {estimates}", show(margin));
    if per - margin > 0.0 {
        detail::Reading::Direction {
            direction: Direction::Rising,
            said,
            over,
        }
    } else if per + margin < 0.0 {
        detail::Reading::Direction {
            direction: Direction::Falling,
            said,
            over,
        }
    } else {
        detail::Reading::NoRule(format!("{said}, {over}: not yet clear of zero"))
    }
}

/// Where the line puts the next test, its range, and which way that is from
/// the last test, where the whole range lies to one side of it.
fn next_test(projection: &Projection, test: Option<&Point>, today: Date) -> Cell<'static> {
    let reading = test.and_then(Figure::value).map_or_else(
        || detail::Reading::NoRule("No test to read it against.".to_owned()),
        |last| {
            let said = signed(projection.value - last);
            let over = format!("on the {} test", show(last));
            if projection.low > last {
                detail::Reading::Direction {
                    direction: Direction::Rising,
                    said,
                    over,
                }
            } else if projection.high < last {
                detail::Reading::Direction {
                    direction: Direction::Falling,
                    said,
                    over,
                }
            } else {
                detail::Reading::NoRule(format!("{said} {over}, which its range takes in"))
            }
        },
    );
    Cell {
        metric: Metric::OneRepMax,
        label: "Next test",
        figure: kg(projection.value),
        unit: "kg",
        margin: Some(format!(
            "{} – {} kg · {}",
            kg(projection.low),
            kg(projection.high),
            detail::day(projection.on, today)
        )),
        readings: vec![reading],
    }
}

/// ` · +2.5 kg, no clear change (noise ± 9.0 kg)`: a test against the one
/// before, beyond the noise of two tests (#688).
fn against_noise(read: &TestRead) -> String {
    let Some(moved) = read.moved else {
        return " · the first test".to_owned();
    };
    let by = |grams: i64| signed(i32::try_from(grams).map_or(f64::MAX, f64::from));
    let said = match moved.direction {
        Some(Direction::Rising) => "rising".to_owned(),
        Some(Direction::Falling) => "falling".to_owned(),
        Some(Direction::Maintaining) | None => read.noise.map_or_else(
            || "no clear change".to_owned(),
            |noise| format!("no clear change (noise ± {})", show(noise)),
        ),
    };
    format!(" · {} against the test before, {said}", by(moved.by))
}

/// ` · this macrocycle: +2.5 kg along the line through 2 tests, no clear
/// change (± 9.1 kg)`: the macrocycle's tests read by a line through all of
/// them (operator, 2026-11-03).
fn over_the_macrocycle(line: &TestLine) -> String {
    let said = match line.direction {
        Some(Direction::Rising) => "rising",
        Some(Direction::Falling) => "falling",
        Some(Direction::Maintaining) | None => "no clear change",
    };
    format!(
        " · this macrocycle: {} along the line through {} tests, {said} (± {})",
        signed(line.change),
        line.tests,
        show(line.margin)
    )
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
    /// A value as a reading: `96.7 kg`, `1.15×`.
    fn show(self, value: f64) -> String {
        match self {
            Self::Kg => show(value),
            Self::Ratio => format!("{:.2}×", value / 10_000.0),
        }
    }

    /// A value on the axis.
    fn axis(self, value: f64) -> String {
        match self {
            Self::Kg if (value / 1_000.0).fract().abs() > f64::EPSILON => {
                format!("{:.1}", value / 1_000.0)
            }
            Self::Kg => format!("{:.0}", value / 1_000.0),
            Self::Ratio => format!("{:.2}", value / 10_000.0),
        }
    }

    /// How often the axis is marked across `spread`.
    fn step(self, spread: f64) -> f64 {
        match self {
            Self::Kg if spread > 16_000.0 => 5_000.0,
            Self::Kg if spread > 6_000.0 => 2_500.0,
            Self::Kg => 1_000.0,
            Self::Ratio if spread > 2_000.0 => 500.0,
            Self::Ratio => 100.0,
        }
    }
}

/// Grams as kilograms to one place, without the unit: `96.7`.
fn kg(grams: f64) -> String {
    format!("{:.1}", (grams / 100.0).round() / 10.0)
}

/// Grams as kilograms with the unit: `96.7 kg`.
fn show(grams: f64) -> String {
    format!("{} kg", kg(grams))
}

/// Grams with their sign, none where they round to nothing: `+0.4 kg`.
fn signed(grams: f64) -> String {
    let shown = show(grams.abs());
    if shown == show(0.0) {
        shown
    } else if grams < 0.0 {
        format!("−{shown}")
    } else {
        format!("+{shown}")
    }
}

fn counted(count: usize, one: &str, after: &str) -> String {
    if count == 1 {
        format!("1 {one}{after}")
    } else {
        format!("{count} {one}s{after}")
    }
}

/// How a figure reads in its chart's taps.
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
        format!("{} kg", self.estimate.one_rep_max)
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
        format!("{} · over {}", from(&self.point), show(body))
    }
}

/// The set a point was read from, and how.
fn from(point: &Point) -> String {
    match point.reading {
        Reading::Estimated => point.estimate.to_string(),
        Reading::Tested { failed } => {
            let failed = failed.map_or_else(String::new, |failed| format!(", {failed} kg failed"));
            format!(
                "{} kg × {}{failed}",
                point.estimate.load, point.estimate.reps
            )
        }
    }
}

/// Where a session since the test and a value fall on the chart.
struct Frame {
    sessions: f64,
    low: f64,
    high: f64,
    step: f64,
}

impl Frame {
    fn x(&self, session: u32) -> f64 {
        let along = (f64::from(session) + 0.5) / (self.sessions + 1.0);
        (WIDTH - LEFT - RIGHT).mul_add(along, LEFT)
    }

    fn y(&self, value: f64) -> f64 {
        (HEIGHT - TOP - BOTTOM).mul_add((self.high - value) / (self.high - self.low), TOP)
    }
}

/// The last session a stretch reaches: its projection, its closing test or
/// its last estimate.
fn reach<P>(stretch: &Stretch<P>) -> u32 {
    let projected = stretch
        .projection
        .map_or(0, |projection| projection.session);
    let closed = stretch
        .closed_by
        .as_ref()
        .map_or(0, |(session, _)| *session);
    let estimated = stretch.estimates.last().map_or(0, |(session, _)| *session);
    projected.max(closed).max(estimated)
}

/// A chart's line beneath its title and its plot.
fn drawn<P: Shown>(unit: &str, stretches: &[Stretch<P>], today: Date) -> Markup {
    let sub = stretches.last().map_or_else(String::new, |stretch| {
        let line = stretch.line.map_or_else(String::new, |line| {
            let per = line.per_session();
            let sign = if per < 0.0 { "−" } else { "+" };
            format!(" · line {sign}{} a session", P::SCALE.show(per.abs()))
        });
        format!(
            "{} since the test of {}{line} · {unit}",
            counted(stretch.estimates.len(), "estimate", ""),
            detail::day(stretch.from, today)
        )
    });
    let Some(frame) = framed(stretches) else {
        return html! { p.sub { "Nothing in this range." } };
    };
    let sessions = stretches.iter().map(reach).max().unwrap_or(1).max(1);
    let every = tick_every(sessions);
    html! {
        p.sub { (sub) }
        div.plotted {
            svg viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img" aria-label=(format!("estimated 1RM, {unit}, by session since the test")) {
                @for value in marks(&frame) {
                    line.grid x1="0" x2=(WIDTH) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.tick x=(WIDTH - 6.0) y=(frame.y(value) - 4.0) text-anchor="end" { (P::SCALE.axis(value)) }
                }
                @for session in (0..=sessions).filter(|session| session % every == 0) {
                    text.tick x=(frame.x(session)) y=(HEIGHT - 6.0) text-anchor="middle" {
                        @if session == 0 { "test" } @else { (session) }
                    }
                }
                @for (index, stretch) in stretches.iter().enumerate() {
                    g class=(if index + 1 == stretches.len() { "stretch" } else { "stretch earlier" }) {
                        (stretch_marks(stretch, &frame))
                    }
                }
            }
            @if let Some(stretch) = stretches.last() {
                (taps(stretch, &frame, today))
            }
        }
    }
}

/// How often the sessions axis is labelled, so its labels do not crowd.
const fn tick_every(sessions: u32) -> u32 {
    match sessions {
        0..=10 => 1,
        11..=20 => 2,
        _ => 5,
    }
}

/// A stretch's band, line, projection, dots and squares.
fn stretch_marks<P: Shown>(stretch: &Stretch<P>, frame: &Frame) -> Markup {
    html! {
        @if let Some(line) = &stretch.line {
            polygon.band points=(band(line, frame)) {}
            line.line x1=(frame.x(first(line))) y1=(frame.y(line.at(line.first())))
                x2=(frame.x(last(line))) y2=(frame.y(line.at(line.last()))) {}
            @if let Some(projection) = &stretch.projection {
                (whisker(line, projection, frame))
            }
        }
        @for (session, point) in &stretch.estimates {
            @if let Some(value) = point.value() {
                circle.dot cx=(format!("{:.1}", frame.x(*session))) cy=(format!("{:.1}", frame.y(value))) r=(RADIUS + 1.0) {}
            }
        }
        @if let Some(test) = &stretch.test {
            (square(0, test, frame))
        }
        @if let Some((session, test)) = &stretch.closed_by {
            (square(*session, test, frame))
        }
    }
}

/// A test, as a square.
fn square<P: Shown>(session: u32, test: &P, frame: &Frame) -> Markup {
    let Some(value) = test.value() else {
        return html! {};
    };
    let side = RADIUS * 3.0;
    html! {
        rect.square x=(format!("{:.1}", frame.x(session) - side / 2.0)) y=(format!("{:.1}", frame.y(value) - side / 2.0))
            width=(side) height=(side) rx="1.5" {}
    }
}

/// The line carried on to the test, and the range the test could fall in.
fn whisker(line: &Line, projection: &Projection, frame: &Frame) -> Markup {
    let x = frame.x(projection.session);
    html! {
        line.projected x1=(frame.x(last(line))) y1=(frame.y(line.at(line.last())))
            x2=(x) y2=(frame.y(projection.value)) {}
        line.whisker x1=(x) x2=(x) y1=(frame.y(projection.low)) y2=(frame.y(projection.high)) {}
        line.whisker x1=(x - 6.0) x2=(x + 6.0) y1=(frame.y(projection.low)) y2=(frame.y(projection.low)) {}
        line.whisker x1=(x - 6.0) x2=(x + 6.0) y1=(frame.y(projection.high)) y2=(frame.y(projection.high)) {}
        circle.predicted cx=(x) cy=(frame.y(projection.value)) r=(RADIUS + 1.0) {}
    }
}

/// Each session of the latest stretch, and its next test, shown by tapping
/// anywhere nearer to it than to its neighbours, as the template's charts do.
fn taps<P: Shown>(stretch: &Stretch<P>, frame: &Frame, today: Date) -> Markup {
    let mut days: Vec<(u32, Markup)> = Vec::new();
    if let Some(test) = &stretch.test {
        days.push((
            0,
            html! { span.on { "Test · " (detail::day(test.on(), today)) } br; b { (test.shown()) } " · " (test.read()) },
        ));
    }
    for (session, point) in &stretch.estimates {
        days.push((
            *session,
            html! { span.on { "Session " (session) " · " (detail::day(point.on(), today)) } br; b { (point.shown()) } " from " (point.read()) },
        ));
    }
    if let Some((session, test)) = &stretch.closed_by {
        days.push((
            *session,
            html! { span.on { "Test · " (detail::day(test.on(), today)) } br; b { (test.shown()) } " · " (test.read()) },
        ));
    } else if let Some(projection) = &stretch.projection {
        days.push((
            projection.session,
            html! {
                span.on { "Next test · " (detail::day(projection.on, today)) } br;
                b { (P::SCALE.show(projection.value)) } " (" (P::SCALE.show(projection.low)) " – " (P::SCALE.show(projection.high)) ")"
            },
        ));
    }
    let at: Vec<f64> = days.iter().map(|(session, _)| frame.x(*session)).collect();
    let percent = |x: f64| x / WIDTH * 100.0;
    html! {
        div.days style=(format!("top:{:.2}%;bottom:{:.2}%", TOP / HEIGHT * 100.0, BOTTOM / HEIGHT * 100.0)) {
            @for (index, (_, said)) in days.iter().enumerate() {
                @let x = at.get(index).copied().unwrap_or(0.0);
                @let from = index.checked_sub(1).and_then(|before| at.get(before)).map_or(0.0, |before| f64::midpoint(*before, x));
                @let to = at.get(index + 1).map_or(WIDTH, |after| f64::midpoint(x, *after));
                @let width = (to - from).max(f64::EPSILON);
                label.day style=(format!("left:{:.3}%;width:{:.3}%", percent(from), percent(width))) {
                    input type="radio" name="one-rep-max-day" aria-label="Show this session";
                    span.cross style=(format!("left:{:.2}%", (x - from) / width * 100.0)) {}
                    span class=(if x < WIDTH / 2.0 { "tip left" } else { "tip right" }) { (said) }
                }
            }
        }
    }
}

fn first(line: &Line) -> u32 {
    u32::try_from(line.first()).unwrap_or(0)
}

fn last(line: &Line) -> u32 {
    u32::try_from(line.last()).unwrap_or(0)
}

/// The band as one polygon: along the top from the first estimate to the
/// last, and back along the bottom.
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

/// Every marked value on the axis, low to high.
fn marks(frame: &Frame) -> Vec<f64> {
    (0..=20)
        .map(f64::from)
        .map(|index| frame.step.mul_add(index, frame.low))
        .take_while(|value| *value <= frame.high + frame.step / 1_000.0)
        .collect()
}

/// The values the axis runs between and how often it is marked; `None` with
/// nothing to draw.
fn framed<P: Shown>(stretches: &[Stretch<P>]) -> Option<Frame> {
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
    // A quarter step of room at either end, so a mark on a whole step is not
    // cut in half by the chart's edge.
    let low = ((min - step / 4.0) / step).floor() * step;
    Some(Frame {
        sessions: f64::from(stretches.iter().map(reach).max().unwrap_or(1).max(1)),
        low,
        high: (((max + step / 4.0) / step).ceil() * step).max(low + step),
        step,
    })
}

/// The latest stretch's sessions, newest first, its test included.
fn latest(stretches: &[Stretch<Point>], today: Date) -> Markup {
    let Some(stretch) = stretches.last() else {
        return html! {};
    };
    let rows: Vec<(String, Markup)> = stretch
        .closed_by
        .iter()
        .map(|(_, test)| test)
        .chain(stretch.estimates.iter().rev().map(|(_, point)| point))
        .chain(stretch.test.iter())
        .take(7)
        .map(|point| {
            (
                format!("{} · {}", detail::day(point.on, today), from(point)),
                html! { (point.shown()) },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Sessions", "estimated 1RM", &rows)
}

#[cfg(test)]
mod tests {
    use application::one_rep_max::{Lift, View};
    use domain::{
        analytical::{
            Estimate, RelativeStrength,
            one_rep_max::{
                Against, LIFTS, NextTest, Point, Reading, Relative, read_tests, stretches, tests,
            },
            range::{Over, Range, Ranges},
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
                failed: Some(Kg::from_grams(grams + 2_500)),
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
                    tests: read_tests(points),
                    macrocycle: None,
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

    /// The autumn macrocycle, as the range the page opens on.
    const RANGES: Ranges = Ranges {
        mesocycle: Some(Range {
            from: date(2026, 9, 28),
            to: date(2026, 10, 14),
        }),
        macrocycle: Some(Range {
            from: date(2026, 9, 14),
            to: date(2026, 10, 14),
        }),
    };

    fn drawn(points: &[Point], next: Option<NextTest>, over: Over) -> String {
        page(
            &view(points, next),
            &RANGES,
            over,
            &crate::run::View::default(),
        )
        .into_string()
    }

    #[test]
    fn the_summary_is_the_line_now_and_the_next_test() {
        let page = drawn(&autumn().expect("points"), Some(NEXT), Over::Macrocycle);
        assert!(page.contains("Now, on the line"), "{page}");
        assert!(page.contains("5 estimates since the test"), "{page}");
        assert!(page.contains("Next test"), "{page}");
        assert!(page.contains("82.3 – 111.1 kg · 23 Oct"), "{page}");
        assert!(
            page.contains("tested 25 Sep: 92.5 kg × 1, 95 kg failed · the first test"),
            "{page}"
        );
        assert!(!page.contains(r#"class="gauge""#), "no noise bar: {page}");
    }

    #[test]
    fn each_stretch_is_dots_squares_a_band_and_a_whisker() {
        let page = drawn(&autumn().expect("points"), Some(NEXT), Over::Macrocycle);
        assert_eq!(
            page.matches(r#"class="dot""#).count(),
            5 + 4 + 4,
            "an unweighed session is a gap in each ratio"
        );
        assert_eq!(
            page.matches(r#"class="square""#).count(),
            3,
            "the test is a square"
        );
        assert_eq!(page.matches(r#"class="band""#).count(), 3);
        assert_eq!(page.matches(r#"class="whisker""#).count(), 3 * 3);
        assert!(page.contains(">× weight</label>") && page.contains(">× fat-free</label>"));
    }

    #[test]
    fn a_test_reads_against_the_one_before_and_its_noise() {
        let mut points = autumn().expect("points");
        points.retain(|point| point.on < date(2026, 10, 9));
        points.push(test_on(date(2026, 10, 9), 95_000).expect("a test"));
        let page = drawn(&points, None, Over::Macrocycle);
        assert!(
            page.contains(
                "tested 9 Oct: 95 kg × 1, 97.5 kg failed · +2.5 kg against the test before, no clear change (noise ± 9.0 kg)"
            ),
            "{page}"
        );
    }

    #[test]
    fn without_a_line_the_summary_is_the_latest_estimate() {
        let points: Vec<Point> = autumn().expect("points").into_iter().take(3).collect();
        let page = drawn(&points, Some(NEXT), Over::Macrocycle);
        assert!(page.contains("Latest estimate"), "{page}");
        assert!(
            page.contains("2 estimates since the test; a line needs at least 3."),
            "{page}"
        );
        assert!(!page.contains(r#"class="band""#) && !page.contains("Next test"));
    }

    #[test]
    fn no_test_is_said_rather_than_drawn() {
        let points = vec![estimate_on(date(2026, 9, 29), 90_625).expect("a point")];
        let page = drawn(&points, Some(NEXT), Over::All);
        assert!(page.contains("No test has been recorded."), "{page}");
        assert!(!page.contains("<svg viewBox"), "{page}");
    }
}
