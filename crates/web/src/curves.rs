//! Where the plan stands on the volume and intensity curves (#742): one chart
//! per discipline across the macrocycle's weeks.
//!
//! Volume is bars and intensity a line. The current mesocycle is shaded, this
//! week is marked, test weeks carry a T and lost weeks a dashed outline. **The
//! weeks not yet committed are outlined**, because they are what the plan is
//! expected to be rather than what it is (operator, 2026-11-09).

use std::fmt::Write as _;

use application::next::{MacrocycleStanding, PlanCurves};
use domain::curve::{OnCurve, Point, Week};
use maud::{Markup, PreEscaped, html};

const WIDTH: f64 = 354.0;
const LEFT: f64 = 4.0;
const RIGHT: f64 = 320.0;
const TOP: f64 = 14.0;
const BASE: f64 = 128.0;
const HEIGHT: f64 = 150.0;

/// The card, or nothing where there is no macrocycle or no plan to draw.
pub fn card(
    macrocycle: &MacrocycleStanding,
    curves: &Result<Option<PlanCurves>, application::commit::CommitError>,
) -> Markup {
    let MacrocycleStanding::Week {
        macrocycle,
        week,
        of,
    } = macrocycle
    else {
        return html! {};
    };
    let heading = format!(
        "{}, week {week} of {of}",
        capitalised(&macrocycle.to_string())
    );
    match curves {
        Ok(None) => html! {},
        Err(error) => html! {
            section.card.curves {
                h2 { (heading) }
                p.failed { "the rest of the term could not be worked out: " (error) }
            }
        },
        Ok(Some(curves)) => html! {
            section.card.curves {
                h2 { (heading) }
                h3.gym { "Gym" }
                (chart("gym", &curves.curves.gym, curves))
                h3.cycling { "Cycling" }
                (chart("cycling", &curves.curves.cycling, curves))
                p.key {
                    span { i.volume {} "volume" }
                    span { i.intensity {} "intensity" }
                    span { i.expected {} "expected" }
                    span { i.lost {} "lost week" }
                    span { b { "T" } " test" }
                }
            }
        },
    }
}

fn capitalised(words: &str) -> String {
    let mut characters = words.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// Where a chart puts a week, a volume and an intensity.
struct Layout {
    /// What volume is counted in.
    unit: &'static str,
    slot: f64,
    most: f64,
    low: f64,
    high: f64,
}

impl Layout {
    fn of(weeks: &[Week], unit: &'static str) -> Self {
        let points: Vec<Point> = weeks.iter().filter_map(|week| point_of(week.on)).collect();
        let (low, high) = intensity_range(&points);
        #[expect(
            clippy::cast_precision_loss,
            reason = "a count of weeks in a term, far inside f64's exact range"
        )]
        let slot = (RIGHT - LEFT) / weeks.len().max(1) as f64;
        Self {
            unit,
            slot,
            most: points
                .iter()
                .map(|point| point.volume)
                .fold(0.0_f64, f64::max),
            low,
            high,
        }
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "an index of a week in a term, far inside f64's exact range"
    )]
    fn centre(&self, at: usize) -> f64 {
        self.slot.mul_add(at as f64 + 0.5, LEFT)
    }

    fn bar(&self) -> f64 {
        self.slot * 0.68
    }

    fn volume(&self, volume: f64) -> f64 {
        if self.most > 0.0 {
            (BASE - TOP).mul_add(-volume / self.most, BASE)
        } else {
            BASE
        }
    }

    fn intensity(&self, intensity: f64) -> f64 {
        (BASE - TOP).mul_add(-(intensity - self.low) / (self.high - self.low), BASE)
    }
}

/// One discipline's chart.
fn chart(class: &str, weeks: &[Week], standing: &PlanCurves) -> Markup {
    let unit = if class == "cycling" { "min" } else { "reps" };
    let layout = Layout::of(weeks, unit);
    let mut svg =
        format!(r#"<svg viewBox="0 0 {WIDTH} {HEIGHT}" class="curves {class}" role="img">"#);
    shade_mesocycle(&mut svg, &layout, weeks, standing);
    axis(&mut svg, &layout);
    for (at, week) in weeks.iter().enumerate() {
        mark(&mut svg, &layout, at, week, weeks.len(), standing);
    }
    intensity_line(&mut svg, &layout, weeks);
    if let Some(at) = weeks
        .iter()
        .position(|week| standing.commencing == Some(week.monday))
    {
        let x = layout.centre(at);
        let _ = write!(
            svg,
            r#"<line class="today" x1="{x:.1}" x2="{x:.1}" y1="4" y2="{BASE}"/>"#
        );
    }
    svg.push_str("</svg>");
    PreEscaped(svg)
}

/// The current mesocycle, shaded.
fn shade_mesocycle(svg: &mut String, layout: &Layout, weeks: &[Week], standing: &PlanCurves) {
    let Some(mesocycle) = standing.mesocycle else {
        return;
    };
    let inside: Vec<usize> = (0..weeks.len())
        .filter(|at| {
            weeks
                .get(*at)
                .is_some_and(|week| mesocycle.covers(week.monday))
        })
        .collect();
    let (Some(first), Some(last)) = (inside.first(), inside.last()) else {
        return;
    };
    let x = layout.centre(*first) - layout.slot / 2.0;
    let width = layout.centre(*last) - layout.centre(*first) + layout.slot;
    let _ = write!(
        svg,
        r#"<rect class="current" x="{x:.1}" y="4" width="{width:.1}" height="{:.1}" rx="6"/>"#,
        BASE - 4.0
    );
}

/// The intensity axis: its lowest, middle and highest lines.
fn axis(svg: &mut String, layout: &Layout) {
    for value in [
        layout.low,
        f64::midpoint(layout.low, layout.high),
        layout.high,
    ] {
        let y = layout.intensity(value);
        let _ = write!(
            svg,
            r#"<line class="grid" x1="{LEFT}" x2="{RIGHT}" y1="{y:.1}" y2="{y:.1}"/><text class="tick" x="{}" y="{:.1}">{}%</text>"#,
            RIGHT + 4.0,
            y + 4.0,
            figure(value)
        );
    }
}

/// A week's bar, its test or its lost outline, and its number where it is
/// labelled.
fn mark(
    svg: &mut String,
    layout: &Layout,
    at: usize,
    week: &Week,
    of: usize,
    standing: &PlanCurves,
) {
    let x = layout.centre(at);
    let bar = layout.bar();
    let expected = if week.expected { " expected" } else { "" };
    match week.on {
        OnCurve::Microcycle { point, tests } => {
            let mut top = BASE;
            if let Some(point) = point {
                top = layout.volume(point.volume);
                let _ = write!(
                    svg,
                    r#"<rect class="volume{expected}" x="{:.1}" y="{top:.1}" width="{bar:.1}" height="{:.1}" rx="2"><title>week {}: {} {}, {}%</title></rect>"#,
                    x - bar / 2.0,
                    BASE - top,
                    at + 1,
                    figure(point.volume),
                    layout.unit,
                    figure(point.intensity)
                );
            }
            if tests {
                let _ = write!(
                    svg,
                    r#"<text class="test" x="{x:.1}" y="{:.1}" text-anchor="middle">T</text>"#,
                    (top - 4.0).max(TOP)
                );
            }
        }
        OnCurve::Lost => {
            let _ = write!(
                svg,
                r#"<rect class="lost" x="{:.1}" y="{TOP}" width="{bar:.1}" height="{:.1}" rx="2"/>"#,
                x - bar / 2.0,
                BASE - TOP
            );
        }
        OnCurve::Holding | OnCurve::Unplanned => {}
    }
    let current = standing.commencing == Some(week.monday);
    let number = at + 1;
    if current || number == 1 || number == of || number.is_multiple_of(5) {
        let class = if current { "week current" } else { "week" };
        let _ = write!(
            svg,
            r#"<text class="{class}" x="{x:.1}" y="{:.1}" text-anchor="middle">{number}</text>"#,
            HEIGHT - 6.0
        );
    }
}

/// The intensity line, broken wherever a week has no point, and dashed where
/// it is expected.
fn intensity_line(svg: &mut String, layout: &Layout, weeks: &[Week]) {
    let mut runs: Vec<Vec<(f64, f64, bool)>> = vec![Vec::new()];
    for (at, week) in weeks.iter().enumerate() {
        match point_of(week.on) {
            Some(point) => {
                if let Some(run) = runs.last_mut() {
                    run.push((
                        layout.centre(at),
                        layout.intensity(point.intensity),
                        week.expected,
                    ));
                }
            }
            None => runs.push(Vec::new()),
        }
    }
    for run in &runs {
        for pair in run.windows(2) {
            if let [(x1, y1, _), (x2, y2, expected)] = pair {
                let class = if *expected {
                    "intensity expected"
                } else {
                    "intensity"
                };
                let _ = write!(
                    svg,
                    r#"<line class="{class}" x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}"/>"#
                );
            }
        }
        for (x, y, expected) in run {
            let class = if *expected { "dot expected" } else { "dot" };
            let _ = write!(
                svg,
                r#"<circle class="{class}" cx="{x:.1}" cy="{y:.1}" r="3.5"/>"#
            );
        }
    }
}

const fn point_of(on: OnCurve) -> Option<Point> {
    match on {
        OnCurve::Microcycle { point, .. } => point,
        OnCurve::Lost | OnCurve::Holding | OnCurve::Unplanned => None,
    }
}

/// The intensity axis: tens either side of every point, with room above and
/// below.
fn intensity_range(points: &[Point]) -> (f64, f64) {
    let lowest = points
        .iter()
        .map(|point| point.intensity)
        .fold(f64::INFINITY, f64::min);
    let highest = points
        .iter()
        .map(|point| point.intensity)
        .fold(f64::NEG_INFINITY, f64::max);
    if !lowest.is_finite() || !highest.is_finite() {
        return (70.0, 100.0);
    }
    let low = ((lowest - 2.5) / 10.0).floor() * 10.0;
    let high = (((highest + 2.5) / 10.0).ceil() * 10.0).max(low + 10.0);
    (low, high)
}

fn figure(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::intensity_range;
    use domain::curve::Point;

    /// **The gym's SBS week runs 75 to 90%, so its axis is 70 to 100.**
    #[test]
    fn the_intensity_axis_runs_in_tens_around_every_point() {
        let points = [75.0, 80.0, 85.0, 90.0].map(|intensity| Point {
            volume: 1.0,
            intensity,
        });
        assert_eq!(intensity_range(&points), (70.0, 100.0));
    }
}
