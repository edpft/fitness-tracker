//! A Withings Body Scan weigh-in.
//!
//! **Everything the scale measured at one step onto it** (the operator: *"if
//! we're building a `BodyScanWeighIn`, it should contain everything"*), read off
//! 841 landed groups on 2026-09-17 (#153):
//!
//! - **Composition is always there.** A weigh-in without it (a nerve reading
//!   taken alone, a manual entry) is not a weigh-in, and is refused.
//! - **Heart, nerves and vascular are each whole or absent.** A few weigh-ins
//!   lack one, and none has half of one.
//! - **Every body part, every time.** 213 of 213 carry all five segments, so
//!   they are five fields rather than a list that could be short.
//!
//! **Fat ratio is not here.** Withings serves it, and in all 213 weigh-ins it is
//! exactly fat mass over mass, so it is derivable (§ 5) and stays in raw.
//! Fat-free mass and fat mass do not sum to mass (they miss it by tens of
//! grams), so all three are carried.

use std::fmt;

use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::{BeatsPerMinute, Kg},
    normalised::{NormalisedEntity, StartedAt},
    sequence::NonEmpty,
};

use super::quantity::{Age, KilocaloriesPerDay, PulseWaveVelocity, SkinConductance, VisceralFat};

/// Which of the source's records one part of a weigh-in came from.
///
/// Per part rather than per weigh-in, because Withings files each part as its
/// own group, with its own identifier and its own algorithm version: the
/// composition groups in the operator's record were produced by two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredBy {
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    /// The algorithm identifier the source states, verbatim. Zero is what it
    /// states for the nerve and vascular groups.
    pub algorithm: u64,
    pub provenance: Provenance,
}

/// Mass by body part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub fat_free_mass: Kg,
    pub fat_mass: Kg,
    pub muscle_mass: Kg,
}

/// The five parts the scale divides a body into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segments {
    pub left_arm: Segment,
    pub right_arm: Segment,
    pub left_leg: Segment,
    pub right_leg: Segment,
    pub torso: Segment,
}

/// What the scale says the body is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    pub mass: Kg,
    pub fat_free_mass: Kg,
    pub fat_mass: Kg,
    pub muscle_mass: Kg,
    pub body_water: Kg,
    pub extracellular_water: Kg,
    pub intracellular_water: Kg,
    pub bone_mass: Kg,
    pub visceral_fat: VisceralFat,
    pub basal_metabolic_rate: KilocaloriesPerDay,
    pub metabolic_age: Age,
    pub segments: Segments,
    pub from: MeasuredBy,
}

/// What the electrocardiogram concluded.
///
/// The three results the operator's record holds, named as the Withings app
/// shows them. A code outside these refuses the weigh-in rather than being
/// guessed at: no atrial fibrillation result has been recorded, so its code is
/// not known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rhythm {
    SinusRhythm,
    /// A high heart rate, with no signs of atrial fibrillation.
    HighHeartRate,
    /// The recording ran and could not be read. A result, not an absence.
    NotClassified,
}

impl Rhythm {
    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SinusRhythm => "sinus-rhythm",
            Self::HighHeartRate => "high-heart-rate",
            Self::NotClassified => "not-classified",
        }
    }
}

impl fmt::Display for Rhythm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SinusRhythm => "sinus rhythm",
            Self::HighHeartRate => "high heart rate, no signs of AFib",
            Self::NotClassified => "not classified",
        })
    }
}

/// The heart, as read through the handle.
///
/// **One group or two.** The rate and the rhythm usually arrive together; once,
/// on 15 Jan, the rhythm came as a group of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartReading {
    pub heart_rate: BeatsPerMinute,
    /// Absent where the recording did not run, which three weigh-ins show.
    pub rhythm: Option<Rhythm>,
    pub from: NonEmpty<MeasuredBy>,
}

/// Nerve health, as read through the feet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NerveReading {
    pub left_foot: SkinConductance,
    pub right_foot: SkinConductance,
    pub both_feet: SkinConductance,
    pub from: MeasuredBy,
}

/// The arteries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VascularReading {
    pub pulse_wave_velocity: PulseWaveVelocity,
    pub vascular_age: Age,
    pub from: MeasuredBy,
}

/// What a weigh-in is built from.
pub struct WeighInRecord {
    pub measured_at: StartedAt,
    pub composition: Composition,
    pub heart: Option<HeartReading>,
    pub nerves: Option<NerveReading>,
    pub vascular: Option<VascularReading>,
}

/// One step onto a Withings Body Scan.
///
/// **No device field**: the entity is the device's, as a Bike+ ride is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyScanWeighIn {
    measured_at: StartedAt,
    composition: Composition,
    heart: Option<HeartReading>,
    nerves: Option<NerveReading>,
    vascular: Option<VascularReading>,
}

impl BodyScanWeighIn {
    pub fn new(record: WeighInRecord) -> Self {
        Self {
            measured_at: record.measured_at,
            composition: record.composition,
            heart: record.heart,
            nerves: record.nerves,
            vascular: record.vascular,
        }
    }

    /// The second every group of the weigh-in is stamped with.
    pub const fn measured_at(&self) -> &StartedAt {
        &self.measured_at
    }

    pub const fn composition(&self) -> &Composition {
        &self.composition
    }

    pub const fn mass(&self) -> Kg {
        self.composition.mass
    }

    pub const fn heart(&self) -> Option<&HeartReading> {
        self.heart.as_ref()
    }

    pub const fn nerves(&self) -> Option<&NerveReading> {
        self.nerves.as_ref()
    }

    pub const fn vascular(&self) -> Option<&VascularReading> {
        self.vascular.as_ref()
    }

    /// Every record the weigh-in was built from, composition first.
    pub fn parts(&self) -> Vec<&MeasuredBy> {
        let mut parts = vec![&self.composition.from];
        if let Some(heart) = &self.heart {
            parts.extend(heart.from.iter());
        }
        parts.extend(self.nerves.iter().map(|nerves| &nerves.from));
        parts.extend(self.vascular.iter().map(|vascular| &vascular.from));
        parts
    }
}

impl NormalisedEntity for BodyScanWeighIn {
    fn composes(&self) -> Vec<&SourceRecordId> {
        self.parts()
            .into_iter()
            .map(|part| &part.source_record_id)
            .collect()
    }
}

impl fmt::Display for BodyScanWeighIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — {}kg", self.measured_at, self.composition.mass)
    }
}
