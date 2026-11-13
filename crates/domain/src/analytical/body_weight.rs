//! Body weight as one time series (#250).
//!
//! **Every canonical weigh-in, once, in date order.** Body mass is
//! source-independent (§ 6), so the scale's readings and the ones typed into
//! a spreadsheet or MyFitnessPal are one series, and the canonical layer has
//! already made a weighing recorded twice one weigh-in (#409).
//!
//! **Ordered here, by the operator's day and then by the clock where one was
//! read**, so the series does not depend on the order a store returned rows in
//! (§ 9).
//!
//! **Nothing is filled in** (§ 37). A day with no weigh-in has no point, so a
//! gap in the record is a gap in the series.
//!
//! **The trend is the operator's, 2026-10-14** (#252): the mean of the
//! weigh-ins in the 7 days ending on a day, given at least 4 of those days have
//! one. The weigh-ins are discrete and the trend estimates a continuous mass,
//! so it is a line where they are points; a week spans one weekend whatever day
//! it ends on, which keeps the weekend's rise out of it. Where fewer than 4 days
//! were weighed there is no trend, and the line breaks rather than guessing
//! across (§ 37). It runs from the first weigh-in to the last, and never past.
//!
//! **Weight change is the rate the trend moved, in kg a week** (operator,
//! 2026-11-02, #688, #689): the trend centred 14 days after a day less the
//! trend centred 14 days before it, over the 4 weeks between. It is placed at
//! the middle of what it measures, so it follows the trend's slope rather than
//! lagging it by a fortnight, and the last 17 days of the record have none
//! yet. It replaced a 7-day reading at ± 0.125 kg, which was a quarter of the
//! noise of the difference it read. No word says what it means: the line
//! against zero and the aim shows it.
//!
//! **The aim is the operator's, 2026-10-14** (#252): to lose 0.25 kg a
//! microcycle in a cut. It is drawn only where a plan says what he intended,
//! each mesocycle in the phase assumed from the macrocycle (#489).
//!
//! **Its band is [`super::confidence`]'s**, σ about ± 0.43 kg on the
//! operator's record and steady at 0.3 to 0.5 kg in every year and source
//! from 2015. A day's weight barely follows the day before's (ρ₁ about 0.1),
//! so a full week's band is about ± 0.35 kg and 4 days' about ± 0.45 kg. The
//! weekend's rise is not noise in it: every week holds one weekend.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{
    Weighed,
    confidence::Noise,
    range::Range,
    recomposition::NutritionPhase,
    since_start::{SinceStart, since_start},
};
use crate::{macrocycle::Macrocycle, measure::Kg, plan::Span};

/// What the trend is called, so a different method is never mistaken for it.
pub const METHOD: &str = "mean of the 7 days ending on the day, at least 4 of them weighed";

/// How many days the trend is the mean of, the day itself included.
const WINDOW_DAYS: i64 = 7;

/// How many of them must have a weigh-in for there to be a trend.
const WEIGHED_DAYS: usize = 4;

/// How many days either side of a day its weight change reads the trend
/// (operator, 2026-11-02, #688).
const HALF_SPAN_DAYS: i64 = 14;

/// What weight change is called, so a different method is never mistaken for it.
pub const CHANGE_METHOD: &str =
    "the trend 14 days after less the trend 14 days before, a week at a time";

/// The operator's aim in a cut, a week (2026-10-14, #252).
pub const CUT_AIM_GRAMS_A_WEEK: i64 = -250;

/// Which way a trend went: the other series' readings use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Rising,
    Maintaining,
    Falling,
}

/// How fast the trend moved around one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightChange {
    /// The middle of the four weeks it measures.
    pub on: Date,
    /// Kilograms a week, negative when the trend fell.
    pub per_week: f64,
    /// How far either side its 95% band runs, in kilograms a week.
    pub margin: Option<f64>,
}

/// What the plan intended over one mesocycle, a week.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aim {
    pub span: Span,
    pub phase: NutritionPhase,
    /// Kilograms a week; `None` where the operator has stated none.
    pub per_week: Option<f64>,
}

/// Body weight's trend on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    pub on: Date,
    pub mass: Kg,
    /// How far either side its 95% band runs, in kilograms; `None` where the
    /// record is too short to measure the noise.
    pub margin: Option<f64>,
}

/// Every weigh-in, the trend through them, and how fast it moved.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Oldest first, as [`body_weight`] orders them.
    pub weigh_ins: Vec<Weighed>,
    /// As [`trend`] gives it.
    pub trend: Vec<Vec<Trend>>,
    /// As [`weight_change`] gives it.
    pub weight_change: Vec<Vec<WeightChange>>,
}

impl Series {
    pub fn of(weigh_ins: Vec<Weighed>) -> Self {
        let weigh_ins = body_weight(weigh_ins);
        let trend = trend(&weigh_ins);
        let weight_change = weight_change(&trend);
        Self {
            weigh_ins,
            trend,
            weight_change,
        }
    }
}

/// What a range comes to: the mean of its
/// weigh-ins and how far the trend moved across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub average: Kg,
    /// The last trend in the range less the first; `None` with fewer than two.
    pub difference: Option<i64>,
}

impl Series {
    /// Only what falls in `range`: weigh-ins and trend on its days, and the
    /// cycles that start in it.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        let inside = |day: Date| range.covers(day);
        Self {
            weigh_ins: self
                .weigh_ins
                .iter()
                .filter(|weighed| inside(weighed.at.day()))
                .cloned()
                .collect(),
            trend: self
                .trend
                .iter()
                .map(|run| {
                    run.iter()
                        .filter(|day| inside(day.on))
                        .copied()
                        .collect::<Vec<_>>()
                })
                .filter(|run| !run.is_empty())
                .collect(),
            weight_change: self
                .weight_change
                .iter()
                .map(|run| {
                    run.iter()
                        .filter(|day| inside(day.on))
                        .copied()
                        .collect::<Vec<_>>()
                })
                .filter(|run| !run.is_empty())
                .collect(),
        }
    }

    /// `None` when there is no weigh-in to average.
    pub fn summary(&self) -> Option<Summary> {
        let count = u64::try_from(self.weigh_ins.len())
            .ok()
            .filter(|count| *count > 0)?;
        let total: u64 = self
            .weigh_ins
            .iter()
            .map(|weighed| weighed.mass.as_grams())
            .sum();
        let mut days = self.trend.iter().flatten();
        let first = days.next();
        let difference = first.zip(days.last()).and_then(|(first, last)| {
            Some(
                i64::try_from(last.mass.as_grams()).ok()?
                    - i64::try_from(first.mass.as_grams()).ok()?,
            )
        });
        Some(Summary {
            average: Kg::from_grams((total + count / 2) / count),
            difference,
        })
    }
}

/// The body-weight series the canonical weigh-ins give, oldest first.
pub fn body_weight(mut weigh_ins: Vec<Weighed>) -> Vec<Weighed> {
    weigh_ins.sort_by_key(|weighed| {
        (
            weighed.at.day(),
            weighed.at.instant().map(|at| at.instant().as_second()),
        )
    });
    weigh_ins
}

/// The trend, oldest first, as unbroken runs of consecutive days.
///
/// A gap between two runs is where fewer than 4 of the 7 days were weighed.
pub fn trend(series: &[Weighed]) -> Vec<Vec<Trend>> {
    let by_day = by_day(series);
    let (Some(&first), Some(&last)) = (by_day.keys().next(), by_day.keys().next_back()) else {
        return Vec::new();
    };

    let kilograms: BTreeMap<Date, f64> = by_day
        .iter()
        .filter_map(|(&on, grams)| {
            let count = f64::from(u32::try_from(grams.len()).ok()?);
            let total = grams
                .iter()
                .map(|&gram| u32::try_from(gram).ok().map(f64::from))
                .sum::<Option<f64>>()?;
            Some((on, total / count / 1_000.0))
        })
        .collect();
    let noise = Noise::of(&kilograms, WINDOW_DAYS, WEIGHED_DAYS);

    let mut runs: Vec<Vec<Trend>> = Vec::new();
    let mut run: Vec<Trend> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        match mean_of_week(&by_day, on) {
            Some(mass) => run.push(Trend {
                on,
                mass,
                margin: noise
                    .as_ref()
                    .and_then(|noise| noise.margin(&kilograms, on)),
            }),
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// The mean of the weigh-ins in the week ending on `on`, if enough days of it
/// were weighed.
fn mean_of_week(by_day: &BTreeMap<Date, Vec<u64>>, on: Date) -> Option<Kg> {
    let start = on
        .checked_sub(jiff::Span::new().days(WINDOW_DAYS - 1))
        .ok()?;
    let week: Vec<&Vec<u64>> = by_day.range(start..=on).map(|(_, grams)| grams).collect();
    if week.len() < WEIGHED_DAYS {
        return None;
    }
    let grams: Vec<u64> = week.into_iter().flatten().copied().collect();
    let count = u64::try_from(grams.len()).ok()?;
    let total: u64 = grams.iter().sum();
    Some(Kg::from_grams((total + count / 2) / count))
}

/// Weight change, oldest first, as unbroken runs of consecutive days.
///
/// The trend is the mean of the week *ending* on a day, so the trend centred
/// on a day is the one 3 days after it. A day has a weight change wherever
/// the trends centred 14 days either side of it both exist; their bands are
/// independent, 4 weeks apart, so they add in quadrature.
pub fn weight_change(trend: &[Vec<Trend>]) -> Vec<Vec<WeightChange>> {
    let by_day: BTreeMap<Date, Trend> = trend.iter().flatten().map(|day| (day.on, *day)).collect();
    let centre = (WINDOW_DAYS - 1) / 2;
    let weeks = f64::from(i32::try_from(2 * HALF_SPAN_DAYS).unwrap_or(28)) / 7.0;
    let mut runs: Vec<Vec<WeightChange>> = Vec::new();
    let mut run: Vec<WeightChange> = Vec::new();
    for (&ending, after) in &by_day {
        let read = || {
            let on = ending
                .checked_sub(jiff::Span::new().days(HALF_SPAN_DAYS + centre))
                .ok()?;
            let before = by_day.get(
                &ending
                    .checked_sub(jiff::Span::new().days(2 * HALF_SPAN_DAYS))
                    .ok()?,
            )?;
            let grams = i64::try_from(after.mass.as_grams()).ok()?
                - i64::try_from(before.mass.as_grams()).ok()?;
            let kilograms = f64::from(i32::try_from(grams).ok()?) / 1_000.0;
            Some(WeightChange {
                on,
                per_week: kilograms / weeks,
                margin: after
                    .margin
                    .zip(before.margin)
                    .map(|(after, before)| after.hypot(before) / weeks),
            })
        };
        match read() {
            Some(change) => {
                let follows = run
                    .last()
                    .is_none_or(|last| last.on.tomorrow().is_ok_and(|next| next == change.on));
                if !follows {
                    runs.push(std::mem::take(&mut run));
                }
                run.push(change);
            }
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// What the plan intended over each of the macrocycle's mesocycles that has
/// started by `today`, oldest first.
///
/// **A mesocycle's phase is the macrocycle's on its first day**, assumed as
/// [`super::recomposition`] assumes it. A week no mesocycle covers has no aim.
#[must_use]
pub fn aims(macrocycle: Macrocycle, mesocycles: &[Span], today: Date) -> Vec<Aim> {
    let started = macrocycle.start()..=macrocycle.last().min(today);
    let mut spans: Vec<Span> = mesocycles
        .iter()
        .copied()
        .filter(|span| started.contains(&span.start()))
        .collect();
    spans.sort_by_key(|span| (span.start(), span.end()));
    spans.dedup();
    spans
        .into_iter()
        .map(|span| {
            let phase = NutritionPhase::assumed(macrocycle.phase(span.start(), Some(span)));
            let per_week = match phase {
                NutritionPhase::Cut => i32::try_from(CUT_AIM_GRAMS_A_WEEK)
                    .ok()
                    .map(|grams| f64::from(grams) / 1_000.0),
                NutritionPhase::Maintenance => Some(0.0),
                NutritionPhase::Bulk => None,
            };
            Aim {
                span,
                phase,
                per_week,
            }
        })
        .collect()
}

fn by_day(series: &[Weighed]) -> BTreeMap<Date, Vec<u64>> {
    let mut by_day: BTreeMap<Date, Vec<u64>> = BTreeMap::new();
    for weighed in series {
        by_day
            .entry(weighed.at.day())
            .or_default()
            .push(weighed.mass.as_grams());
    }
    by_day
}

/// Body weight's trend on one day, and where the plan's aims would have
/// taken it by then.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aimed {
    pub trend: Trend,
    /// In kilograms; `None` once a day falls under a mesocycle with no aim
    /// stated, as a bulk has none.
    pub expected: Option<f64>,
}

/// Body weight over the macrocycle so far, against the plan's aims (#769).
#[derive(Debug, Clone, PartialEq)]
pub struct Weight {
    /// Unbroken runs of the trend from the macrocycle's start, oldest first.
    pub days: Vec<Vec<Aimed>>,
    /// The trend's first day in the macrocycle against its latest, in
    /// kilograms.
    pub change: SinceStart,
    /// The change the aims expect over the same days, in kilograms.
    pub expected: Option<f64>,
}

impl Weight {
    /// Where the aims would have taken the trend, day by day: one run per
    /// stretch where it is known, so a chart draws a line that breaks where
    /// the trend does or an aim states none.
    #[must_use]
    pub fn expected(&self) -> Vec<Vec<(Date, f64)>> {
        let mut runs: Vec<Vec<(Date, f64)>> = Vec::new();
        for run in &self.days {
            let mut open = false;
            for day in run {
                match day.expected {
                    Some(expected) if open => {
                        if let Some(last) = runs.last_mut() {
                            last.push((day.trend.on, expected));
                        }
                    }
                    Some(expected) => {
                        runs.push(vec![(day.trend.on, expected)]);
                        open = true;
                    }
                    None => open = false,
                }
            }
        }
        runs
    }
}

/// The trend from the macrocycle that `began` on a day to `today`, and where
/// the aims would have taken it from its first day there.
///
/// **Each day adds its mesocycle's aim, a seventh of a week's**, from the
/// trend on the first day. A day no aim covers adds nothing (agreed with the
/// operator, 2026-11-12), and a day whose aim states none leaves the rest of
/// the line unknown. `None` where fewer than two trend days fall in it.
#[must_use]
pub fn against_aims(
    trend: &[Vec<Trend>],
    aims: &[Aim],
    began: Date,
    today: Date,
) -> Option<Weight> {
    let days: Vec<Vec<Trend>> = trend
        .iter()
        .map(|run| {
            run.iter()
                .filter(|day| day.on >= began && day.on <= today)
                .copied()
                .collect::<Vec<_>>()
        })
        .filter(|run| !run.is_empty())
        .collect();
    let first = *days.first()?.first()?;
    let kg = |day: &Trend| kilograms(day.mass);
    let change = since_start(
        days.iter()
            .flatten()
            .filter_map(|day| Some((day.on, kg(day)?, day.margin))),
        began,
        today,
    )?;
    let mut expected = Some(change.from);
    let mut on = first.on;
    let mut aimed = Vec::new();
    for run in &days {
        let mut aimed_run = Vec::new();
        for day in run {
            while on < day.on {
                on = on.tomorrow().ok()?;
                expected = expected.and_then(|mass| {
                    aims.iter()
                        .find(|aim| aim.span.covers(on))
                        .map_or(Some(mass), |aim| {
                            aim.per_week.map(|per_week| mass + per_week / 7.0)
                        })
                });
            }
            aimed_run.push(Aimed {
                trend: *day,
                expected,
            });
        }
        aimed.push(aimed_run);
    }
    let last = aimed.last()?.last()?.expected;
    Some(Weight {
        days: aimed,
        change,
        expected: last.map(|last| last - change.from),
    })
}

fn kilograms(mass: Kg) -> Option<f64> {
    u32::try_from(mass.as_grams())
        .ok()
        .map(|grams| f64::from(grams) / 1_000.0)
}

#[cfg(test)]
mod against_aims_tests {
    use jiff::civil::{Date, date};

    use super::{Aim, Trend, against_aims};
    use crate::{analytical::recomposition::NutritionPhase, measure::Kg, plan::Span};

    const BEGAN: Date = date(2026, 9, 14);

    fn trend(from: Date, to: Date) -> Vec<Trend> {
        from.series(jiff::Span::new().days(1))
            .take_while(|on| *on <= to)
            .map(|on| Trend {
                on,
                mass: Kg::from_grams(84_600),
                margin: Some(0.4),
            })
            .collect()
    }

    fn cut(start: Date, weeks: u32) -> Aim {
        Aim {
            span: Span::new(start, weeks),
            phase: NutritionPhase::Cut,
            per_week: Some(-0.25),
        }
    }

    /// **Autumn 2026**: a cut from 28 September, nothing aimed in the
    /// fortnight before it. Four weeks of the cut expect a kilogram.
    #[test]
    fn each_day_under_an_aim_adds_a_seventh_of_it() {
        let today = date(2026, 10, 26);
        let runs = vec![trend(BEGAN, today)];
        let weight =
            against_aims(&runs, &[cut(date(2026, 9, 28), 4)], BEGAN, today).expect("a trend");

        let expected = weight.expected.expect("an aim throughout");
        assert!((expected + 1.0).abs() < 1e-9, "{expected}");
        let first = weight.days[0][0];
        assert_eq!(first.expected, Some(84.6));
        let before_the_cut = weight.days[0][14];
        assert_eq!(before_the_cut.trend.on, date(2026, 9, 28));
        assert!(
            (before_the_cut.expected.expect("aimed") - (84.6 - 0.25 / 7.0)).abs() < 1e-9,
            "{before_the_cut:?}"
        );
        assert!(weight.change.by().abs() < 1e-9);
    }

    #[test]
    fn a_mesocycle_with_no_aim_leaves_the_rest_unknown() {
        let today = date(2026, 10, 12);
        let bulk = Aim {
            span: Span::new(date(2026, 9, 28), 4),
            phase: NutritionPhase::Bulk,
            per_week: None,
        };
        let weight = against_aims(&[trend(BEGAN, today)], &[bulk], BEGAN, today).expect("a trend");
        assert_eq!(weight.expected, None);
        assert_eq!(weight.days[0][13].expected, Some(84.6));
        assert_eq!(weight.days[0][14].expected, None);

        let expected = weight.expected();
        assert_eq!(expected.len(), 1, "the line stops at the bulk");
        assert_eq!(expected[0].len(), 14);
        assert_eq!(expected[0].last(), Some(&(date(2026, 9, 27), 84.6)));
    }

    #[test]
    fn no_trend_in_the_macrocycle_is_nothing_to_read() {
        let runs = vec![trend(date(2026, 8, 1), date(2026, 9, 1))];
        assert_eq!(against_aims(&runs, &[], BEGAN, date(2026, 11, 12)), None);
    }
}
