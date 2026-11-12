//! The detail template (#614): every chart page, top to bottom, is a back
//! link and the page's name, the range, a summary card, one card per chart,
//! the latest entries, and one line saying what the line and band are.
//!
//! **Built on the references the operator named**, MacroFactor, Withings and
//! Octopus, and on the mocks he reviewed on his own data on 2026-10-28. His
//! conditions hold over the references: discrete observations are never
//! joined by a line, only the trend is; a trend shows its confidence; and
//! pages are simple and bright.
//!
//! **No verdict words** (#489): the summary's arrow and its ± say it.
//!
//! **Tapping a chart shows the nearest day's figures without a script**
//! (§ 20): each day is a radio button as wide as the stretch nearer to it than
//! to its neighbours, and the one checked shows its figures.

use domain::analytical::{
    body_weight::Direction,
    per_device::SentBy,
    range::{Over, Range},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::run::View;

/// What a chart or a summary draws, each in the one colour every page uses
/// for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    BodyWeight,
    Fat,
    FatFree,
    Asleep,
    Regularity,
    RestingHeartRate,
    HeartRateAtPower,
    Hrv,
    PulseWaveVelocity,
    Ftp,
    OneRepMax,
    TestHeartRate,
    Stress,
}

impl Metric {
    /// The stylesheet's class for it, which sets its colour.
    pub const fn class(self) -> &'static str {
        match self {
            Self::BodyWeight => "weight",
            Self::Fat => "fat",
            Self::FatFree => "fat-free",
            Self::Asleep => "sleep",
            Self::Regularity => "sleep-regularity",
            Self::RestingHeartRate => "resting-heart-rate",
            Self::HeartRateAtPower => "heart-rate-at-power",
            Self::Hrv => "hrv",
            Self::PulseWaveVelocity => "pulse-wave-velocity",
            Self::Ftp => "ftp",
            Self::OneRepMax => "one-rep-max",
            Self::TestHeartRate => "test-heart-rate",
            Self::Stress => "stress",
        }
    }
}

/// The whole page: a back link and its name, the range where the page offers
/// a choice of one, then `body`.
pub fn page(here: &str, title: &str, over: Option<Over>, run: &View, body: &Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(title, &[])) }
            body {
                (crate::page::following(run))
                main {
                    (crate::page::bar(here))
                    (crate::page::header(here, title))
                    @if let Some(over) = over {
                        (crate::range::choices(here, over))
                    }
                    (body)
                }
            }
        }
    }
}

/// One figure in the summary card.
pub struct Cell<'a> {
    pub metric: Metric,
    pub label: &'a str,
    /// The current figure, as it is read.
    pub figure: String,
    pub unit: &'a str,
    /// Its ±, where it has one.
    pub margin: Option<String>,
    /// Each way it is read, in turn: where it falls in its reference, and
    /// which way it went.
    pub readings: Vec<Reading>,
}

/// How the summary reads its figure.
pub enum Reading {
    /// Where the figure falls among reference ranges, the one holding it lit.
    Position { segments: Vec<Segment>, at: f64 },
    /// A change against the noise: the badge, the signed change, and a bar
    /// centred on zero with the noise shaded and a dot where the change fell.
    /// No direction, no badge: where the change turns weeks after the line
    /// does, a badge would contradict the chart (#688).
    Change {
        by: f64,
        noise: f64,
        direction: Option<Direction>,
        /// The change as it is written, signed.
        said: String,
        /// What it is a change over.
        over: String,
    },
    /// There is no rule to read the figure by, and it says so.
    NoRule(String),
}

/// One reference range, from its lower bound up to, not including, its upper.
pub struct Segment {
    pub from: f64,
    pub to: f64,
    pub name: String,
}

/// The summary card: each figure, large, and beneath the card, as of when.
pub fn summary(cells: &[Cell<'_>], asof: &str) -> Markup {
    html! {
        section.card.summary style=(format!("grid-template-columns:repeat({},minmax(0,1fr))", cells.len().max(1))) {
            @for cell in cells {
                div class=(format!("cell {}", cell.metric.class())) {
                    div.label { i.swatch {} (cell.label) }
                    div.figure { (cell.figure) small { (cell.unit) } }
                    @if let Some(margin) = &cell.margin {
                        div.margin { "± " (margin) }
                    }
                    @for each in &cell.readings { (reading(each)) }
                }
            }
            div.asof { (asof) }
        }
    }
}

fn reading(reading: &Reading) -> Markup {
    match reading {
        Reading::Position { segments, at } => position(segments, *at),
        Reading::Change {
            by,
            noise,
            direction,
            said,
            over,
        } => html! {
            div.change.plain[direction.is_none()] {
                @if let Some(direction) = direction {
                    (badge(*direction))
                }
                div { b { (said) } span { (over) } }
                (gauge(*by, *noise, None))
            }
        },
        Reading::NoRule(said) => html! { div.none { (said) } },
    }
}

/// A round arrow: up, down, or flat.
fn badge(direction: Direction) -> Markup {
    let (class, path) = match direction {
        Direction::Rising => ("badge", "M6 14 L14 6 M8 6 H14 V12"),
        Direction::Falling => ("badge", "M6 6 L14 14 M8 14 H14 V8"),
        Direction::Maintaining => ("badge flat", "M5 10 H15 M11 6 L15 10 L11 14"),
    };
    html! {
        svg class=(class) width="22" height="22" viewBox="0 0 20 20" aria-hidden="true" {
            circle cx="10" cy="10" r="10" {}
            path d=(path) {}
        }
    }
}

const GAUGE: f64 = 200.0;

/// A bar centred on zero, the noise either side shaded, and a dot where the
/// change fell; a change past the bar's end sits at the end. `aim` is where
/// the change was expected to fall, marked with a dashed line (#517).
pub fn gauge(by: f64, noise: f64, aim: Option<f64>) -> Markup {
    let middle = GAUGE / 2.0;
    let span = by
        .abs()
        .max(noise)
        .max(by.abs() * 1.2)
        .max(aim.map_or(0.0, f64::abs))
        .max(f64::EPSILON)
        * 1.25;
    let x = |value: f64| (middle - 8.0).mul_add((value / span).clamp(-1.0, 1.0), middle);
    html! {
        svg.gauge viewBox=(format!("0 0 {GAUGE} 24")) aria-hidden="true" {
            line.track x1="4" x2=(GAUGE - 4.0) y1="12" y2="12" {}
            @if noise > 0.0 {
                rect.noise x=(x(-noise)) y="6" width=(x(noise) - x(-noise)) height="12" rx="6" {}
            }
            line.zero x1=(middle) x2=(middle) y1="2" y2="22" {}
            @if let Some(aim) = aim {
                line.aim x1=(x(aim)) x2=(x(aim)) y1="1" y2="23" {}
            }
            circle.fell cx=(x(by)) cy="12" r="6" {}
        }
    }
}

/// The reference ranges side by side, the one holding the figure lit and a
/// marker above it.
fn position(segments: &[Segment], at: f64) -> Markup {
    let (Some(first), Some(last)) = (segments.first(), segments.last()) else {
        return html! {};
    };
    let (low, high) = (first.from, last.to);
    let x = |value: f64| (GAUGE - 8.0).mul_add((value - low) / (high - low), 4.0);
    let count = segments.len();
    html! {
        svg.reference viewBox=(format!("0 0 {GAUGE} 40")) aria-hidden="true" {
            @let held = at.clamp(low, high);
            @for (index, segment) in segments.iter().enumerate() {
                @let from = x(segment.from) + if index == 0 { 0.0 } else { 2.0 };
                @let to = x(segment.to) - if index + 1 == count { 0.0 } else { 2.0 };
                @let lit = segment.from <= held && (held < segment.to || index + 1 == count);
                rect class=(if lit { "segment lit" } else { "segment" })
                    x=(from) y="16" width=((to - from).max(0.0)) height="8" rx="4" {}
                text.segment-name x=(f64::midpoint(from, to)) y="38" text-anchor="middle" { (segment.name) }
            }
            path.pointer d=(format!("M{} 4 h12 l-6 8z", x(held) - 6.0)) {}
        }
    }
}

/// A trend's value on one day, and its ± where it has one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Traced {
    pub on: Date,
    pub value: f64,
    pub margin: Option<f64>,
}

/// One kind of mark on a chart.
pub enum Layer {
    /// Each observation, never joined to another.
    Dots(Vec<(Date, f64)>),
    /// Each observation by a second method, hollow so the two are told apart.
    Hollow(Vec<(Date, f64)>),
    /// Each observation of a third figure, solid where [`Self::Dots`] are
    /// faint (#548).
    Solid(Vec<(Date, f64)>),
    /// Stretches of days grouped under a name, shaded behind the chart, every
    /// other one, with the name above: the device a stretch was recorded by
    /// (#548).
    Spans(Vec<(Date, Date, String)>),
    /// Observations singled out, ringed around their dots.
    Rings(Vec<(Date, f64)>),
    /// The trend: one line per unbroken run, and its band where it has one.
    Trend(Vec<Vec<Traced>>),
    /// A reference region, from low to high on each day, in neutral grey.
    Reference(Vec<(Date, f64, f64)>),
    /// Each observation's span, from low to high on its day, never joined.
    Bars(Vec<(Date, f64, f64)>),
    /// Zero, across the whole chart, where a change is none.
    Zero,
    /// What was intended, dashed, one line per stretch a plan states it.
    Aim(Vec<Vec<(Date, f64)>>),
    /// A change: one line per unbroken run, filled down to zero, more
    /// strongly below it than above, so which side of zero it falls reads at
    /// a glance rather than as a trend (operator, 2026-11-02, #689).
    Filled(Vec<Vec<Traced>>),
}

impl Layer {
    fn values(&self) -> Vec<f64> {
        match self {
            Self::Dots(dots) | Self::Hollow(dots) | Self::Rings(dots) | Self::Solid(dots) => {
                dots.iter().map(|(_, value)| *value).collect()
            }
            Self::Spans(_) => Vec::new(),
            Self::Trend(runs) => runs
                .iter()
                .flatten()
                .flat_map(|day| {
                    let margin = day.margin.unwrap_or(0.0);
                    [day.value - margin, day.value + margin]
                })
                .collect(),
            Self::Reference(days) | Self::Bars(days) => days
                .iter()
                .flat_map(|(_, low, high)| [*low, *high])
                .collect(),
            Self::Zero => vec![0.0],
            Self::Aim(runs) => runs.iter().flatten().map(|(_, value)| *value).collect(),
            Self::Filled(runs) => runs
                .iter()
                .flatten()
                .map(|day| day.value)
                .chain(std::iter::once(0.0))
                .collect(),
        }
    }
}

/// How a chart's value axis reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Figures, rising up the chart.
    Figures,
    /// Hours from the morning's midnight, as a clock reads them, running down
    /// the chart: the evening before is at the top, so a later night sits
    /// lower.
    Clock,
}

/// One chart card.
pub struct Chart<'a> {
    /// Names its days' buttons, so two charts on a page keep theirs apart.
    pub id: &'a str,
    pub metric: Metric,
    pub title: &'a str,
    pub sub: String,
    pub range: Range,
    pub axis: Axis,
    pub layers: Vec<Layer>,
    /// What a tap on each day shows, oldest first.
    pub days: Vec<(Date, Markup)>,
}

const WIDTH: f64 = 412.0;
const HEIGHT: f64 = 230.0;
const LEFT: f64 = 12.0;
/// Room for the value axis's labels, which sit inside on the right.
const RIGHT: f64 = 44.0;
const TOP: f64 = 20.0;
const BOTTOM: f64 = 26.0;

/// Where a day and a value fall on the chart.
struct Frame {
    range: Range,
    axis: Axis,
    days: i32,
    low: f64,
    high: f64,
    step: f64,
}

impl Frame {
    /// A day's middle, so the range's first and last days are not on its edges.
    fn x(&self, on: Date) -> f64 {
        let along = (f64::from(days_between(self.range.from, on)) + 0.5) / f64::from(self.days + 1);
        (WIDTH - LEFT - RIGHT).mul_add(along, LEFT)
    }

    fn y(&self, value: f64) -> f64 {
        let down = match self.axis {
            Axis::Figures => self.high - value,
            Axis::Clock => value - self.low,
        };
        (HEIGHT - TOP - BOTTOM).mul_add(down / (self.high - self.low), TOP)
    }

    /// Every marked value, low to high.
    fn marks(&self) -> Vec<f64> {
        let count = ((self.high - self.low) / self.step).round();
        (0..=20)
            .map(f64::from)
            .take_while(|index| *index <= count)
            .map(|index| self.step.mul_add(index, self.low))
            .collect()
    }
}

/// The chart card, or what to say where the range holds nothing.
pub fn chart(chart: &Chart<'_>) -> Markup {
    html! {
        section class=(format!("card {}", chart.metric.class())) {
            h2 { (chart.title) }
            (drawn(chart))
        }
    }
}

/// One chart card for one figure in more than one unit, each option a chart
/// and the first shown; the others are a tap away, without a script (§ 20).
pub fn choice(id: &str, metric: Metric, title: &str, options: &[(&str, Chart<'_>)]) -> Markup {
    html! {
        section class=(format!("card choice {}", metric.class())) {
            div.head {
                h2 { (title) }
                div.toggle {
                    @for (index, (unit, _)) in options.iter().enumerate() {
                        @let option = format!("{id}-{index}");
                        input type="radio" name=(id) id=(option) class=(format!("o{index}")) checked[index == 0];
                        label for=(option) { (unit) }
                    }
                }
            }
            @for (index, (_, chart)) in options.iter().enumerate() {
                div class=(format!("option o{index}")) { (drawn(chart)) }
            }
        }
    }
}

/// A chart's line beneath its title and its plot, or what to say where the
/// range holds nothing: a reference alone is not something recorded.
pub fn drawn(chart: &Chart<'_>) -> Markup {
    let recorded = chart
        .layers
        .iter()
        .filter(|layer| !matches!(layer, Layer::Reference(_) | Layer::Zero | Layer::Aim(_)))
        .any(|layer| !layer.values().is_empty());
    let values: Vec<f64> = chart.layers.iter().flat_map(Layer::values).collect();
    let low = values.iter().copied().reduce(f64::min).filter(|_| recorded);
    let high = values.iter().copied().reduce(f64::max);
    let body = low.zip(high).map_or_else(
        || html! { p.sub { "Nothing in this range." } },
        |(low, high)| plotted(chart, &framed(chart.range, chart.axis, low, high)),
    );
    html! {
        p.sub { (chart.sub) }
        (body)
    }
}

/// The value axis's step, the fewest marks that still give it at least two
/// intervals, and its ends on whole steps; a clock's steps are hours that
/// divide its day.
fn framed(range: Range, axis: Axis, low: f64, high: f64) -> Frame {
    const STEPS: [f64; 16] = [
        0.05, 0.1, 0.2, 0.25, 0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0,
    ];
    const HOURS: [f64; 6] = [1.0, 2.0, 3.0, 6.0, 12.0, 24.0];
    let spread = high - low;
    let steps: &[f64] = match axis {
        Axis::Figures => &STEPS,
        Axis::Clock => &HOURS,
    };
    let step = steps
        .iter()
        .copied()
        .find(|step| spread / step <= 5.0)
        .unwrap_or(1000.0);
    let floor = (low / step).floor() * step;
    let ceiling = ((high / step).ceil() * step).max(floor + step);
    Frame {
        range,
        axis,
        days: days_between(range.from, range.to),
        low: floor,
        high: ceiling,
        step,
    }
}

fn plotted(chart: &Chart<'_>, frame: &Frame) -> Markup {
    // Small only where a long range is crowded: a few tests over years are
    // as easy to miss as a year of nights is to read at full size.
    let dots: usize = chart
        .layers
        .iter()
        .map(|layer| match layer {
            Layer::Dots(dots) | Layer::Hollow(dots) | Layer::Solid(dots) => dots.len(),
            _ => 0,
        })
        .sum();
    let radius = if frame.days > 400 && dots > 100 {
        1.5
    } else {
        3.0
    };
    html! {
        div.plotted {
            svg viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img" aria-label=(chart.title) {
                @for value in frame.marks() {
                    line.grid x1="0" x2=(WIDTH) y1=(frame.y(value)) y2=(frame.y(value)) {}
                    text.tick x=(WIDTH - 6.0) y=(frame.y(value) - 4.0) text-anchor="end" {
                        @match frame.axis {
                            Axis::Figures => (figure(value, frame.step)),
                            Axis::Clock => (clock(value)),
                        }
                    }
                }
                @for (x, label) in ticks(frame) {
                    text.tick x=(x) y=(HEIGHT - 6.0) text-anchor="middle" { (label) }
                }
                @for layer in &chart.layers {
                    @if let Layer::Spans(spans) = layer { (shaded(spans, frame)) }
                }
                @for layer in &chart.layers {
                    @if let Layer::Reference(days) = layer {
                        polygon.reference points=(region(days.iter().map(|(on, low, high)| (*on, *low, *high)), frame)) {}
                    }
                }
                @for layer in &chart.layers {
                    @match layer {
                        Layer::Trend(runs) => {
                            @for run in runs {
                                @if run.iter().all(|day| day.margin.is_some()) {
                                    polygon.band points=(region(run.iter().map(|day| {
                                        let margin = day.margin.unwrap_or(0.0);
                                        (day.on, day.value - margin, day.value + margin)
                                    }), frame)) {}
                                }
                                polyline.line points=(line(run, frame)) {}
                            }
                        }
                        Layer::Dots(dots) => { (marks(dots, "dot", frame, radius)) }
                        Layer::Hollow(dots) => { (marks(dots, "hollow", frame, radius)) }
                        Layer::Solid(dots) => { (marks(dots, "solid", frame, radius)) }
                        Layer::Rings(dots) => {
                            @for (on, value) in dots {
                                circle.ring cx=(format!("{:.1}", frame.x(*on))) cy=(format!("{:.1}", frame.y(*value))) r=(radius + 3.0) {}
                            }
                        }
                        Layer::Bars(bars) => {
                            @for (on, low, high) in bars {
                                @let x = format!("{:.1}", frame.x(*on));
                                line.bar x1=(x) x2=(x) y1=(format!("{:.1}", frame.y(*low))) y2=(format!("{:.1}", frame.y(*high)))
                                    stroke-width=(if frame.days > 400 { "1" } else { "2" }) {}
                            }
                        }
                        Layer::Zero => {
                            line.zero x1="0" x2=(WIDTH) y1=(format!("{:.1}", frame.y(0.0))) y2=(format!("{:.1}", frame.y(0.0))) {}
                        }
                        Layer::Filled(runs) => {
                            @let zero = frame.y(0.0);
                            defs {
                                clipPath id=(format!("{}-below", chart.id)) { rect x="0" y=(format!("{zero:.1}")) width=(WIDTH) height=(HEIGHT) {} }
                                clipPath id=(format!("{}-above", chart.id)) { rect x="0" y="0" width=(WIDTH) height=(format!("{zero:.1}")) {} }
                            }
                            @for run in runs {
                                @if let (Some(first), Some(last)) = (run.first(), run.last()) {
                                    @let area = format!("{:.1},{zero:.1} {} {:.1},{zero:.1}", frame.x(first.on), line(run, frame), frame.x(last.on));
                                    polygon.below points=(area) clip-path=(format!("url(#{}-below)", chart.id)) {}
                                    polygon.above points=(area) clip-path=(format!("url(#{}-above)", chart.id)) {}
                                }
                                polyline.line points=(line(run, frame)) {}
                            }
                        }
                        Layer::Aim(runs) => {
                            @for run in runs {
                                polyline.aim points=(run.iter().map(|(on, value)| format!("{:.1},{:.1}", frame.x(*on), frame.y(*value))).collect::<Vec<_>>().join(" ")) {}
                            }
                        }
                        Layer::Reference(_) | Layer::Spans(_) => {}
                    }
                }
            }
            (taps(chart, frame))
        }
    }
}

/// One circle per observation, of the stylesheet's `class`.
fn marks(dots: &[(Date, f64)], class: &str, frame: &Frame, radius: f64) -> Markup {
    html! {
        @for (on, value) in dots {
            circle class=(class) cx=(format!("{:.1}", frame.x(*on))) cy=(format!("{:.1}", frame.y(*value))) r=(radius) {}
        }
    }
}

/// Every other span shaded behind the chart, each named above it.
fn shaded(spans: &[(Date, Date, String)], frame: &Frame) -> Markup {
    html! {
        @for (index, (from, to, _)) in spans.iter().enumerate() {
            @let (left, right) = (frame.x(*from) - 7.0, frame.x(*to) + 7.0);
            @if index % 2 == 0 {
                rect.span x=(format!("{left:.1}")) y=(TOP) width=(format!("{:.1}", right - left)) height=(HEIGHT - TOP - BOTTOM) {}
            }
        }
        @for (x, name) in span_names(spans, frame) {
            text.tick x=(format!("{x:.1}")) y=(TOP - 6.0) text-anchor="middle" { (name) }
        }
    }
}

/// Where each span's name is centred: over its span, kept inside the chart,
/// and pushed right of the name before it where two short spans sit close
/// (TICKR and the next device in 2024, #548).
fn span_names(spans: &[(Date, Date, String)], frame: &Frame) -> Vec<(f64, String)> {
    // The tick face's width per character, near enough to keep names apart.
    const CHARACTER: f64 = 6.0;
    let mut placed = Vec::new();
    let mut taken = f64::MIN;
    for (from, to, name) in spans {
        let half =
            f64::from(u32::try_from(name.chars().count()).unwrap_or(u32::MAX)) * CHARACTER / 2.0;
        let centre = f64::midpoint(frame.x(*from), frame.x(*to))
            .max(taken + 8.0 + half)
            .min(WIDTH - RIGHT - half)
            .max(half);
        taken = centre + half;
        placed.push((centre, name.clone()));
    }
    placed
}

/// The date axis's labels, each kept inside the chart and none on top of
/// the one before.
///
/// **A label pushed in from the edge gives way** to the next one where the
/// two would collide, so the range's first day does not cost the axis its
/// second tick (21 Sep went missing after 14 Sep, operator, 2026-11-02).
fn ticks(frame: &Frame) -> Vec<(f64, String)> {
    let mut kept: Vec<(f64, String, bool)> = Vec::new();
    for (tick, label) in crate::range::ticks(frame.range) {
        let at = frame.x(tick);
        let x = at.clamp(LEFT + 20.0, WIDTH - RIGHT - 20.0);
        let pushed = (x - at).abs() > f64::EPSILON;
        match kept.last() {
            Some((before, _, before_pushed)) if x - before < 48.0 => {
                if *before_pushed && !pushed {
                    kept.pop();
                    kept.push((x, label, pushed));
                }
            }
            _ => kept.push((x, label, pushed)),
        }
    }
    kept.into_iter().map(|(x, label, _)| (x, label)).collect()
}

/// Each day's figures, shown by tapping anywhere nearer to it than to the
/// days either side.
fn taps(chart: &Chart<'_>, frame: &Frame) -> Markup {
    let at: Vec<f64> = chart.days.iter().map(|(on, _)| frame.x(*on)).collect();
    let percent = |x: f64| x / WIDTH * 100.0;
    html! {
        div.days style=(format!("top:{:.2}%;bottom:{:.2}%", TOP / HEIGHT * 100.0, BOTTOM / HEIGHT * 100.0)) {
            @for (index, (_, said)) in chart.days.iter().enumerate() {
                @let x = at.get(index).copied().unwrap_or(0.0);
                @let from = index.checked_sub(1).and_then(|before| at.get(before)).map_or(0.0, |before| f64::midpoint(*before, x));
                @let to = at.get(index + 1).map_or(WIDTH, |after| f64::midpoint(x, *after));
                @let width = (to - from).max(f64::EPSILON);
                label.day style=(format!("left:{:.3}%;width:{:.3}%", percent(from), percent(width))) {
                    input type="radio" name=(chart.id) aria-label="Show this day";
                    span.cross style=(format!("left:{:.2}%", (x - from) / width * 100.0)) {}
                    span class=(if x < WIDTH / 2.0 { "tip left" } else { "tip right" }) { (said) }
                }
            }
        }
    }
}

/// Along the top from the first day to the last, and back along the bottom.
fn region(days: impl Iterator<Item = (Date, f64, f64)>, frame: &Frame) -> String {
    let days: Vec<(Date, f64, f64)> = days.collect();
    let upper = days.iter().map(|(on, _, high)| (*on, *high));
    let lower = days.iter().rev().map(|(on, low, _)| (*on, *low));
    upper
        .chain(lower)
        .map(|(on, value)| format!("{:.1},{:.1}", frame.x(on), frame.y(value)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn line(run: &[Traced], frame: &Frame) -> String {
    run.iter()
        .map(|day| format!("{:.1},{:.1}", frame.x(day.on), frame.y(day.value)))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A value to as many places as the axis's step has; a mark a rounding error
/// below zero is zero, not "-0.0".
fn figure(value: f64, step: f64) -> String {
    let value = if value.abs() < step / 1_000.0 {
        0.0
    } else {
        value
    };
    if step < 0.1 {
        format!("{value:.2}")
    } else if step < 1.0 || step.fract() > 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    }
}

/// Hours from midnight as a clock reads them, the evening before included.
fn clock(hours: f64) -> String {
    // Plus zero, so minus zero is written as zero.
    let minutes = (hours * 60.0).round().rem_euclid(24.0 * 60.0) + 0.0;
    format!("{:02.0}:{:02.0}", (minutes / 60.0).floor(), minutes % 60.0)
}

/// The latest entries as a list, newest first.
pub fn latest(title: &str, heading: &str, rows: &[(String, Markup)]) -> Markup {
    html! {
        section.card.latest {
            div.head { h2 { (title) } span { (heading) } }
            @for (on, value) in rows {
                div.row { span.on { (on) } span.value { (value) } }
            }
        }
    }
}

/// The one line saying what the line and band are.
pub fn foot(said: &str) -> Markup {
    html! { p.foot { (said) } }
}

/// A day as the template writes it: its day and month, and its year where it
/// is not this one.
pub fn day(on: Date, today: Date) -> String {
    if on.year() == today.year() {
        on.strftime("%-d %b").to_string()
    } else {
        on.strftime("%-d %b %Y").to_string()
    }
}

/// The watch that recorded a series, as the pages name it.
pub fn name(sent_by: &SentBy) -> String {
    match sent_by {
        SentBy::Device(device) => device.name().to_string(),
        SentBy::Unknown => "device unknown".to_owned(),
    }
}

fn days_between(from: Date, to: Date) -> i32 {
    from.until(to).map_or(0, |span| span.get_days())
}

#[cfg(test)]
mod tests {
    use domain::analytical::{body_weight::Direction, range::Range};
    use jiff::civil::date;
    use maud::html;

    use super::{Axis, Cell, Chart, Layer, Metric, Reading, Segment, Traced, chart, summary};

    fn range() -> Range {
        Range {
            from: date(2026, 10, 1),
            to: date(2026, 10, 28),
        }
    }

    /// This macrocycle on 2026-11-02: 14 Sep is pushed in from the edge, and
    /// gives way to 21 Sep rather than hiding it.
    #[test]
    fn a_label_pushed_in_from_the_edge_gives_way() {
        let range = Range {
            from: date(2026, 9, 14),
            to: date(2026, 11, 2),
        };
        let frame = super::framed(range, Axis::Figures, 83.0, 86.0);
        let labels: Vec<String> = super::ticks(&frame)
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        assert_eq!(
            labels.first().map(String::as_str),
            Some("21 Sep"),
            "{labels:?}"
        );
        assert!(labels.contains(&"28 Sep".to_owned()), "{labels:?}");
    }

    #[test]
    fn a_change_against_the_noise_is_a_badge_a_figure_and_a_bar() {
        let shown = summary(
            &[Cell {
                metric: Metric::BodyWeight,
                label: "Trend",
                figure: "84.1".to_owned(),
                unit: "kg",
                margin: Some("0.3 kg".to_owned()),
                readings: vec![Reading::Change {
                    by: 0.21,
                    noise: 0.125,
                    direction: Some(Direction::Rising),
                    said: "+0.21 kg".to_owned(),
                    over: "in 7 days".to_owned(),
                }],
            }],
            "28 Oct",
        )
        .into_string();
        assert!(shown.contains(r#"class="cell weight""#), "{shown}");
        assert!(shown.contains("± 0.3 kg"), "{shown}");
        assert!(shown.contains(r#"class="badge""#), "{shown}");
        assert!(shown.contains(r#"class="noise""#), "the noise is shaded");
        assert!(shown.contains(r#"class="fell""#), "the change is a dot");
    }

    #[test]
    fn a_position_lights_the_segment_holding_it_and_no_rule_says_so() {
        let shown = summary(
            &[
                Cell {
                    metric: Metric::BodyWeight,
                    label: "Pulse wave velocity",
                    figure: "7.1".to_owned(),
                    unit: "m/s",
                    margin: None,
                    readings: vec![Reading::Position {
                        segments: vec![
                            Segment {
                                from: 4.0,
                                to: 7.0,
                                name: "low".to_owned(),
                            },
                            Segment {
                                from: 7.0,
                                to: 10.0,
                                name: "high".to_owned(),
                            },
                        ],
                        at: 7.1,
                    }],
                },
                Cell {
                    metric: Metric::BodyWeight,
                    label: "Next test",
                    figure: "–".to_owned(),
                    unit: "",
                    margin: None,
                    readings: vec![Reading::NoRule("Nothing predicts it yet".to_owned())],
                },
            ],
            "",
        )
        .into_string();
        assert_eq!(
            shown.matches(r#"class="segment lit""#).count(),
            1,
            "{shown}"
        );
        assert_eq!(shown.matches(r#"class="segment""#).count(), 1, "{shown}");
        assert!(shown.contains("Nothing predicts it yet"), "{shown}");
    }

    #[test]
    fn dots_are_never_joined_and_the_trend_draws_its_band_where_it_has_one() {
        let trend = |day, margin| Traced {
            on: date(2026, 10, day),
            value: 84.0,
            margin,
        };
        let shown = chart(&Chart {
            id: "weight",
            metric: Metric::BodyWeight,
            title: "Body weight",
            sub: "2 weigh-ins".to_owned(),
            range: range(),
            axis: Axis::Figures,
            layers: vec![
                Layer::Reference(vec![(date(2026, 10, 1), 80.0, 82.0)]),
                Layer::Trend(vec![
                    vec![trend(2, Some(0.3)), trend(3, Some(0.3))],
                    vec![trend(20, None), trend(21, None)],
                ]),
                Layer::Dots(vec![(date(2026, 10, 2), 84.3), (date(2026, 10, 20), 83.8)]),
            ],
            days: vec![
                (date(2026, 10, 2), html! { "2 Oct" }),
                (date(2026, 10, 20), html! { "20 Oct" }),
            ],
        })
        .into_string();
        assert_eq!(shown.matches(r#"class="dot""#).count(), 2, "{shown}");
        assert_eq!(shown.matches(r#"class="line""#).count(), 2, "{shown}");
        assert_eq!(shown.matches(r#"class="band""#).count(), 1, "{shown}");
        assert_eq!(shown.matches(r#"class="reference""#).count(), 1, "{shown}");
        assert_eq!(shown.matches(r#"type="radio""#).count(), 2, "{shown}");
        assert!(shown.contains(r#"class="tip left""#), "{shown}");
        assert!(shown.contains(r#"class="tip right""#), "{shown}");
    }

    #[test]
    fn a_clock_runs_down_the_chart_and_reads_as_one() {
        let frame = super::framed(range(), Axis::Clock, -1.5, 7.0);
        assert!(frame.y(-2.0) < frame.y(7.0), "the evening is at the top");
        assert!((frame.step - 2.0).abs() < f64::EPSILON, "{}", frame.step);
        assert_eq!(super::clock(-2.0), "22:00");
        assert_eq!(super::clock(6.5), "06:30");
        assert_eq!(super::clock(-24.0), "00:00", "never minus zero");
        assert_eq!(
            super::figure(7.5, 2.5),
            "7.5",
            "a step of 2.5 keeps its half"
        );
        assert_eq!(super::figure(10.0, 5.0), "10");
        assert_eq!(
            super::figure(-1e-17, 0.2),
            "0.0",
            "a mark a rounding error below zero"
        );
        let night_long = super::framed(range(), Axis::Clock, -6.0, 40.0);
        assert!(
            (night_long.step - 12.0).abs() < f64::EPSILON,
            "{}",
            night_long.step
        );
        let longer = super::framed(range(), Axis::Clock, -30.0, 60.0);
        assert!((longer.step - 24.0).abs() < f64::EPSILON, "{}", longer.step);
    }

    #[test]
    fn a_range_holding_nothing_says_so() {
        let shown = chart(&Chart {
            id: "weight",
            metric: Metric::BodyWeight,
            title: "Body weight",
            sub: "0 weigh-ins".to_owned(),
            range: range(),
            axis: Axis::Figures,
            layers: vec![Layer::Dots(Vec::new())],
            days: Vec::new(),
        })
        .into_string();
        assert!(shown.contains("Nothing in this range."), "{shown}");
        assert!(!shown.contains("<svg"), "{shown}");
    }
}
