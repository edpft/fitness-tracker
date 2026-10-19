//! Body composition against the nutrition phase in force (#489).
//!
//! **Fat mass against fat-free mass, each falling, maintaining or rising**
//! (operator, 2026-10-16): a 3×3 matrix, and every period falls in one cell.
//! Fat-free mass is the weigh-in's mass less its fat, so the spreadsheets'
//! percentage reads on the same axes as the Body Scan's mass; the scale's
//! muscle is 0.95 of it on every reading, so it is the same axis.
//!
//! **A period is one nutrition phase within the macrocycle** (operator,
//! 2026-10-19), read from its first day to its last or to today. Success
//! depends on the phase, so it is never judged over the whole macrocycle.
//!
//! **The phase is assumed from the macrocycle** until one is planned (#258): a
//! cut in the preparatory phase, maintenance in the competitive and transition
//! phases (operator, 2026-10-19).
//!
//! **A period's change is its last trend against its first**, each the 7-day
//! trend of [`super::composition`], the first whose 7 days all fall in the
//! period, so nothing of the phase before is read into it. The two weeks never
//! overlap, and a change is beyond the noise on the same test as #419's.

use std::{collections::BTreeMap, fmt};

use jiff::civil::Date;

use super::composition::{
    CONFIDENCE, Direction, Measured, Method, Stated, Trend, WINDOW_DAYS, scatter, trend,
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
}

impl NutritionPhase {
    /// The phase assumed for a macrocycle's phase, until one is planned.
    pub const fn assumed(phase: Phase) -> Self {
        match phase {
            Phase::Preparatory => Self::Cut,
            Phase::Competitive | Phase::Transition => Self::Maintenance,
        }
    }

    /// Whether a cell is a success in this phase (operator, 2026-10-19).
    ///
    /// **A cut** loses fat, and gains fat-free mass, holds it, or loses it
    /// more slowly than fat. **Maintenance** does not gain fat, and does not
    /// lose fat-free mass.
    pub fn succeeds(self, cell: &Cell) -> bool {
        match self {
            Self::Cut => {
                cell.fat.direction == Direction::Falling
                    && match cell.fat_free.direction {
                        Direction::Rising | Direction::Maintaining => true,
                        Direction::Falling => cell.fat_free.by.abs() < cell.fat.by.abs(),
                    }
            }
            Self::Maintenance => {
                cell.fat.direction != Direction::Rising
                    && cell.fat_free.direction != Direction::Falling
            }
        }
    }
}

impl fmt::Display for NutritionPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cut => "cut",
            Self::Maintenance => "maintenance",
        })
    }
}

/// One nutrition phase within a macrocycle, both ends included.
///
/// An end is `None` where the plan does not yet say: the competitive
/// mesocycle starts maintenance, and it is not realised until the one before
/// it ends (#222).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Period {
    pub phase: NutritionPhase,
    pub from: Option<Date>,
    pub to: Option<Date>,
}

/// The macrocycle's periods, in the order they run.
///
/// **Maintenance starts with the competitive mesocycle**, the one ending on
/// the last day of term ([`Macrocycle::phase`]). Until it is planned it starts
/// on no known day, unless the transition has already begun, which no
/// competitive mesocycle came before.
#[must_use]
pub fn periods(macrocycle: &Macrocycle, mesocycles: &[Span], today: Date) -> [Period; 2] {
    let transition = macrocycle.transition();
    let maintenance = mesocycles
        .iter()
        .find(|span| span.end() == transition && span.start() >= macrocycle.start())
        .map(|span| span.start())
        .or_else(|| (today >= transition).then_some(transition));
    [
        Period {
            phase: NutritionPhase::Cut,
            from: Some(macrocycle.start()),
            to: maintenance.and_then(|day| day.yesterday().ok()),
        },
        Period {
            phase: NutritionPhase::Maintenance,
            from: maintenance,
            to: Some(macrocycle.last()),
        },
    ]
}

/// One axis's change over a period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moved {
    /// In kilograms, positive when the trend rose.
    pub by: f64,
    /// How far either way is within the noise.
    pub noise: f64,
    pub direction: Direction,
}

/// Where a period falls in the matrix, read between two trends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// The day of the first trend.
    pub from: Date,
    /// The day of the last.
    pub to: Date,
    pub fat: Moved,
    pub fat_free: Moved,
}

/// What one method gave over a period: its cell, or `None` where it has too
/// few weigh-ins for a trend a week apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ByMethod {
    pub method: Method,
    pub cell: Option<Cell>,
}

/// A period as far as it has run.
#[derive(Debug, Clone, PartialEq)]
pub enum Standing {
    NotStarted,
    /// One per method with a figure in the period so far, the earliest first.
    Read(Vec<ByMethod>),
}

/// A period and how it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodReading {
    pub period: Period,
    /// Whether today falls in it.
    pub running: bool,
    pub standing: Standing,
}

/// The macrocycle and each of its periods, read on `today`.
#[derive(Debug, Clone, PartialEq)]
pub struct Recomposition {
    pub macrocycle: Macrocycle,
    pub periods: Vec<PeriodReading>,
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
    let by = last.value - first.value;
    let direction = if by > noise {
        Direction::Rising
    } else if by < -noise {
        Direction::Falling
    } else {
        Direction::Maintaining
    };
    Some(Moved {
        by,
        noise,
        direction,
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

/// The macrocycle's periods as they stand on `today`.
#[must_use]
pub fn read(
    measured: &[Measured],
    macrocycle: Macrocycle,
    mesocycles: &[Span],
    today: Date,
) -> Recomposition {
    let masses = masses(measured);
    let periods = periods(&macrocycle, mesocycles, today)
        .into_iter()
        .map(|period| {
            let standing =
                period
                    .from
                    .filter(|from| *from <= today)
                    .map_or(Standing::NotStarted, |from| {
                        Standing::Read(by_method(
                            &masses,
                            from,
                            period.to.map_or(today, |to| to.min(today)),
                        ))
                    });
            let running = period.from.is_some_and(|from| from <= today)
                && period.to.is_none_or(|to| today <= to);
            PeriodReading {
                period,
                running,
                standing,
            }
        })
        .collect();
    Recomposition {
        macrocycle,
        periods,
    }
}
