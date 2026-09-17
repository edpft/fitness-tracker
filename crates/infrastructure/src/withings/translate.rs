//! Turning Withings' groups for one second into a Body Scan weigh-in.
//!
//! Deterministic and total, as the other translators are (§ 9).
//!
//! **Three things are refused, each by the operator's ruling (2026-09-17)**:
//! a reading not from a Body Scan (the two typed in at account creation), a
//! reading the scale could not attribute (`attrib` 1), and a heart, nerve or
//! vascular reading taken without a weigh-in. None is dropped (§ 37).
//!
//! **Withings' numbers stay here.** A measure is `value × 10^unit` with a
//! numeric type; which type is which is read from the operator's payloads and
//! named after the `aiowithings` client library, since Withings' own reference
//! could not be read. The body-part positions and the three foot readings are
//! named the same way, and are unconfirmed.

use std::collections::BTreeMap;

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{
        Age, BodyScanWeighIn, Composition, HeartReading, KilocaloriesPerDay, MeasuredBy,
        NerveReading, PulseWaveVelocity, Rhythm, Segment, Segments, SkinConductance,
        VascularReading, VisceralFat, WeighInRecord,
    },
    landing::{EventKind, LandedRecord, Provenance},
    measure::{BeatsPerMinute, Kg},
    normalised::{OperatorZone, RefusalLocus, RefusalReason, StartedAt},
    sequence::NonEmpty,
};
use jiff::Timestamp;
use serde::Deserialize;

use crate::scribe::Scribe;

use super::account::WeighInAccount;

/// What Withings calls the scale.
const BODY_SCAN: &str = "Body Scan";

/// Attributed to the operator: `0` for most groups, `8` for the vascular one.
const ATTRIBUTED: [i64; 2] = [0, 8];
/// The scale did not know who stepped on it.
const UNATTRIBUTED: i64 = 1;

/// Measure types, by what they are.
mod kind {
    pub const MASS: i64 = 1;
    pub const FAT_FREE_MASS: i64 = 5;
    /// Fat mass over mass, exactly. Served and deliberately not carried (§ 5).
    pub const FAT_RATIO: i64 = 6;
    pub const FAT_MASS: i64 = 8;
    pub const HEART_RATE: i64 = 11;
    pub const MUSCLE_MASS: i64 = 76;
    pub const BODY_WATER: i64 = 77;
    pub const BONE_MASS: i64 = 88;
    pub const PULSE_WAVE_VELOCITY: i64 = 91;
    pub const RHYTHM: i64 = 130;
    pub const VASCULAR_AGE: i64 = 155;
    pub const NERVES_LEFT_FOOT: i64 = 158;
    pub const NERVES_RIGHT_FOOT: i64 = 159;
    pub const NERVES_BOTH_FEET: i64 = 167;
    pub const EXTRACELLULAR_WATER: i64 = 168;
    pub const INTRACELLULAR_WATER: i64 = 169;
    pub const VISCERAL_FAT: i64 = 170;
    pub const SEGMENT_FAT_FREE_MASS: i64 = 173;
    pub const SEGMENT_FAT_MASS: i64 = 174;
    pub const SEGMENT_MUSCLE_MASS: i64 = 175;
    pub const BASAL_METABOLIC_RATE: i64 = 226;
    pub const METABOLIC_AGE: i64 = 227;
}

/// Where on the body a measure was taken.
mod position {
    pub const RIGHT_ARM: i64 = 2;
    pub const LEFT_ARM: i64 = 3;
    pub const WHOLE_BODY: i64 = 7;
    pub const LEFT_LEG: i64 = 10;
    pub const RIGHT_LEG: i64 = 11;
    pub const TORSO: i64 = 12;
}

/// Rhythm codes, as the operator read them off the Withings app.
const SINUS_RHYTHM: i64 = 9;
const HIGH_HEART_RATE: i64 = 10;
const NOT_CLASSIFIED: i64 = 5;

const COMPOSITION: [i64; 15] = [
    kind::MASS,
    kind::FAT_FREE_MASS,
    kind::FAT_RATIO,
    kind::FAT_MASS,
    kind::MUSCLE_MASS,
    kind::BODY_WATER,
    kind::BONE_MASS,
    kind::EXTRACELLULAR_WATER,
    kind::INTRACELLULAR_WATER,
    kind::VISCERAL_FAT,
    kind::SEGMENT_FAT_FREE_MASS,
    kind::SEGMENT_FAT_MASS,
    kind::SEGMENT_MUSCLE_MASS,
    kind::BASAL_METABOLIC_RATE,
    kind::METABOLIC_AGE,
];
const HEART: [i64; 2] = [kind::HEART_RATE, kind::RHYTHM];
const NERVES: [i64; 3] = [
    kind::NERVES_LEFT_FOOT,
    kind::NERVES_RIGHT_FOOT,
    kind::NERVES_BOTH_FEET,
];
const VASCULAR: [i64; 2] = [kind::PULSE_WAVE_VELOCITY, kind::VASCULAR_AGE];

#[derive(Debug, Deserialize)]
struct Group {
    attrib: i64,
    date: i64,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
    measures: Vec<Measure>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct Measure {
    value: i64,
    #[serde(rename = "type")]
    kind: i64,
    unit: i32,
    #[serde(default)]
    algo: Option<u64>,
    #[serde(default)]
    position: Option<i64>,
}

/// Which part of a weigh-in a measure type belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Composition,
    Heart,
    Nerves,
    Vascular,
}

impl Part {
    fn of(kind: i64) -> Option<Self> {
        if COMPOSITION.contains(&kind) {
            Some(Self::Composition)
        } else if HEART.contains(&kind) {
            Some(Self::Heart)
        } else if NERVES.contains(&kind) {
            Some(Self::Nerves)
        } else if VASCULAR.contains(&kind) {
            Some(Self::Vascular)
        } else {
            None
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Composition => "composition",
            Self::Heart => "heart",
            Self::Nerves => "nerve",
            Self::Vascular => "vascular",
        }
    }
}

/// One weigh-in's measures, keyed by type and position.
struct Measures(BTreeMap<(i64, Option<i64>), Measure>);

impl Measures {
    fn get(&self, kind: i64, position: Option<i64>) -> Result<Measure, RefusalReason> {
        self.0
            .get(&(kind, position))
            .copied()
            .ok_or_else(|| RefusalReason::UnreadablePayload {
                detail: format!("no measure of type {kind} at position {position:?}"),
            })
    }

    /// A whole-body measure, which is served at position 7 or at none.
    fn whole(&self, kind: i64) -> Result<Measure, RefusalReason> {
        self.get(kind, Some(position::WHOLE_BODY))
            .or_else(|_| self.get(kind, None))
    }

    fn has(&self, kind: i64) -> bool {
        self.0.keys().any(|(held, _)| *held == kind)
    }
}

/// `value × 10^unit`, as an integer count of `10^exponent`. Exact or refused.
fn scaled(measure: Measure, exponent: i32) -> Result<u64, RefusalReason> {
    let unreadable = || RefusalReason::UnreadableValue {
        field: "measure",
        detail: format!(
            "type {} is {} × 10^{}, which is not a whole number of 10^{exponent}",
            measure.kind, measure.value, measure.unit
        ),
    };
    let value = u64::try_from(measure.value).map_err(|_| unreadable())?;
    let shift = measure.unit.checked_sub(exponent).ok_or_else(unreadable)?;
    let power = 10_u64
        .checked_pow(shift.unsigned_abs())
        .ok_or_else(unreadable)?;
    if shift >= 0 {
        value.checked_mul(power).ok_or_else(unreadable)
    } else if value % power == 0 {
        Ok(value / power)
    } else {
        Err(unreadable())
    }
}

fn narrow(value: u64, kind: i64) -> Result<u32, RefusalReason> {
    u32::try_from(value).map_err(|_| RefusalReason::UnreadableValue {
        field: "measure",
        detail: format!("type {kind} is out of range"),
    })
}

fn grams(measures: &Measures, kind: i64, position: Option<i64>) -> Result<Kg, RefusalReason> {
    let measure = match position {
        Some(position) => measures.get(kind, Some(position))?,
        None => measures.whole(kind)?,
    };
    scaled(measure, -3).map(Kg::from_grams)
}

fn segment(measures: &Measures, at: i64) -> Result<Segment, RefusalReason> {
    Ok(Segment {
        fat_free_mass: grams(measures, kind::SEGMENT_FAT_FREE_MASS, Some(at))?,
        fat_mass: grams(measures, kind::SEGMENT_FAT_MASS, Some(at))?,
        muscle_mass: grams(measures, kind::SEGMENT_MUSCLE_MASS, Some(at))?,
    })
}

fn tenths(measures: &Measures, kind: i64) -> Result<u32, RefusalReason> {
    narrow(scaled(measures.whole(kind)?, -1)?, kind)
}

fn thousandths(measures: &Measures, kind: i64) -> Result<u32, RefusalReason> {
    narrow(scaled(measures.whole(kind)?, -3)?, kind)
}

fn whole(measures: &Measures, kind: i64) -> Result<u32, RefusalReason> {
    narrow(scaled(measures.whole(kind)?, 0)?, kind)
}

fn composition(measures: &Measures, from: MeasuredBy) -> Result<Composition, RefusalReason> {
    Ok(Composition {
        mass: grams(measures, kind::MASS, None)?,
        fat_free_mass: grams(measures, kind::FAT_FREE_MASS, None)?,
        fat_mass: grams(measures, kind::FAT_MASS, None)?,
        muscle_mass: grams(measures, kind::MUSCLE_MASS, None)?,
        body_water: grams(measures, kind::BODY_WATER, None)?,
        extracellular_water: grams(measures, kind::EXTRACELLULAR_WATER, None)?,
        intracellular_water: grams(measures, kind::INTRACELLULAR_WATER, None)?,
        bone_mass: grams(measures, kind::BONE_MASS, None)?,
        visceral_fat: VisceralFat::from_tenths(tenths(measures, kind::VISCERAL_FAT)?),
        basal_metabolic_rate: KilocaloriesPerDay::new(whole(measures, kind::BASAL_METABOLIC_RATE)?),
        metabolic_age: Age::from_tenths_of_a_year(
            whole(measures, kind::METABOLIC_AGE)?.saturating_mul(10),
        ),
        segments: Segments {
            left_arm: segment(measures, position::LEFT_ARM)?,
            right_arm: segment(measures, position::RIGHT_ARM)?,
            left_leg: segment(measures, position::LEFT_LEG)?,
            right_leg: segment(measures, position::RIGHT_LEG)?,
            torso: segment(measures, position::TORSO)?,
        },
        from,
    })
}

fn heart(measures: &Measures, from: NonEmpty<MeasuredBy>) -> Result<HeartReading, RefusalReason> {
    if !measures.has(kind::HEART_RATE) {
        return Err(RefusalReason::Unmodelled {
            detail: "an ECG result with no heart rate".to_owned(),
        });
    }
    let heart_rate = BeatsPerMinute::new(whole(measures, kind::HEART_RATE)?).map_err(|error| {
        RefusalReason::UnreadableValue {
            field: "heart rate",
            detail: error.to_string(),
        }
    })?;
    let rhythm = if measures.has(kind::RHYTHM) {
        let code = measures.whole(kind::RHYTHM)?.value;
        Some(match code {
            SINUS_RHYTHM => Rhythm::SinusRhythm,
            HIGH_HEART_RATE => Rhythm::HighHeartRate,
            NOT_CLASSIFIED => Rhythm::NotClassified,
            other => {
                return Err(RefusalReason::Unmodelled {
                    detail: format!("ECG result code {other}"),
                });
            }
        })
    } else {
        None
    };
    Ok(HeartReading {
        heart_rate,
        rhythm,
        from,
    })
}

fn nerves(measures: &Measures, from: MeasuredBy) -> Result<NerveReading, RefusalReason> {
    Ok(NerveReading {
        left_foot: SkinConductance::from_nanosiemens(thousandths(
            measures,
            kind::NERVES_LEFT_FOOT,
        )?),
        right_foot: SkinConductance::from_nanosiemens(thousandths(
            measures,
            kind::NERVES_RIGHT_FOOT,
        )?),
        both_feet: SkinConductance::from_nanosiemens(thousandths(
            measures,
            kind::NERVES_BOTH_FEET,
        )?),
        from,
    })
}

fn vascular(measures: &Measures, from: MeasuredBy) -> Result<VascularReading, RefusalReason> {
    Ok(VascularReading {
        pulse_wave_velocity: PulseWaveVelocity::from_millimetres_per_second(thousandths(
            measures,
            kind::PULSE_WAVE_VELOCITY,
        )?),
        vascular_age: Age::from_tenths_of_a_year(tenths(measures, kind::VASCULAR_AGE)?),
        from,
    })
}

/// A group, read and checked against the operator's rulings.
fn admitted(record: &LandedRecord) -> Result<Group, RefusalReason> {
    let group: Group = serde_json::from_slice(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.to_string(),
        }
    })?;
    match group.model.as_deref() {
        Some(BODY_SCAN) => {}
        Some(other) => {
            return Err(RefusalReason::NotTheInstrument {
                detail: format!("a reading from a {other}"),
            });
        }
        None => {
            return Err(RefusalReason::NotTheInstrument {
                detail: "a reading with no device".to_owned(),
            });
        }
    }
    if group.attrib == UNATTRIBUTED {
        return Err(RefusalReason::Unattributed);
    }
    if !ATTRIBUTED.contains(&group.attrib) {
        return Err(RefusalReason::Unmodelled {
            detail: format!("a Withings group with attribution {}", group.attrib),
        });
    }
    Ok(group)
}

/// The one algorithm a group's measures state.
fn algorithm(group: &Group) -> Result<u64, RefusalReason> {
    let mut stated = group.measures.iter().filter_map(|measure| measure.algo);
    let first = stated
        .next()
        .ok_or_else(|| RefusalReason::UnreadablePayload {
            detail: "a group stating no algorithm".to_owned(),
        })?;
    if stated.any(|other| other != first) {
        return Err(RefusalReason::UnreadablePayload {
            detail: "a group stating two algorithms".to_owned(),
        });
    }
    Ok(first)
}

/// The weigh-in, or the one reason there is none.
fn weigh_in(
    account: &WeighInAccount,
    zone: &OperatorZone,
) -> Result<BodyScanWeighIn, RefusalReason> {
    let mut measures = BTreeMap::new();
    let mut parts: BTreeMap<Part, Vec<MeasuredBy>> = BTreeMap::new();
    let mut stamp = None;

    for record in account.groups().iter() {
        let group = admitted(record)?;
        let mut own = Vec::new();
        for measure in &group.measures {
            let part = Part::of(measure.kind).ok_or_else(|| RefusalReason::Unmodelled {
                detail: format!("Withings measure type {}", measure.kind),
            })?;
            if !own.contains(&part) {
                own.push(part);
            }
            if measures
                .insert((measure.kind, measure.position), *measure)
                .is_some()
            {
                return Err(RefusalReason::UnreadableValue {
                    field: "measure",
                    detail: format!("type {} served twice", measure.kind),
                });
            }
        }
        let by = MeasuredBy {
            landed_as: record.id(),
            source_record_id: record.source_record_id().clone(),
            algorithm: algorithm(&group)?,
            provenance: record.provenance().clone(),
        };
        for part in own {
            parts.entry(part).or_default().push(by.clone());
        }
        stamp.get_or_insert((group.date, group.timezone));
    }
    let measures = Measures(measures);

    let single = |part: Part, parts: &mut BTreeMap<Part, Vec<MeasuredBy>>| {
        parts.remove(&part).map_or(Ok(None), |from| {
            let mut from = from.into_iter();
            match (from.next(), from.next()) {
                (Some(by), None) => Ok(Some(by)),
                _ => Err(RefusalReason::UnreadablePayload {
                    detail: format!("a {} reading split across groups", part.name()),
                }),
            }
        })
    };

    let Some(composition_from) = single(Part::Composition, &mut parts)? else {
        return Err(parts
            .keys()
            .next()
            .map_or(RefusalReason::NothingTranslatable, |part| {
                RefusalReason::WithoutWeighIn { part: part.name() }
            }));
    };

    let (date, stated_zone) = stamp.ok_or(RefusalReason::NothingTranslatable)?;
    let instant = Timestamp::from_second(date).map_err(|error| RefusalReason::UnreadableValue {
        field: "date",
        detail: error.to_string(),
    })?;
    // The zone the source states, where it states one (§ II.3).
    let zone = stated_zone
        .and_then(|stated| OperatorZone::try_from(stated).ok())
        .unwrap_or_else(|| zone.clone());

    let heart_from = parts
        .remove(&Part::Heart)
        .and_then(|from| NonEmpty::new(from).ok());
    Ok(BodyScanWeighIn::new(WeighInRecord {
        measured_at: StartedAt::new(instant, zone),
        composition: composition(&measures, composition_from)?,
        heart: heart_from.map(|from| heart(&measures, from)).transpose()?,
        nerves: single(Part::Nerves, &mut parts)?
            .map(|from| nerves(&measures, from))
            .transpose()?,
        vascular: single(Part::Vascular, &mut parts)?
            .map(|from| vascular(&measures, from))
            .transpose()?,
    }))
}

/// The Withings adapter's translator.
#[derive(Debug, Clone, Copy, Default)]
pub struct WithingsWeighInTranslator;

impl Translator for WithingsWeighInTranslator {
    type Account = WeighInAccount;
    type Entity = BodyScanWeighIn;

    fn translate(
        &self,
        account: &WeighInAccount,
        zone: &OperatorZone,
    ) -> Result<Translation<BodyScanWeighIn>, NormalisationError> {
        let mut scribe = Scribe::new(account.groups().first());

        for record in account.groups().iter() {
            let Provenance::Event(event) = record.provenance();
            match event.kind() {
                // `getmeas` never reports a deletion. Were it to, the weigh-in
                // goes with the group.
                EventKind::Deleted => {
                    return Ok(Translation::Retraction {
                        of: record.source_record_id().clone(),
                    });
                }
                EventKind::Unrecognised(kind) => {
                    return Ok(scribe.only(
                        RefusalLocus::Record,
                        RefusalReason::UnreadablePayload {
                            detail: format!("event kind {kind:?} is not one we translate"),
                        },
                    ));
                }
                EventKind::Updated => {}
            }
        }

        Ok(match weigh_in(account, zone) {
            Ok(weigh_in) => Translation::Entity {
                entity: Box::new(weigh_in),
                refusals: scribe.into_refusals(),
            },
            Err(reason) => scribe.only(RefusalLocus::Record, reason),
        })
    }
}
