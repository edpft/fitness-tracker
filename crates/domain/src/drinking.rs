//! What the operator drank on a day, at whatever accuracy he knows it
//! (#873).
//!
//! **A day with no entry is no alcohol** (operator, 2026-11-19). An entry
//! is kept like an illness rather than as an observation: changing one replaces
//! it, because only what is now thought about a day matters.

use std::{fmt, num::NonZeroU8, num::NonZeroU16};

use jiff::civil::Date;

use crate::{
    newtype::{from_str_via_string, string_name},
    sequence::NonEmpty,
};

/// One day's drinking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub date: Date,
    pub drinking: Drinking,
}

/// How much is known about what was drunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drinking {
    /// Drinking, and nothing counted.
    AmountUnknown,
    /// These, then an unknown amount more.
    AtLeast(NonEmpty<Drinks>),
    /// These and nothing else.
    Exactly(NonEmpty<Drinks>),
}

impl Drinking {
    /// The drinks counted, none where the amount is unknown.
    pub fn counted(&self) -> impl Iterator<Item = &Drinks> {
        let drinks = match self {
            Self::AmountUnknown => None,
            Self::AtLeast(drinks) | Self::Exactly(drinks) => Some(drinks),
        };
        drinks.into_iter().flat_map(NonEmpty::iter)
    }

    /// The units in what was counted: all of them for [`Drinking::Exactly`],
    /// a floor for [`Drinking::AtLeast`], and none known otherwise.
    pub fn units(&self) -> Option<Units> {
        match self {
            Self::AmountUnknown => None,
            Self::AtLeast(_) | Self::Exactly(_) => Some(self.counted().map(Drinks::units).sum()),
        }
    }
}

/// "3 × house red": a count of one drink.
///
/// The name, volume and strength are copied in, so changing one of the
/// operator's own drinks later leaves the days already recorded as they
/// were drunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drinks {
    pub count: NonZeroU8,
    pub name: DrinkName,
    pub drink: Drink,
}

impl Drinks {
    pub fn units(&self) -> Units {
        Units(self.drink.units().0 * u32::from(self.count.get()))
    }
}

/// A drink's size and strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Drink {
    pub volume: Millilitres,
    pub strength: Strength,
}

impl Drink {
    /// ABV × ml ÷ 1,000, the NHS's formula, to the nearest hundredth.
    pub fn units(&self) -> Units {
        // Tenths of a percent × ml ÷ 100 is hundredths of a unit.
        let product = u32::from(self.strength.0.get()) * u32::from(self.volume.0.get());
        Units((product + 50) / 100)
    }
}

/// Units of alcohol, held in hundredths. Derived, never stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Units(u32);

impl Units {
    pub const fn as_hundredths(self) -> u32 {
        self.0
    }
}

impl std::iter::Sum for Units {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|units| units.0).sum())
    }
}

impl fmt::Display for Units {
    /// To one decimal place, as the NHS gives them.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tenths = (self.0 + 5) / 10;
        write!(f, "{}.{}", tenths / 10, tenths % 10)
    }
}

/// Why a volume could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a whole number of millilitres above zero")]
pub struct InvalidVolume {
    value: String,
}

/// A volume in whole millilitres, above zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Millilitres(NonZeroU16);

impl Millilitres {
    pub const fn new(millilitres: NonZeroU16) -> Self {
        Self(millilitres)
    }

    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl TryFrom<String> for Millilitres {
    type Error = InvalidVolume;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value
            .trim()
            .parse::<NonZeroU16>()
            .map(Self)
            .map_err(|_| InvalidVolume { value })
    }
}

from_str_via_string!(Millilitres, InvalidVolume);

impl fmt::Display for Millilitres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ml", self.0)
    }
}

/// Why a strength could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a percentage above 0 and at most 100, to one decimal place")]
pub struct InvalidStrength {
    value: String,
}

/// Alcohol by volume, in tenths of a percent: above 0 and at most 100 %.
///
/// **Above zero because a drink with no alcohol is not drinking**, and an
/// day of it is a day with no entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Strength(NonZeroU16);

impl Strength {
    /// # Errors
    ///
    /// [`InvalidStrength`] above 1,000 tenths.
    pub fn from_tenths(tenths: NonZeroU16) -> Result<Self, InvalidStrength> {
        if tenths.get() > 1_000 {
            return Err(InvalidStrength {
                value: format!("{tenths} tenths"),
            });
        }
        Ok(Self(tenths))
    }

    pub const fn as_tenths(self) -> u16 {
        self.0.get()
    }
}

impl TryFrom<String> for Strength {
    type Error = InvalidStrength;

    /// A percentage as written on a label: `5`, `5.2`, or `5.2%`.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let invalid = || InvalidStrength {
            value: value.clone(),
        };
        let text = value.trim().trim_end_matches('%').trim_end();
        let (whole, tenth) = text.split_once('.').unwrap_or((text, "0"));
        let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if !all_digits(whole) || !all_digits(tenth) || tenth.len() != 1 {
            return Err(invalid());
        }
        let whole: u16 = whole.parse().map_err(|_| invalid())?;
        let tenth: u16 = tenth.parse().map_err(|_| invalid())?;
        let tenths = whole
            .checked_mul(10)
            .and_then(|tens| tens.checked_add(tenth))
            .and_then(NonZeroU16::new)
            .ok_or_else(invalid)?;
        Self::from_tenths(tenths).map_err(|_| invalid())
    }
}

from_str_via_string!(Strength, InvalidStrength);

impl fmt::Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tenths = self.0.get();
        if tenths.is_multiple_of(10) {
            write!(f, "{}%", tenths / 10)
        } else {
            write!(f, "{}.{}%", tenths / 10, tenths % 10)
        }
    }
}

/// Why a drink's name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a drink's name has something in it besides spaces")]
pub struct EmptyDrinkName;

/// What a drink is called: an NHS serving's name, or one the operator gave.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DrinkName(String);

impl TryFrom<String> for DrinkName {
    type Error = EmptyDrinkName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(EmptyDrinkName);
        }
        if trimmed.len() == value.len() {
            Ok(Self(value))
        } else {
            Ok(Self(trimmed.to_owned()))
        }
    }
}

string_name!(DrinkName, EmptyDrinkName);

/// One of the operator's own drinks, named once and picked again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnDrink {
    pub name: DrinkName,
    pub drink: Drink,
}

/// The NHS's standard servings.
///
/// From <https://www.nhs.uk/live-well/alcohol-advice/calculating-alcohol-units/>
/// as it stood on 2026-11-19 (page last reviewed 2024-08-27).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StandardServing {
    SmallSpirit,
    LargeSpirit,
    Alcopop,
    SmallWine,
    StandardWine,
    LargeWine,
    BottleOfWine,
    BottleOfBeer,
    CanOfBeer,
    PintLowerStrength,
    PintHigherStrength,
}

impl StandardServing {
    pub const ALL: [Self; 11] = [
        Self::SmallSpirit,
        Self::LargeSpirit,
        Self::Alcopop,
        Self::SmallWine,
        Self::StandardWine,
        Self::LargeWine,
        Self::BottleOfWine,
        Self::BottleOfBeer,
        Self::CanOfBeer,
        Self::PintLowerStrength,
        Self::PintHigherStrength,
    ];

    /// The NHS's name for it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SmallSpirit => "single small shot of spirits",
            Self::LargeSpirit => "large single shot of spirits",
            Self::Alcopop => "alcopop",
            Self::SmallWine => "small glass of wine",
            Self::StandardWine => "standard glass of wine",
            Self::LargeWine => "large glass of wine",
            Self::BottleOfWine => "bottle of wine",
            Self::BottleOfBeer => "bottle of lager, beer or cider",
            Self::CanOfBeer => "can of lager, beer or cider",
            Self::PintLowerStrength => "pint of lower-strength lager, beer or cider",
            Self::PintHigherStrength => "pint of higher-strength lager, beer or cider",
        }
    }

    pub const fn drink(self) -> Drink {
        let (millilitres, tenths) = match self {
            Self::SmallSpirit => (25, 400),
            Self::LargeSpirit => (35, 400),
            Self::Alcopop => (275, 46),
            Self::SmallWine => (125, 120),
            Self::StandardWine => (175, 120),
            Self::LargeWine => (250, 120),
            Self::BottleOfWine => (750, 135),
            Self::BottleOfBeer => (330, 50),
            Self::CanOfBeer => (440, 55),
            Self::PintLowerStrength => (568, 36),
            Self::PintHigherStrength => (568, 52),
        };
        Drink {
            volume: Millilitres(nonzero(millilitres)),
            strength: Strength(nonzero(tenths)),
        }
    }

    /// The serving as the operator's own drink, for the same picker.
    pub fn as_own(self) -> OwnDrink {
        OwnDrink {
            name: DrinkName(self.name().to_owned()),
            drink: self.drink(),
        }
    }
}

/// The table's constants, none of which is zero.
const fn nonzero(value: u16) -> NonZeroU16 {
    match NonZeroU16::new(value) {
        Some(value) => value,
        None => NonZeroU16::MIN,
    }
}
