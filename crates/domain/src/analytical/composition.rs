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
//! **Copies of one workbook are one reading.** The same day sits in up to four
//! copies of a sheet, and they never disagree; a day where two did would show
//! both figures rather than choose.

use std::fmt;

use jiff::civil::Date;

use crate::{
    body::{Component, Percentage},
    measure::Kg,
};

pub use super::per_device::Reading;

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

/// Every figure one method gave for one component, in date order.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSeries {
    pub method: Method,
    pub readings: Vec<Reading<f64>>,
}

/// One component, one series per method.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentSeries {
    pub component: Component,
    pub unit: Unit,
    pub series: Vec<MethodSeries>,
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

            let mut series: Vec<MethodSeries> = Vec::new();
            for (method, on, value) in figures {
                let reading = Reading { on, value };
                match series.iter_mut().find(|one| one.method == method) {
                    Some(one) => one.readings.push(reading),
                    None => series.push(MethodSeries {
                        method,
                        readings: vec![reading],
                    }),
                }
            }
            (!series.is_empty()).then_some(ComponentSeries {
                component,
                unit,
                series,
            })
        })
        .collect()
}
