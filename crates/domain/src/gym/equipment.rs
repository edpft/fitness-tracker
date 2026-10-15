//! The equipment the operator trains on, as facts about the world (issue #450).
//!
//! **A kind or a specific.** A kind is interchangeable — one barbell is
//! another, a gym's plates and dumbbells and cable stacks step alike — so its
//! increments belong to the kind. A specific is a particular product, the
//! Technogym Kneeling Easy Chin Dip or a manufacturer's range of bands, and
//! what it offers belongs to it and is assumed of nothing else: the operator,
//! 2026-10-15, *"one assisted pull up / dip machine may or may not be
//! interchangeable with another"*. The Bike+ is the same argument made for a
//! bike.
//!
//! **Facts are bitemporal.** Each is true over a period of real-world time from
//! a date, apart from when it was entered. A change starts a new period; a
//! correction supersedes a period's value and the store keeps the wrong one as
//! what was once believed. What is here is what is *now* known: the store
//! answers with every fact no correction has superseded.

use std::{collections::BTreeMap, fmt};

use jiff::{Timestamp, civil::Date};

use crate::measure::Kg;
use crate::newtype::string_name;
use crate::prescription::{LoadSteps, Scales};
use crate::sequence::NonEmpty;

use super::exercise::Implement;

const MAX_NAME: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidEquipmentName {
    #[error("a name must not be empty")]
    Empty,
    #[error("a name of {length} characters is longer than {MAX_NAME}")]
    TooLong { length: usize },
    #[error("a name is printable text")]
    NotPrintable,
}

fn name(value: &str) -> Result<String, InvalidEquipmentName> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(InvalidEquipmentName::Empty);
    }
    let length = trimmed.chars().count();
    if length > MAX_NAME {
        return Err(InvalidEquipmentName::TooLong { length });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(InvalidEquipmentName::NotPrintable);
    }
    Ok(trimmed.to_owned())
}

/// What a specific is called: *Technogym Kneeling Easy Chin Dip*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpecificName(String);

impl TryFrom<String> for SpecificName {
    type Error = InvalidEquipmentName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        name(&value).map(Self)
    }
}

string_name!(SpecificName, InvalidEquipmentName);

/// One setting of a range that is not counted in kilograms: *level 3*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LevelName(String);

impl TryFrom<String> for LevelName {
    type Error = InvalidEquipmentName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        name(&value).map(Self)
    }
}

string_name!(LevelName, InvalidEquipmentName);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSettings {
    #[error("a stack holds something, so its most is more than nothing")]
    EmptyStack,
    #[error("a stack's most is one of the loads it steps through")]
    MostOffTheStack,
    #[error("a range of levels has at least one")]
    NoLevels,
    #[error("two levels with one name are one level")]
    RepeatedLevel,
}

/// What a specific can be set to, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Settings {
    /// A stack: every load its steps reach, from the first step to its most.
    /// The Kneeling Easy Chin Dip is 7kg steps to 84kg.
    Stack { steps: LoadSteps, most: Kg },
    /// A range counted in levels and not kilograms, as its maker numbers them.
    /// A band gives what it gives by how far it is stretched, so a level is
    /// the honest unit and a kilogram figure is a guess.
    Levels(NonEmpty<LevelName>),
}

impl Settings {
    /// # Errors
    ///
    /// See [`InvalidSettings`].
    pub fn stack(steps: LoadSteps, most: Kg) -> Result<Self, InvalidSettings> {
        if most.is_none() {
            return Err(InvalidSettings::EmptyStack);
        }
        if steps.quantise(most) != most {
            return Err(InvalidSettings::MostOffTheStack);
        }
        Ok(Self::Stack { steps, most })
    }

    /// # Errors
    ///
    /// See [`InvalidSettings`].
    pub fn levels(levels: Vec<LevelName>) -> Result<Self, InvalidSettings> {
        for (index, level) in levels.iter().enumerate() {
            if levels
                .iter()
                .skip(index.saturating_add(1))
                .any(|other| other == level)
            {
                return Err(InvalidSettings::RepeatedLevel);
            }
        }
        NonEmpty::new(levels)
            .map(Self::Levels)
            .map_err(|_| InvalidSettings::NoLevels)
    }
}

impl fmt::Display for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stack { steps, most } => write!(f, "{steps} to {most}kg"),
            Self::Levels(levels) => {
                let names: Vec<&str> = levels.iter().map(LevelName::as_str).collect();
                write!(f, "levels {}", names.join(", "))
            }
        }
    }
}

/// A fact's identity in the store, which is what a correction names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FactId(i64);

impl FactId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

/// One thing believed true from a date.
///
/// `from` is `None` for what has been true since before anything was recorded:
/// the scales the parameters held before they were facts had no date, and
/// inventing one would claim a change nobody saw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact<T> {
    pub id: FactId,
    pub from: Option<Date>,
    pub recorded_at: Timestamp,
    pub value: T,
}

/// Every period one piece of equipment has, earliest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeline<T>(Vec<Fact<T>>);

impl<T> Timeline<T> {
    /// Ordered by start, an undated fact first. Two facts starting together
    /// would be two answers for one period; the store refuses to write that,
    /// so here the later-recorded one stands.
    #[must_use]
    pub fn new(mut facts: Vec<Fact<T>>) -> Self {
        facts.sort_by(|a, b| a.from.cmp(&b.from).then(a.recorded_at.cmp(&b.recorded_at)));
        Self(facts)
    }

    /// What was true on a date: the latest period starting on or before it.
    #[must_use]
    pub fn on(&self, date: Date) -> Option<&Fact<T>> {
        self.0
            .iter()
            .rev()
            .find(|fact| fact.from.is_none_or(|from| from <= date))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Fact<T>> {
        self.0.iter()
    }
}

/// Which piece of equipment a fact is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Equipment {
    Kind(Implement),
    Specific(SpecificName),
}

impl fmt::Display for Equipment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(implement) => write!(f, "{implement}"),
            Self::Specific(name) => write!(f, "{name}"),
        }
    }
}

/// What a fact says: a kind's increments, or a specific's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Kind(Implement, LoadSteps),
    Specific(SpecificName, Settings),
}

impl Statement {
    #[must_use]
    pub fn equipment(&self) -> Equipment {
        match self {
            Self::Kind(implement, _) => Equipment::Kind(*implement),
            Self::Specific(name, _) => Equipment::Specific(name.clone()),
        }
    }
}

/// Everything now known about the equipment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EquipmentFacts {
    pub kinds: BTreeMap<Implement, Timeline<LoadSteps>>,
    pub specifics: BTreeMap<SpecificName, Timeline<Settings>>,
}

impl EquipmentFacts {
    /// Every kind's increments as they were on a date: what a prescription for
    /// that date is generated against.
    #[must_use]
    pub fn scales_on(&self, date: Date) -> Scales {
        Scales::new(
            self.kinds
                .iter()
                .filter_map(|(implement, timeline)| {
                    timeline
                        .on(date)
                        .map(|fact| (*implement, fact.value.clone()))
                })
                .collect(),
        )
    }

    /// The fact a correction names, and the piece of equipment it is about.
    #[must_use]
    pub fn find(&self, id: FactId) -> Option<(Equipment, Option<Date>)> {
        let kind = self.kinds.iter().find_map(|(implement, timeline)| {
            timeline
                .iter()
                .find(|fact| fact.id == id)
                .map(|fact| (Equipment::Kind(*implement), fact.from))
        });
        kind.or_else(|| {
            self.specifics.iter().find_map(|(name, timeline)| {
                timeline
                    .iter()
                    .find(|fact| fact.id == id)
                    .map(|fact| (Equipment::Specific(name.clone()), fact.from))
            })
        })
    }

    /// Whether a period already starts on this date for this equipment.
    #[must_use]
    pub fn starts(&self, equipment: &Equipment, from: Option<Date>) -> bool {
        match equipment {
            Equipment::Kind(implement) => self
                .kinds
                .get(implement)
                .is_some_and(|timeline| timeline.iter().any(|fact| fact.from == from)),
            Equipment::Specific(name) => self
                .specifics
                .get(name)
                .is_some_and(|timeline| timeline.iter().any(|fact| fact.from == from)),
        }
    }
}
