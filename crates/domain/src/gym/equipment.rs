//! The equipment the operator trains on, as facts about the world (issue #450).
//!
//! **A kind, a machine or a range of bands.** A kind is interchangeable —
//! one barbell is another, a gym's plates and dumbbells and cable stacks step
//! alike — so its increments belong to the kind. A machine is a particular
//! product, by maker and model, and what it offers belongs to it and is
//! assumed of nothing else: the operator, 2026-10-15, *"one assisted pull up /
//! dip machine may or may not be interchangeable with another"*. The Bike+ is
//! the same argument made for a bike. A range of bands is by maker and model
//! too, but every range has one shape: bands, each giving a force at twice
//! its length.
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

/// Who makes a specific: *Physical*, *Decathlon*, *Technogym*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Maker(String);

impl TryFrom<String> for Maker {
    type Error = InvalidEquipmentName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        name(&value).map(Self)
    }
}

string_name!(Maker, InvalidEquipmentName);

/// What its maker calls it: *Supaflex*, *Corength*, *Kneeling Easy Chin Dip*.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Model(String);

impl TryFrom<String> for Model {
    type Error = InvalidEquipmentName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        name(&value).map(Self)
    }
}

string_name!(Model, InvalidEquipmentName);

/// A particular product, by maker and model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Specific {
    pub maker: Maker,
    pub model: Model,
}

impl fmt::Display for Specific {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.maker, self.model)
    }
}

/// How one band of a range is told from the others, in its maker's terms.
///
/// *4/5 stars* on a Supaflex band, a colour on a Corength one. A colour is not
/// the name where the maker marks something else, because a colour may change
/// (the operator, 2026-10-15).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BandName(String);

impl TryFrom<String> for BandName {
    type Error = InvalidEquipmentName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        name(&value).map(Self)
    }
}

string_name!(BandName, InvalidEquipmentName);

/// One band, and the force it takes to stretch it to twice its resting length.
///
/// **That force is its nominal load** (the operator, 2026-10-15): the one
/// figure most makers publish, so bands of different ranges compare. Supaflex
/// publishes it beside a figure at 1.15×, Decathlon publishes only it, and
/// nobody publishes a midpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Band {
    pub name: BandName,
    pub at_twice_length: Kg,
}

/// Bands used together, each named as often as it is used: *1/5 stars + 1/5
/// stars*. Their nominal loads add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combination(NonEmpty<BandName>);

impl Combination {
    #[must_use]
    pub const fn new(bands: NonEmpty<BandName>) -> Self {
        Self(bands)
    }

    pub fn iter(&self) -> impl Iterator<Item = &BandName> {
        self.0.iter()
    }
}

impl fmt::Display for Combination {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut counted: Vec<(&BandName, usize)> = Vec::new();
        for band in self.0.iter() {
            match counted.iter_mut().find(|(name, _)| *name == band) {
                Some((_, count)) => *count = count.saturating_add(1),
                None => counted.push((band, 1)),
            }
        }
        let parts: Vec<String> = counted
            .into_iter()
            .map(|(name, count)| {
                if count == 1 {
                    name.to_string()
                } else {
                    format!("{count} × {name}")
                }
            })
            .collect();
        f.write_str(&parts.join(" + "))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSettings {
    #[error("a stack holds something, so its most is more than nothing")]
    EmptyStack,
    #[error("a stack's most is one of the loads it steps through")]
    MostOffTheStack,
    #[error("a range of bands has at least one band")]
    NoBands,
    #[error("two bands with one name are one band")]
    RepeatedBand,
    #[error("a band takes some force to stretch")]
    SlackBand,
    #[error("a range of bands has at least one step")]
    NoSteps,
    #[error("a step uses a band the range does not have")]
    UnknownBand,
    #[error("each step gives less assistance than the one before it")]
    StepsOutOfOrder,
}

/// A machine's stack: every load its steps reach, from the first step to its
/// most. The Kneeling Easy Chin Dip is 7kg steps to 84kg.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    steps: LoadSteps,
    most: Kg,
}

impl Stack {
    /// # Errors
    ///
    /// See [`InvalidSettings`].
    pub fn new(steps: LoadSteps, most: Kg) -> Result<Self, InvalidSettings> {
        if most.is_none() {
            return Err(InvalidSettings::EmptyStack);
        }
        if steps.quantise(most) != most {
            return Err(InvalidSettings::MostOffTheStack);
        }
        Ok(Self { steps, most })
    }

    pub const fn steps(&self) -> &LoadSteps {
        &self.steps
    }

    pub const fn most(&self) -> Kg {
        self.most
    }
}

impl fmt::Display for Stack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} to {}kg", self.steps, self.most)
    }
}

/// A range of bands, and the combinations of them the operator steps through,
/// most assistance first.
///
/// **Bands are their own category, not a kind of machine** (the operator,
/// 2026-10-15): every range has the same shape, bands giving a force at twice
/// their length, and what differs is the maker and model. The steps are
/// declared rather than worked out from every combination, which would need
/// to know how many of each band there are (*"this chart looks reasonable for
/// now"*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandRange {
    bands: NonEmpty<Band>,
    steps: NonEmpty<Combination>,
}

impl BandRange {
    /// # Errors
    ///
    /// See [`InvalidSettings`].
    pub fn new(bands: Vec<Band>, steps: Vec<Combination>) -> Result<Self, InvalidSettings> {
        for (index, band) in bands.iter().enumerate() {
            if band.at_twice_length.is_none() {
                return Err(InvalidSettings::SlackBand);
            }
            if bands
                .iter()
                .skip(index.saturating_add(1))
                .any(|other| other.name == band.name)
            {
                return Err(InvalidSettings::RepeatedBand);
            }
        }
        let bands = NonEmpty::new(bands).map_err(|_| InvalidSettings::NoBands)?;
        let mut previous: Option<Kg> = None;
        for step in &steps {
            let load = nominal(&bands, step).ok_or(InvalidSettings::UnknownBand)?;
            if previous.is_some_and(|previous| load >= previous) {
                return Err(InvalidSettings::StepsOutOfOrder);
            }
            previous = Some(load);
        }
        let steps = NonEmpty::new(steps).map_err(|_| InvalidSettings::NoSteps)?;
        Ok(Self { bands, steps })
    }

    pub const fn bands(&self) -> &NonEmpty<Band> {
        &self.bands
    }

    pub const fn steps(&self) -> &NonEmpty<Combination> {
        &self.steps
    }
}

impl fmt::Display for BandRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bands: Vec<String> = self
            .bands
            .iter()
            .map(|band| format!("{} {}kg", band.name, band.at_twice_length))
            .collect();
        let steps: Vec<String> = self.steps.iter().map(ToString::to_string).collect();
        write!(f, "{}; steps {}", bands.join(", "), steps.join(" → "))
    }
}

/// The nominal load of bands used together, or nothing if one of them is not
/// in the range.
fn nominal(bands: &NonEmpty<Band>, step: &Combination) -> Option<Kg> {
    step.iter().try_fold(Kg::NONE, |total, name| {
        bands.iter().find(|band| band.name == *name).map(|band| {
            Kg::from_grams(
                total
                    .as_grams()
                    .saturating_add(band.at_twice_length.as_grams()),
            )
        })
    })
}

/// One step of a range of bands, as a prescription names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandSetting {
    pub specific: Specific,
    pub bands: Combination,
    pub nominal: Kg,
}

impl fmt::Display for BandSetting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.specific, self.bands)
    }
}

/// A range of bands as a prescription steps through it: each declared step
/// with its nominal load, most assistance first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BandLadder {
    specific: Specific,
    rungs: Vec<(Kg, Combination)>,
}

impl BandLadder {
    /// The ladder a range of bands makes.
    #[must_use]
    pub fn of(specific: &Specific, range: &BandRange) -> Self {
        let rungs = range
            .steps
            .iter()
            .filter_map(|step| nominal(&range.bands, step).map(|load| (load, step.clone())))
            .collect();
        Self {
            specific: specific.clone(),
            rungs,
        }
    }

    fn setting(&self, index: usize) -> Option<BandSetting> {
        self.rungs.get(index).map(|(nominal, bands)| BandSetting {
            specific: self.specific.clone(),
            bands: bands.clone(),
            nominal: *nominal,
        })
    }

    /// The step nearest an amount of assistance, the one giving more where two
    /// are as near; nothing where no band at all is nearer.
    #[must_use]
    pub fn nearest(&self, assistance: Kg) -> Option<BandSetting> {
        let grams = assistance.as_grams();
        let distance = |load: Kg| load.as_grams().abs_diff(grams);
        let (index, (load, _)) = self
            .rungs
            .iter()
            .enumerate()
            .min_by_key(|(index, (load, _))| (distance(*load), *index))?;
        if grams < distance(*load) {
            return None;
        }
        self.setting(index)
    }

    /// The step after this one, giving less assistance; nothing after the
    /// last, where the next is no band at all.
    #[must_use]
    pub fn after(&self, setting: &BandSetting) -> Option<BandSetting> {
        let index = self
            .rungs
            .iter()
            .position(|(_, bands)| *bands == setting.bands)?;
        self.setting(index.saturating_add(1))
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
    Machine(Specific),
    Bands(Specific),
}

impl fmt::Display for Equipment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(implement) => write!(f, "{implement}"),
            Self::Machine(specific) | Self::Bands(specific) => write!(f, "{specific}"),
        }
    }
}

/// What a fact says: a kind's increments, a machine's stack, or a range of
/// bands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    Kind(Implement, LoadSteps),
    Machine(Specific, Stack),
    Bands(Specific, BandRange),
}

impl Statement {
    #[must_use]
    pub fn equipment(&self) -> Equipment {
        match self {
            Self::Kind(implement, _) => Equipment::Kind(*implement),
            Self::Machine(specific, _) => Equipment::Machine(specific.clone()),
            Self::Bands(specific, _) => Equipment::Bands(specific.clone()),
        }
    }
}

/// Everything now known about the equipment.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EquipmentFacts {
    pub kinds: BTreeMap<Implement, Timeline<LoadSteps>>,
    pub machines: BTreeMap<Specific, Timeline<Stack>>,
    pub bands: BTreeMap<Specific, Timeline<BandRange>>,
}

fn found<K: Clone, T>(
    map: &BTreeMap<K, Timeline<T>>,
    id: FactId,
    equipment: impl Fn(K) -> Equipment,
) -> Option<(Equipment, Option<Date>)> {
    map.iter().find_map(|(key, timeline)| {
        timeline
            .iter()
            .find(|fact| fact.id == id)
            .map(|fact| (equipment(key.clone()), fact.from))
    })
}

fn starts_in<K: Ord, T>(map: &BTreeMap<K, Timeline<T>>, key: &K, from: Option<Date>) -> bool {
    map.get(key)
        .is_some_and(|timeline| timeline.iter().any(|fact| fact.from == from))
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
        found(&self.kinds, id, Equipment::Kind)
            .or_else(|| found(&self.machines, id, Equipment::Machine))
            .or_else(|| found(&self.bands, id, Equipment::Bands))
    }

    /// Whether a period already starts on this date for this equipment.
    #[must_use]
    pub fn starts(&self, equipment: &Equipment, from: Option<Date>) -> bool {
        match equipment {
            Equipment::Kind(implement) => starts_in(&self.kinds, implement, from),
            Equipment::Machine(specific) => starts_in(&self.machines, specific, from),
            Equipment::Bands(specific) => starts_in(&self.bands, specific, from),
        }
    }
}
