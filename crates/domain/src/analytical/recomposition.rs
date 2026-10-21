//! Body composition against the nutrition phase in force (#489).
//!
//! **Fat mass against fat-free mass** (operator, 2026-10-16): each falling,
//! maintaining or rising makes a 3×3 matrix, and each phase a region of it.
//! Fat-free mass is the weigh-in's mass less its fat, so the spreadsheets'
//! percentage reads on the same axes as the Body Scan's mass; the scale's
//! muscle is 0.95 of it on every reading, so it is the same axis.
//!
//! **A point is a mesocycle, and a chart a nutrition phase** (operator,
//! 2026-10-19): success depends on the phase, so each phase's mesocycles are
//! judged on its own region. A mesocycle's change is drawn per week so that
//! a two-week entry test and a four-week mesocycle are comparable; every edge
//! of either region runs through the origin, so the rate never moves a point
//! across one.
//!
//! **The goal is recomposition, and a phase is the goal restricted by its
//! intent** (operator, 2026-10-20): fat-free mass changing by more kilograms
//! than fat, the half-plane above the diagonal, whatever the phase. A point in
//! the goal but outside its phase's region is progress made by the wrong
//! means, as a cut that gains fat more slowly than fat-free mass.
//!
//! **The phase is assumed from the macrocycle** until one is planned (#258): a
//! cut in the preparatory phase, maintenance in the competitive and transition
//! phases (operator, 2026-10-19).
//!
//! **A mesocycle's change is its last trend against its first**, each the
//! 7-day trend of [`super::composition`], the first whose 7 days all fall in
//! the mesocycle, so nothing of the one before is read into it. The two weeks never
//! overlap, and a change is beyond the noise on the same test as #419's.

use std::{collections::BTreeMap, fmt};

use jiff::civil::Date;

use super::composition::{
    CONFIDENCE, Measured, Method, Stated, Trend, WINDOW_DAYS, scatter, trend,
};
use crate::{
    body::Component,
    macrocycle::{Macrocycle, Phase},
    measure::Kg,
    plan::Span,
};

/// What the operator is eating for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NutritionPhase {
    Cut,
    Maintenance,
    Bulk,
}

/// The goal's half-plane, on the same axes as [`NutritionPhase::region`].
///
/// Fat-free mass changing by more than fat (operator, 2026-10-20). Body fat %
/// was rejected, since as one number it counts losing 2 kg of fat-free mass
/// for 1 kg of fat as progress.
pub const GOAL: [(f64, f64); 3] = [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

impl NutritionPhase {
    /// The phase assumed for a macrocycle's phase, until one is planned.
    pub const fn assumed(phase: Phase) -> Self {
        match phase {
            Phase::Preparatory => Self::Cut,
            Phase::Competitive | Phase::Transition => Self::Maintenance,
        }
    }

    /// The phase's region (operator, 2026-10-19 and 2026-10-20), corner by
    /// corner, fat across and fat-free mass up, each from −1 to 1. Each is
    /// the goal restricted by the phase's intent, so lies within [`GOAL`].
    ///
    /// **Maintenance** does not gain fat, and does not lose fat-free mass:
    /// the quadrant. **A cut** has fat falling: the quadrant, and the wedge
    /// below it where fat-free mass falls more slowly than fat. **A bulk** has
    /// fat-free mass rising: the quadrant, and the wedge beside it where fat
    /// rises more slowly than fat-free mass. Either wedge means weight was
    /// moving, so the intake was not maintenance.
    ///
    /// **A region, never a verdict** (operator, 2026-10-19): a point's noise
    /// is drawn beside it, and calling it a success or a failure would take
    /// one value within that noise as the real one.
    pub const fn region(self) -> [(f64, f64); 4] {
        match self {
            Self::Cut => [(0.0, 0.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, -1.0)],
            Self::Maintenance => [(0.0, 0.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)],
            Self::Bulk => [(0.0, 0.0), (1.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)],
        }
    }
}

impl fmt::Display for NutritionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cut => "cut",
            Self::Maintenance => "maintenance",
            Self::Bulk => "bulk",
        })
    }
}

/// One axis's change over a mesocycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moved {
    /// In kilograms, positive when the trend rose.
    pub by: f64,
    /// How far either way is within the noise.
    pub noise: f64,
}

/// Where a mesocycle falls in the matrix, read between two trends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The day of the first trend.
    pub from: Date,
    /// The day of the last.
    pub to: Date,
    pub fat: Moved,
    pub fat_free: Moved,
}

/// What one method gave over a mesocycle: its cell, or `None` where it has
/// too few weigh-ins for two trends a week apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByMethod {
    pub method: Method,
    pub cell: Option<Cell>,
}

/// A mesocycle as far as it has run, and its nutrition phase.
#[derive(Debug, Clone, PartialEq)]
pub struct MesocycleReading {
    pub span: Span,
    pub phase: NutritionPhase,
    /// Whether today falls in it.
    pub running: bool,
    /// One per method with a figure in it so far, the earliest first.
    pub by_method: Vec<ByMethod>,
}

/// The macrocycle and each mesocycle in it that has started, read on `today`.
#[derive(Debug, Clone, PartialEq)]
pub struct Recomposition {
    pub macrocycle: Macrocycle,
    pub mesocycles: Vec<MesocycleReading>,
}

impl Cell {
    /// How many weeks apart its two trends are.
    pub fn weeks(&self) -> f64 {
        let days = (self.to - self.from).get_days();
        f64::from(days) / 7.0
    }
}

/// Fat and fat-free mass by day, in kilograms, for one method.
#[derive(Default)]
struct Masses {
    fat: BTreeMap<Date, Vec<f64>>,
    fat_free: BTreeMap<Date, Vec<f64>>,
}

fn kilograms(mass: Kg) -> Option<f64> {
    u32::try_from(mass.as_grams())
        .ok()
        .map(|grams| f64::from(grams) / 1_000.0)
}

/// Fat in kilograms, whichever way the source stated it.
fn fat_mass(measured: &Measured) -> Option<f64> {
    let weighed = kilograms(measured.weighed)?;
    match measured.stated {
        Stated::Mass(mass) => kilograms(mass),
        Stated::Percentage(percentage) => {
            Some(weighed * f64::from(percentage.as_tenths()) / 1_000.0)
        }
    }
}

/// Every method's masses, keyed by method. Copies of one workbook are one
/// reading, as in [`super::composition`].
fn masses(measured: &[Measured]) -> BTreeMap<Method, Masses> {
    let mut figures: Vec<(Method, Date, f64, f64)> = measured
        .iter()
        .filter(|one| one.component == Component::Fat)
        .filter_map(|one| {
            let fat = fat_mass(one)?;
            let weighed = kilograms(one.weighed)?;
            Some((one.method, one.on, fat, weighed - fat))
        })
        .collect();
    figures.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then(a.2.total_cmp(&b.2))
            .then(a.3.total_cmp(&b.3))
    });
    figures.dedup_by(|a, b| {
        a.0 == b.0 && a.1 == b.1 && a.2.total_cmp(&b.2).is_eq() && a.3.total_cmp(&b.3).is_eq()
    });
    let mut by_method: BTreeMap<Method, Masses> = BTreeMap::new();
    for (method, on, fat, fat_free) in figures {
        let masses = by_method.entry(method).or_default();
        masses.fat.entry(on).or_default().push(fat);
        masses.fat_free.entry(on).or_default().push(fat_free);
    }
    by_method
}

fn moved(first: Trend, last: Trend, sigma: f64) -> Option<Moved> {
    let now = f64::from(u32::try_from(last.figures).ok()?);
    let then = f64::from(u32::try_from(first.figures).ok()?);
    let noise = CONFIDENCE * sigma * (1.0 / now + 1.0 / then).sqrt();
    Some(Moved {
        by: last.value - first.value,
        noise,
    })
}

/// One method's cell over the days `from` to `to`.
fn cell(masses: &Masses, from: Date, to: Date) -> Option<Cell> {
    let fat_sigma = scatter(&masses.fat)?;
    let fat_free_sigma = scatter(&masses.fat_free)?;
    let by_day = |by_day: &BTreeMap<Date, Vec<f64>>| -> BTreeMap<Date, Trend> {
        trend(by_day)
            .into_iter()
            .flatten()
            .map(|day| (day.on, day))
            .collect()
    };
    let fat = by_day(&masses.fat);
    let fat_free = by_day(&masses.fat_free);
    let earliest = from
        .checked_add(jiff::Span::new().days(WINDOW_DAYS - 1))
        .ok()?;
    let both = |on: &Date| fat_free.contains_key(on);
    let first = fat.range(earliest..=to).map(|(on, _)| *on).find(both)?;
    let last = fat
        .range(earliest..=to)
        .map(|(on, _)| *on)
        .rev()
        .find(both)?;
    let apart = first
        .checked_add(jiff::Span::new().days(WINDOW_DAYS))
        .ok()?;
    if last < apart {
        return None;
    }
    Some(Cell {
        from: first,
        to: last,
        fat: moved(*fat.get(&first)?, *fat.get(&last)?, fat_sigma)?,
        fat_free: moved(
            *fat_free.get(&first)?,
            *fat_free.get(&last)?,
            fat_free_sigma,
        )?,
    })
}

/// Each method with a figure from `from` to `to`, the earliest first.
fn by_method(masses: &BTreeMap<Method, Masses>, from: Date, to: Date) -> Vec<ByMethod> {
    let mut methods: Vec<(Date, ByMethod)> = masses
        .iter()
        .filter_map(|(method, masses)| {
            let first = *masses.fat.range(from..=to).next()?.0;
            Some((
                first,
                ByMethod {
                    method: *method,
                    cell: cell(masses, from, to),
                },
            ))
        })
        .collect();
    methods.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.method.cmp(&b.1.method)));
    methods.into_iter().map(|(_, one)| one).collect()
}

/// The macrocycle's mesocycles as they stand on `today`, oldest first.
///
/// **A mesocycle's phase is the macrocycle's on its first day**, assumed into
/// a nutrition phase. One not yet started has nothing to read, and a week no
/// mesocycle covers belongs to none.
#[must_use]
pub fn read(
    measured: &[Measured],
    macrocycle: Macrocycle,
    mesocycles: &[Span],
    today: Date,
) -> Recomposition {
    let masses = masses(measured);
    let started = macrocycle.start()..=macrocycle.last().min(today);
    let mut spans: Vec<Span> = mesocycles
        .iter()
        .copied()
        .filter(|span| started.contains(&span.start()))
        .collect();
    spans.sort_by_key(|span| (span.start(), span.end()));
    spans.dedup();
    let mesocycles = spans
        .into_iter()
        .map(|span| {
            let last = span.end().yesterday().unwrap_or_else(|_| span.start());
            MesocycleReading {
                span,
                phase: NutritionPhase::assumed(macrocycle.phase(span.start(), Some(span))),
                running: today <= last,
                by_method: by_method(&masses, span.start(), last.min(today)),
            }
        })
        .collect();
    Recomposition {
        macrocycle,
        mesocycles,
    }
}
