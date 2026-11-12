//! Estimated 1RM over the macrocycle (#420, #688), and the same over body
//! weight and over fat-free mass (#253), on the detail template (#626).
//!
//! **As the operator settled it on a mock of his own record on 2026-11-04**
//! (#688): *"The card says what my e1RM is, what that is as a movement, and
//! where that movement sits within the noise, and the chart demonstrates how
//! that figure was estimated by fitting through the estimates and how that
//! line is narrowing as we get more information."*
//!
//! **The macrocycle only**, with no choice of span: a line through one
//! mesocycle's sessions carries a margin as wide as a test's, and one through
//! the whole history would join blocks the programming did not focus on the
//! lift (#688).
//!
//! **The card**: the current estimate, and the estimated change in
//! performance on the shared gauge, centred on zero with its margin shaded.
//!
//! **The chart, days across**: each session's estimate a dot, the line through
//! all of them, and behind it the band of what the line knew on each day. A
//! dashed line carries it on to the next planned test, with no range. A tick
//! marks where each mesocycle starts. One chart, the unit a choice (#253).

use application::one_rep_max::{Lift, View};
use domain::analytical::{
    one_rep_max::{Against, CONFIDENCE, Figure, Point, Relative, Trend},
    range::Range,
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
const RADIUS: f64 = 4.0;

/// The whole page.
pub fn page(view: &View, run: &RunView) -> Markup {
    detail::page(PATH, TITLE, None, run, &body(view, view.today))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::bar(PATH))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

fn body(view: &View, today: Date) -> Markup {
    if view.macrocycle.is_none() {
        return html! { p { "No macrocycle is planned for today." } };
    }
    html! {
        @for lift in &view.lifts {
            (lift_body(lift, &view.mesocycles, today))
        }
        (detail::foot(&format!(
            "Dots: each session's estimate ({}). Line: least squares through all of them; band: where the line through the sessions up to each day could lie, {CONFIDENCE}; dashed: the line carried on to the next test. A ratio is over that day's weigh-in.",
            view.estimator
        )))
    }
}

/// One lift: its summary, its chart and its latest sessions.
fn lift_body(lift: &Lift, mesocycles: &[Date], today: Date) -> Markup {
    html! {
        (summary(lift))
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
            div.option.o0 { (drawn("kg", &lift.absolute, mesocycles, today)) }
            @for (index, (against, trend)) in lift.relative.iter().enumerate() {
                div class=(format!("option o{}", index + 1)) {
                    (drawn(&format!("over {}", against.name()), trend, mesocycles, today))
                }
            }
        }
        (latest(&lift.absolute, today))
    }
}

/// What the toggle calls each ratio.
const fn short(against: Against) -> &'static str {
    match against {
        Against::BodyWeight => "× weight",
        Against::FatFreeMass => "× fat-free",
    }
}

/// The current estimate and the estimated change in performance, in
/// kilograms, the change on the gauge against its margin.
fn summary(lift: &Lift) -> Markup {
    let trend = &lift.absolute;
    let cell = match (trend.now(), trend.change, trend.figures.last()) {
        (Some(now), Some(change), _) => Cell {
            metric: Metric::OneRepMax,
            label: "Current estimate",
            figure: kg(now),
            unit: "kg",
            margin: None,
            readings: vec![detail::Reading::Change {
                by: change.by,
                noise: change.margin,
                direction: change.direction,
                said: signed(change.by),
                over: "estimated change in performance".to_owned(),
            }],
        },
        (_, _, Some(latest)) => Cell {
            metric: Metric::OneRepMax,
            label: "Latest estimate",
            figure: kg(latest.value().unwrap_or_default()),
            unit: "kg",
            margin: None,
            readings: vec![detail::Reading::NoRule(format!(
                "{} this macrocycle; a line needs 3.",
                counted(trend.figures.len(), "session")
            ))],
        },
        (_, _, None) => Cell {
            metric: Metric::OneRepMax,
            label: "This macrocycle",
            figure: "0".to_owned(),
            unit: "sessions",
            margin: None,
            readings: vec![detail::Reading::NoRule(
                "No estimate yet this macrocycle.".to_owned(),
            )],
        },
    };
    // The chart beneath names the lift.
    detail::summary(&[cell], "")
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

fn counted(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
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
        self.estimate.to_string()
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
        format!("{} · over {}", self.point.estimate, show(body))
    }
}

/// Where a day and a value fall on the chart.
struct Frame {
    /// The last day across, in days since the macrocycle's first.
    days: i32,
    low: f64,
    high: f64,
    step: f64,
}

impl Frame {
    fn x(&self, day: i32) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(f64::from(day) / f64::from(self.days.max(1)), LEFT)
    }

    /// Held within the plot, so a band wider than it is cut at its edge.
    fn y(&self, value: f64) -> f64 {
        let held = value.clamp(self.low, self.high);
        (HEIGHT - TOP - BOTTOM).mul_add((self.high - held) / (self.high - self.low), TOP)
    }
}

/// A chart's line beneath its title, and its plot.
fn drawn<P: Shown>(unit: &str, trend: &Trend<P>, mesocycles: &[Date], today: Date) -> Markup {
    let Some(frame) = framed(trend, today) else {
        return html! { p.sub { "No estimate yet this macrocycle." } };
    };
    let to = trend
        .from
        .checked_add(jiff::Span::new().days(i64::from(frame.days)))
        .unwrap_or(trend.from);
    html! {
        p.sub { (unit) }
        div.plotted {
            svg viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img" aria-label=(format!("estimated 1RM, {unit}, by day over the macrocycle")) {
                @for value in marks(&frame) {
                    line.grid x1="0" x2=(WIDTH) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.tick x=(WIDTH - 6.0) y=(frame.y(value) - 4.0) text-anchor="end" { (P::SCALE.axis(value)) }
                }
                @for start in mesocycles.iter().filter(|start| **start >= trend.from && **start <= to) {
                    @let x = frame.x(trend.day(*start));
                    line.grid x1=(x) x2=(x) y1=(TOP) y2=(HEIGHT - BOTTOM) stroke-dasharray="2 3" {}
                }
                @for (on, said) in spaced(crate::range::ticks(Range { from: trend.from, to })) {
                    // The macrocycle's first day sits on the edge, so its
                    // label starts there rather than running off it.
                    text.tick x=(frame.x(trend.day(on))) y=(HEIGHT - 6.0) text-anchor=(if on == trend.from { "start" } else { "middle" }) { (said) }
                }
                g.stretch { (marked(trend, &frame)) }
            }
            (taps(trend, &frame, today))
        }
    }
}

/// Every other mark where there are more than five, so the labels of a
/// macrocycle's weeks do not run into each other at phone width.
fn spaced(ticks: Vec<(Date, String)>) -> Vec<(Date, String)> {
    let every = if ticks.len() > 5 { 2 } else { 1 };
    ticks.into_iter().step_by(every).collect()
}

/// The band, the line, its extension and the dots.
fn marked<P: Shown>(trend: &Trend<P>, frame: &Frame) -> Markup {
    html! {
        @if trend.narrowing.len() > 1 {
            polygon.band points=(band(trend, frame)) {}
        }
        @if let Some(line) = &trend.line {
            line.line x1=(frame.x(line.first())) y1=(frame.y(line.at(line.first())))
                x2=(frame.x(line.last())) y2=(frame.y(line.at(line.last()))) {}
            @if let Some(next) = &trend.next {
                @let x = frame.x(trend.day(next.on));
                line.projected x1=(frame.x(line.last())) y1=(frame.y(line.at(line.last())))
                    x2=(x) y2=(frame.y(next.value)) {}
                circle.predicted cx=(x) cy=(frame.y(next.value)) r=(RADIUS) {}
            }
        }
        @for figure in &trend.figures {
            @if let Some(value) = figure.value() {
                circle.dot cx=(format!("{:.1}", frame.x(trend.day(figure.on())))) cy=(format!("{:.1}", frame.y(value))) r=(RADIUS) {}
            }
        }
    }
}

/// The band as one polygon: along the top from the third session to the
/// latest, and back along the bottom.
fn band<P: Shown>(trend: &Trend<P>, frame: &Frame) -> String {
    let upper = trend
        .narrowing
        .iter()
        .map(|known| (known.on, known.value + known.band));
    let lower = trend
        .narrowing
        .iter()
        .rev()
        .map(|known| (known.on, known.value - known.band));
    upper
        .chain(lower)
        .map(|(on, value)| format!("{:.1},{:.1}", frame.x(trend.day(on)), frame.y(value)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Each session, and the next test, shown by tapping anywhere nearer to it
/// than to its neighbours, as the template's charts do.
fn taps<P: Shown>(trend: &Trend<P>, frame: &Frame, today: Date) -> Markup {
    let mut days: Vec<(f64, Markup)> = trend
        .figures
        .iter()
        .map(|figure| {
            (
                frame.x(trend.day(figure.on())),
                html! { span.on { (detail::day(figure.on(), today)) } br; b { (figure.shown()) } " from " (figure.read()) },
            )
        })
        .collect();
    if let Some(next) = &trend.next {
        days.push((
            frame.x(trend.day(next.on)),
            html! { span.on { "Next test · " (detail::day(next.on, today)) } br; b { (P::SCALE.show(next.value)) } " on the line" },
        ));
    }
    let percent = |x: f64| x / WIDTH * 100.0;
    html! {
        div.days style=(format!("top:{:.2}%;bottom:{:.2}%", TOP / HEIGHT * 100.0, BOTTOM / HEIGHT * 100.0)) {
            @for (index, (x, said)) in days.iter().enumerate() {
                @let from = index.checked_sub(1).and_then(|before| days.get(before)).map_or(0.0, |(before, _)| f64::midpoint(*before, *x));
                @let to = days.get(index + 1).map_or(WIDTH, |(after, _)| f64::midpoint(*x, *after));
                @let width = (to - from).max(f64::EPSILON);
                label.day style=(format!("left:{:.3}%;width:{:.3}%", percent(from), percent(width))) {
                    input type="radio" name="one-rep-max-day" aria-label="Show this session";
                    span.cross style=(format!("left:{:.2}%", (x - from) / width * 100.0)) {}
                    span class=(if *x < WIDTH / 2.0 { "tip left" } else { "tip right" }) { (said) }
                }
            }
        }
    }
}

/// Every marked value on the axis, low to high.
fn marks(frame: &Frame) -> Vec<f64> {
    (0..=20)
        .map(f64::from)
        .map(|index| frame.step.mul_add(index, frame.low))
        .take_while(|value| *value <= frame.high + frame.step / 1_000.0)
        .collect()
}

/// The values the axis runs between, how often it is marked, and how many
/// days it runs across; `None` with nothing to draw.
///
/// **The figures, the line and the next test set the scale, not the band**:
/// the band from three sessions runs tens of kilograms either side, and
/// would flatten everything else. It is cut at the plot's edge instead.
fn framed<P: Shown>(trend: &Trend<P>, today: Date) -> Option<Frame> {
    let figures = trend.figures.iter().filter_map(Figure::value);
    let line = trend
        .line
        .iter()
        .flat_map(|line| [line.at(line.first()), line.at(line.last())]);
    let next = trend.next.iter().map(|next| next.value);
    let (min, max) =
        figures
            .chain(line)
            .chain(next)
            .fold(None, |range: Option<(f64, f64)>, value| {
                Some(range.map_or((value, value), |(min, max)| {
                    (min.min(value), max.max(value))
                }))
            })?;
    let step = P::SCALE.step(max - min);
    // A step of room at either end, so the band shows beyond the dots.
    let low = ((min - step) / step).floor() * step;
    let last = trend.next.map_or(today, |next| next.on).max(today);
    Some(Frame {
        days: trend.day(last).max(1),
        low,
        high: (((max + step) / step).ceil() * step).max(low + step),
        step,
    })
}

/// The macrocycle's sessions, newest first.
fn latest(trend: &Trend<Point>, today: Date) -> Markup {
    let rows: Vec<(String, Markup)> = trend
        .figures
        .iter()
        .rev()
        .take(7)
        .map(|point| {
            (
                format!("{} · {}", detail::day(point.on, today), point.estimate),
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
            one_rep_max::{Against, LIFTS, Point, Reading, Relative, trend},
        },
        gym::Rir,
        measure::{Kg, RepCount},
    };
    use jiff::civil::{Date, date};

    use super::page;

    const TODAY: Date = date(2026, 11, 4);
    const MACROCYCLE: Date = date(2026, 9, 14);
    const NEXT: Date = date(2026, 11, 20);

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

    /// The autumn macrocycle's nine sessions on the RTS chart, as a copy of
    /// the store held them on 2026-11-04.
    fn autumn() -> Option<Vec<Point>> {
        [
            (date(2026, 9, 29), 89_395),
            (date(2026, 10, 2), 98_425),
            (date(2026, 10, 5), 92_699),
            (date(2026, 10, 9), 95_596),
            (date(2026, 10, 12), 93_184),
            (date(2026, 10, 16), 97_613),
            (date(2026, 10, 19), 89_605),
            (date(2026, 10, 23), 97_137),
            (date(2026, 10, 30), 96_899),
        ]
        .into_iter()
        .map(|(on, grams)| estimate_on(on, grams))
        .collect()
    }

    fn view(points: &[Point]) -> View {
        View {
            estimator: "rts",
            today: TODAY,
            macrocycle: Some(MACROCYCLE),
            mesocycles: vec![MACROCYCLE, date(2026, 9, 28), date(2026, 10, 26)],
            lifts: LIFTS
                .into_iter()
                .map(|lift| Lift {
                    lift,
                    absolute: trend(points, MACROCYCLE, Some(NEXT)),
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
                            (against, trend(&relative, MACROCYCLE, Some(NEXT)))
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn drawn(points: &[Point]) -> String {
        page(&view(points), &crate::run::View::default()).into_string()
    }

    #[test]
    fn the_card_is_the_current_estimate_and_its_change_on_the_gauge() {
        let page = drawn(&autumn().expect("points"));
        assert!(page.contains("Current estimate"), "{page}");
        assert!(page.contains(">96.3<small>kg</small>"), "{page}");
        assert!(
            page.contains("<b>+3.2 kg</b><span>estimated change in performance</span>"),
            "{page}"
        );
        assert_eq!(page.matches(r#"class="gauge""#).count(), 1, "{page}");
        assert!(!page.contains(r#"class="badge"#), "no clear change: {page}");
    }

    #[test]
    fn there_is_no_choice_of_span() {
        let page = drawn(&autumn().expect("points"));
        assert!(!page.contains("this mesocycle"), "{page}");
        assert!(!page.contains(r#"class="choices""#), "{page}");
    }

    #[test]
    fn the_chart_is_dots_one_line_a_band_and_the_line_carried_on() {
        let page = drawn(&autumn().expect("points"));
        assert_eq!(
            page.matches(r#"class="dot""#).count(),
            9 + 8 + 8,
            "an unweighed session is a gap in each ratio"
        );
        assert_eq!(page.matches(r#"class="line""#).count(), 3);
        assert_eq!(page.matches(r#"class="band""#).count(), 3);
        assert_eq!(page.matches(r#"class="projected""#).count(), 3);
        assert!(!page.contains(r#"class="square""#) && !page.contains(r#"class="whisker""#));
        assert!(page.contains("Next test · 20 Nov"), "{page}");
        assert!(page.contains(">× weight</label>") && page.contains(">× fat-free</label>"));
    }

    #[test]
    fn under_three_sessions_the_card_is_the_latest_estimate() {
        let points: Vec<Point> = autumn().expect("points").into_iter().take(2).collect();
        let page = drawn(&points);
        assert!(page.contains("Latest estimate"), "{page}");
        assert!(
            page.contains("2 sessions this macrocycle; a line needs 3."),
            "{page}"
        );
        assert!(!page.contains(r#"class="band""#) && !page.contains("Next test"));
    }

    #[test]
    fn no_macrocycle_is_said_rather_than_drawn() {
        let mut view = view(&autumn().expect("points"));
        view.macrocycle = None;
        view.lifts.clear();
        let page = page(&view, &crate::run::View::default()).into_string();
        assert!(
            page.contains("No macrocycle is planned for today."),
            "{page}"
        );
        assert!(!page.contains("<svg viewBox"), "{page}");
    }
}
