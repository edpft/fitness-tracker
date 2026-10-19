//! Body composition, one series per method (#256).
//!
//! **Read from the normalised layer** (§ 4): a composition figure is the
//! instrument's own, so two sources' figures are two series and never a second
//! account of one, and there is nothing to merge.
//!
//! **A method is the source and its algorithm** (§ 6). The Body Scan's first
//! weigh-in was made by a different algorithm from every one after it, so it is
//! a series of its own.
//!
//! **Fat and water are charted as a percentage of the weigh-in's mass**
//! (operator, 2026-10-17): the spreadsheets state one, and the Body Scan's is
//! its stated mass over the mass the same step weighed. Every other component
//! is charted as a mass, the one thing its source states.
//!
//! **The trend is body weight's** (operator, 2026-10-18, #419): the mean of
//! the figures in the 7 days ending on a day, given at least 4 of those days
//! have one, per method and per component. Fewer than 4 and the line breaks
//! (§ 37).
//!
//! **Each day's trend is read against the trend 28 days earlier**, not 7: over
//! the Body Scan's first ten months the 7-day reading moved in 7 of 35 weeks
//! and flipped week to week, where 28 days showed the losses and gains. A
//! change is rising or falling only beyond the least significant change at
//! 95% (ISCD's 2.77 × precision error, that is 1.96 × √2) applied to two
//! means: 1.96 × σ × √(1/n₁ + 1/n₀), where σ is the method's own scatter
//! between consecutive days over its whole record, not the vendor's figure
//! (operator, 2026-10-16), and n is the figures in each mean.
//!
//! **Copies of one workbook are one reading.** The same day sits in up to four
//! copies of a sheet, and they never disagree; a day where two did would show
//! both figures rather than choose.

use std::{collections::BTreeMap, fmt};

use jiff::civil::Date;

use super::range::{Range, runs_within};
use crate::{
    body::{Component, Percentage},
    measure::Kg,
};

pub use super::{body_weight::Direction, per_device::Reading};

/// What the trend is called, so a different method is never mistaken for it.
pub const TREND: &str = "mean of the 7 days ending on the day, at least 4 of them measured";

/// How many days the trend is the mean of, the day itself included.
pub(super) const WINDOW_DAYS: i64 = 7;

/// How many of them must have a figure for there to be a trend.
const MEASURED_DAYS: usize = 4;

/// How far back a day's trend is read against.
pub const AGAINST_DAYS: i64 = 28;

/// The two-sided 95% point of the normal distribution.
pub(super) const CONFIDENCE: f64 = 1.96;

/// What measured a composition figure, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// A Withings Body Scan, under the algorithm it states.
    BodyScan { algorithm: u64 },
    /// The operator's historical spreadsheets, typed in off a scale.
    Spreadsheets,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BodyScan { algorithm } => write!(f, "Body Scan, algorithm {algorithm}"),
            Self::Spreadsheets => f.write_str("spreadsheets"),
        }
    }
}

/// How a source stated a component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stated {
    Mass(Kg),
    Percentage(Percentage),
}

/// One figure as the normalised layer holds it, beside the weigh-in it was
/// measured against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured {
    pub on: Date,
    pub method: Method,
    pub component: Component,
    pub stated: Stated,
    /// The mass the same step weighed.
    pub weighed: Kg,
}

/// What a component is charted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Percentage,
    Kilograms,
}

impl Unit {
    pub const fn of(component: Component) -> Self {
        match component {
            Component::Fat | Component::Water => Self::Percentage,
            Component::FatFree
            | Component::Muscle
            | Component::SkeletalMuscle
            | Component::Lean
            | Component::ExtracellularWater
            | Component::IntracellularWater
            | Component::Bone
            | Component::Protein
            | Component::Minerals => Self::Kilograms,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Percentage => "%",
            Self::Kilograms => "kg",
        }
    }
}

/// A component's trend on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    pub on: Date,
    pub value: f64,
    /// How many figures it is the mean of.
    pub figures: usize,
}

/// The trend on a day against the trend [`AGAINST_DAYS`] earlier.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub on: Date,
    /// Positive when the trend rose, in the component's unit.
    pub by: f64,
    /// How far either way is within the noise.
    pub noise: f64,
    pub direction: Direction,
}

/// Every figure one method gave for one component, in date order, the trend
/// through them and which way it went.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSeries {
    pub method: Method,
    pub readings: Vec<Reading<f64>>,
    /// Unbroken runs of consecutive days, oldest first.
    pub trend: Vec<Vec<Trend>>,
    /// Oldest first, wherever a day and the day [`AGAINST_DAYS`] before it
    /// both have a trend and the method's scatter is known.
    pub changes: Vec<Change>,
}

impl MethodSeries {
    /// The readings, and the trend and changes they give.
    #[must_use]
    pub fn of(method: Method, readings: Vec<Reading<f64>>) -> Self {
        let by_day = by_day(&readings);
        let trend = trend(&by_day);
        let changes = scatter(&by_day).map_or_else(Vec::new, |sigma| changes(&trend, sigma));
        Self {
            method,
            readings,
            trend,
            changes,
        }
    }

    /// Only `range`'s days. The trend and each change on them are the whole
    /// record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            method: self.method,
            readings: self
                .readings
                .iter()
                .filter(|reading| range.covers(reading.on))
                .copied()
                .collect(),
            trend: runs_within(&self.trend, range, |day| day.on),
            changes: self
                .changes
                .iter()
                .filter(|change| range.covers(change.on))
                .copied()
                .collect(),
        }
    }
}

/// One component, one series per method.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentSeries {
    pub component: Component,
    pub unit: Unit,
    pub series: Vec<MethodSeries>,
}

impl ComponentSeries {
    /// Only `range`'s days, every method kept.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            component: self.component,
            unit: self.unit,
            series: self.series.iter().map(|one| one.within(range)).collect(),
        }
    }
}

/// The order components are shown in: what more than one source states first.
const ORDER: [Component; 11] = [
    Component::Fat,
    Component::Water,
    Component::FatFree,
    Component::Muscle,
    Component::SkeletalMuscle,
    Component::Lean,
    Component::Bone,
    Component::ExtracellularWater,
    Component::IntracellularWater,
    Component::Protein,
    Component::Minerals,
];

/// A figure in the unit its component is charted in, or nothing where it
/// cannot be.
fn charted(measured: &Measured, unit: Unit) -> Option<f64> {
    let grams = |mass: Kg| u32::try_from(mass.as_grams()).ok().map(f64::from);
    match (unit, measured.stated) {
        (Unit::Percentage, Stated::Percentage(percentage)) => {
            Some(f64::from(percentage.as_tenths()) / 10.0)
        }
        (Unit::Percentage, Stated::Mass(mass)) => {
            let weighed = grams(measured.weighed).filter(|weighed| *weighed > 0.0)?;
            Some(grams(mass)? / weighed * 100.0)
        }
        (Unit::Kilograms, Stated::Mass(mass)) => Some(grams(mass)? / 1_000.0),
        // Never a mass worked out from a percentage: a figure no source stated.
        (Unit::Kilograms, Stated::Percentage(_)) => None,
    }
}

/// Every component anything measured, one series per method, each in date
/// order and the earliest method first.
#[must_use]
pub fn by_method(measured: &[Measured]) -> Vec<ComponentSeries> {
    ORDER
        .iter()
        .filter_map(|&component| {
            let unit = Unit::of(component);
            let mut figures: Vec<(Method, Date, f64)> = measured
                .iter()
                .filter(|one| one.component == component)
                .filter_map(|one| charted(one, unit).map(|value| (one.method, one.on, value)))
                .collect();
            figures.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)).then(a.2.total_cmp(&b.2)));
            figures.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1 && a.2.total_cmp(&b.2).is_eq());

            let mut grouped: Vec<(Method, Vec<Reading<f64>>)> = Vec::new();
            for (method, on, value) in figures {
                let reading = Reading { on, value };
                match grouped.iter_mut().find(|one| one.0 == method) {
                    Some(one) => one.1.push(reading),
                    None => grouped.push((method, vec![reading])),
                }
            }
            let series: Vec<MethodSeries> = grouped
                .into_iter()
                .map(|(method, readings)| MethodSeries::of(method, readings))
                .collect();
            (!series.is_empty()).then_some(ComponentSeries {
                component,
                unit,
                series,
            })
        })
        .collect()
}

fn by_day(readings: &[Reading<f64>]) -> BTreeMap<Date, Vec<f64>> {
    let mut by_day: BTreeMap<Date, Vec<f64>> = BTreeMap::new();
    for reading in readings {
        by_day.entry(reading.on).or_default().push(reading.value);
    }
    by_day
}

pub(super) fn mean(values: &[f64]) -> Option<f64> {
    let count = u32::try_from(values.len())
        .ok()
        .filter(|count| *count > 0)?;
    Some(values.iter().sum::<f64>() / f64::from(count))
}

/// The trend as unbroken runs of consecutive days, from the first figure to
/// the last and never past.
pub(super) fn trend(by_day: &BTreeMap<Date, Vec<f64>>) -> Vec<Vec<Trend>> {
    let (Some(&first), Some(&last)) = (by_day.keys().next(), by_day.keys().next_back()) else {
        return Vec::new();
    };
    let mut runs: Vec<Vec<Trend>> = Vec::new();
    let mut run: Vec<Trend> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        match mean_of_week(by_day, on) {
            Some(day) => run.push(day),
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

fn mean_of_week(by_day: &BTreeMap<Date, Vec<f64>>, on: Date) -> Option<Trend> {
    let start = on
        .checked_sub(jiff::Span::new().days(WINDOW_DAYS - 1))
        .ok()?;
    let week: Vec<&Vec<f64>> = by_day.range(start..=on).map(|(_, values)| values).collect();
    if week.len() < MEASURED_DAYS {
        return None;
    }
    let values: Vec<f64> = week.into_iter().flatten().copied().collect();
    Some(Trend {
        on,
        value: mean(&values)?,
        figures: values.len(),
    })
}

/// The scatter of one figure: the standard deviation of the differences
/// between consecutive days, over √2 since each difference holds two figures'
/// error. `None` with fewer than two such pairs.
pub(super) fn scatter(by_day: &BTreeMap<Date, Vec<f64>>) -> Option<f64> {
    let days: Vec<(Date, f64)> = by_day
        .iter()
        .filter_map(|(&on, values)| mean(values).map(|value| (on, value)))
        .collect();
    let differences: Vec<f64> = days
        .windows(2)
        .filter_map(|pair| match pair {
            [(before, earlier), (on, later)] => before
                .tomorrow()
                .ok()
                .filter(|next| next == on)
                .map(|_| later - earlier),
            _ => None,
        })
        .collect();
    let count = u32::try_from(differences.len())
        .ok()
        .filter(|count| *count >= 2)?;
    let centre = mean(&differences)?;
    let variance = differences
        .iter()
        .map(|difference| (difference - centre).powi(2))
        .sum::<f64>()
        / f64::from(count - 1);
    Some(variance.sqrt() / std::f64::consts::SQRT_2)
}

fn changes(trend: &[Vec<Trend>], sigma: f64) -> Vec<Change> {
    let by_day: BTreeMap<Date, Trend> = trend.iter().flatten().map(|day| (day.on, *day)).collect();
    by_day
        .values()
        .filter_map(|day| {
            let before = day
                .on
                .checked_sub(jiff::Span::new().days(AGAINST_DAYS))
                .ok()?;
            let earlier = by_day.get(&before)?;
            let now = f64::from(u32::try_from(day.figures).ok()?);
            let then = f64::from(u32::try_from(earlier.figures).ok()?);
            let noise = CONFIDENCE * sigma * (1.0 / now + 1.0 / then).sqrt();
            let by = day.value - earlier.value;
            let direction = if by > noise {
                Direction::Rising
            } else if by < -noise {
                Direction::Falling
            } else {
                Direction::Maintaining
            };
            Some(Change {
                on: day.on,
                by,
                noise,
                direction,
            })
        })
        .collect()
}
