//! Where the plan's aims would have taken a trend, measured afresh from each
//! mesocycle's start (#786).
//!
//! **Reset at every mesocycle** (operator, 2026-11-13): measured from the
//! macrocycle's start, a mesocycle that went well would still read as off the
//! line because of one before it that did not. So each mesocycle's line starts
//! from the trend on its first day, and each mesocycle is read on its own.
//!
//! Body weight, fat and fat-free mass all read it, each with its own share of
//! the aim ([`crate::analytical::composition::aimed`]), so every chart and
//! tile draws one expectation rather than a version of its own.

use jiff::civil::Date;

use super::body_weight::Aim;

/// One day's expectation: where the aims would have taken the trend since
/// `since`, the first trend day of its mesocycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Expected {
    pub on: Date,
    pub value: f64,
    pub since: Date,
}

/// Each trend day's expectation, from `began` to `today`, in runs.
///
/// A chart draws each run as one line: broken at each mesocycle's start,
/// where it resets to the trend, and wherever the trend breaks or an aim
/// states none.
///
/// **Each day adds its mesocycle's aim, a seventh of a week's**, as `per_week`
/// reads it for the series. A day no aim covers adds nothing (agreed with the
/// operator, 2026-11-12), and a day whose aim states none has no expectation
/// until the next mesocycle starts.
#[must_use]
pub fn each_mesocycle(
    trend: &[Vec<(Date, f64)>],
    aims: &[Aim],
    began: Date,
    today: Date,
    per_week: impl Fn(&Aim) -> Option<f64>,
) -> Vec<Vec<Expected>> {
    let covering = |on: Date| aims.iter().find(|aim| aim.span.covers(on));
    let mut runs: Vec<Vec<Expected>> = Vec::new();
    // The mesocycle being read, the day measured from, and where the line is.
    let mut at: Option<(Option<Date>, Date, Option<f64>, Date)> = None;
    for run in trend {
        let mut open = false;
        for &(on, value) in run.iter().filter(|(on, _)| began <= *on && *on <= today) {
            let mesocycle = covering(on).map(|aim| aim.span.start());
            let next = match at {
                Some((reading, since, line, last))
                    if mesocycle.is_none() || mesocycle == reading =>
                {
                    let mut line = line;
                    let mut day = last;
                    while day < on {
                        let Ok(tomorrow) = day.tomorrow() else { break };
                        day = tomorrow;
                        line = line.and_then(|line| {
                            covering(day).map_or(Some(line), |aim| {
                                per_week(aim).map(|per_week| line + per_week / 7.0)
                            })
                        });
                    }
                    (reading, since, line, on)
                }
                _ => {
                    open = false;
                    let stated = covering(on).is_none_or(|aim| per_week(aim).is_some());
                    (mesocycle, on, stated.then_some(value), on)
                }
            };
            at = Some(next);
            let (_, since, line, _) = next;
            match line {
                Some(line) => {
                    let expected = Expected {
                        on,
                        value: line,
                        since,
                    };
                    match runs.last_mut() {
                        Some(last) if open => last.push(expected),
                        _ => runs.push(vec![expected]),
                    }
                    open = true;
                }
                None => open = false,
            }
        }
    }
    runs
}

/// What the aims expect a trend to move between two days, from `from`'s
/// level: each day after `from` to `to` adds its mesocycle's aim, a seventh
/// of a week's. `None` where a day's aim states none.
#[must_use]
pub fn over(
    aims: &[Aim],
    from: Date,
    to: Date,
    per_week: impl Fn(&Aim) -> Option<f64>,
) -> Option<f64> {
    let mut moved = 0.0;
    let mut day = from;
    while day < to {
        day = day.tomorrow().ok()?;
        if let Some(aim) = aims.iter().find(|aim| aim.span.covers(day)) {
            moved += per_week(aim)? / 7.0;
        }
    }
    Some(moved)
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{each_mesocycle, over};
    use crate::{
        analytical::{body_weight::Aim, nutrition_phase::NutritionPhase},
        plan::Span,
    };

    fn cut(start: Date, weeks: u32) -> Aim {
        Aim {
            span: Span::new(start, weeks),
            phase: NutritionPhase::Cut,
            per_week: Some(-0.7),
        }
    }

    fn days(from: Date, to: Date, value: impl Fn(Date) -> f64) -> Vec<(Date, f64)> {
        from.series(jiff::Span::new().days(1))
            .take_while(|on| *on <= to)
            .map(|on| (on, value(on)))
            .collect()
    }

    /// **Each mesocycle starts from its own trend**: a second mesocycle that
    /// begins a kilogram above the first's line is measured from where it
    /// began, not from the first's line.
    #[test]
    fn the_line_resets_at_each_mesocycle() {
        let began = date(2026, 9, 28);
        let today = date(2026, 11, 8);
        let second = date(2026, 10, 26);
        let trend = vec![days(
            began,
            today,
            |on| if on < second { 84.0 } else { 85.0 },
        )];
        let aims = [cut(began, 4), cut(second, 4)];

        let runs = each_mesocycle(&trend, &aims, began, today, |aim| aim.per_week);

        assert_eq!(runs.len(), 2, "broken where it resets: {runs:?}");
        let first = runs[0].last().expect("a day");
        assert_eq!(first.on, date(2026, 10, 25));
        assert!(
            (first.value - 0.1f64.mul_add(-27.0, 84.0)).abs() < 1e-9,
            "{first:?}"
        );
        assert!((runs[1][0].value - 85.0).abs() < 1e-9);
        assert_eq!(runs[1][0].since, second);
        let last = runs[1].last().expect("a day");
        assert!(
            (last.value - 0.1f64.mul_add(-13.0, 85.0)).abs() < 1e-9,
            "{last:?}"
        );
    }

    /// **A share of none holds the trend's level**: fat-free mass in a cut.
    #[test]
    fn a_share_of_nothing_holds() {
        let began = date(2026, 9, 28);
        let today = date(2026, 10, 4);
        let trend = vec![days(began, today, |_| 65.5)];
        let runs = each_mesocycle(&trend, &[cut(began, 4)], began, today, |aim| {
            aim.per_week.map(|_| 0.0)
        });
        assert!(runs[0].iter().all(|day| (day.value - 65.5).abs() < 1e-9));
    }

    /// **The whole macrocycle's aim** is each day's, without resetting.
    #[test]
    fn over_adds_every_aimed_day() {
        let aims = [cut(date(2026, 9, 28), 4)];
        let moved = over(&aims, date(2026, 9, 14), date(2026, 10, 25), |aim| {
            aim.per_week
        })
        .expect("aimed");
        assert!((moved + 2.8).abs() < 1e-9, "{moved}");
    }
}
