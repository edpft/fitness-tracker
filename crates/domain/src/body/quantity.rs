//! The quantities a weigh-in reads that nothing else does.
//!
//! Each holds an integer at the resolution the scale reports, so a value
//! persisted and read back is the value served (§ 7). Masses are
//! [`crate::measure::Kg`] and a heart rate is
//! [`crate::measure::BeatsPerMinute`]; they are shared with the gym and the bike.

use std::fmt;

/// Writes tenths as a decimal, dropping a trailing `.0`.
fn tenths(f: &mut fmt::Formatter<'_>, value: u32) -> fmt::Result {
    let (whole, tenth) = (value / 10, value % 10);
    if tenth == 0 {
        write!(f, "{whole}")
    } else {
        write!(f, "{whole}.{tenth}")
    }
}

/// Withings' visceral fat rating, in tenths.
///
/// An index with no unit, not a mass or a share, so it is its own type rather
/// than a number something could add to a kilogram.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VisceralFat(u32);

impl VisceralFat {
    pub const fn from_tenths(tenths: u32) -> Self {
        Self(tenths)
    }

    pub const fn as_tenths(self) -> u32 {
        self.0
    }
}

impl fmt::Display for VisceralFat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        tenths(f, self.0)
    }
}

/// Energy spent at rest over a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KilocaloriesPerDay(u32);

impl KilocaloriesPerDay {
    pub const fn new(kilocalories: u32) -> Self {
        Self(kilocalories)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for KilocaloriesPerDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}kcal/day", self.0)
    }
}

/// An age the scale estimates, in tenths of a year.
///
/// **Tenths because vascular age is served to one decimal place** and
/// metabolic age in whole years; one type at the finer resolution holds both
/// without rounding either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Age(u32);

impl Age {
    pub const fn from_tenths_of_a_year(tenths: u32) -> Self {
        Self(tenths)
    }

    pub const fn as_tenths_of_a_year(self) -> u32 {
        self.0
    }
}

impl fmt::Display for Age {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        tenths(f, self.0)?;
        f.write_str(" years")
    }
}

/// How fast the pulse travels along the arteries, in millimetres per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PulseWaveVelocity(u32);

impl PulseWaveVelocity {
    pub const fn from_millimetres_per_second(millimetres: u32) -> Self {
        Self(millimetres)
    }

    pub const fn as_millimetres_per_second(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PulseWaveVelocity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (whole, fraction) = (self.0 / 1_000, self.0 % 1_000);
        write!(f, "{whole}.{fraction:03}m/s")
    }
}

/// What the scale reads through the soles of the feet, in nanosiemens.
///
/// **The unit is inferred, not confirmed.** Withings serves these to three
/// decimal places with no unit, and the values (32 to 84) fit electrochemical
/// skin conductance in microsiemens; the store holds them exactly either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkinConductance(u32);

impl SkinConductance {
    pub const fn from_nanosiemens(nanosiemens: u32) -> Self {
        Self(nanosiemens)
    }

    pub const fn as_nanosiemens(self) -> u32 {
        self.0
    }
}

impl fmt::Display for SkinConductance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (whole, fraction) = (self.0 / 1_000, self.0 % 1_000);
        write!(f, "{whole}.{fraction:03}µS")
    }
}
