//! The Peloton Bike+ ride: what one ride was, and the series the bike produced.
//!
//! **It is a Bike+ ride, not an indoor ride recorded by a Bike+.** The
//! operator, 2026-09-07: *"we're not building an indoor ride entity, we're
//! building a Peloton Bike+ entity because we can't separate the recording
//! device from the recorded performance"*, because *"a Peloton Bike+ isn't just
//! an indoor bike with a specific set of sensors, it's specific sensors **and
//! algorithms**"*.
//!
//! That is § 6's method-dependence, and the same argument already made for body
//! composition and heart rate. A Bike+ has no ground speed and no displacement:
//! it derives speed from resistance and cadence through Peloton's own algorithm
//! and integrates that into a distance, and it estimates power the same way,
//! where a pedal-based meter measures force directly. The instrument is part of
//! what the measurement *means*, so an entity abstracting over instruments would
//! assert a comparability that does not exist.
//!
//! What follows from that, and what makes this shape cheaper than a device
//! field:
//!
//! - **There is no device field.** The entity is the device's.
//! - **A generic stationary bike ride is a different entity**, unbuilt. There is
//!   no source for one, and it was never this type with fields missing.
//! - **An outdoor ride is a different entity**, unbuilt. Garmin is where it will
//!   come from.
//! - **§ 6 becomes structural rather than declarative.** Series from different
//!   entities cannot stitch, because they are not the same type. Nobody has to
//!   remember to declare a comparability class.
//! - **"How far did I ride this year" across a Bike+ and a Garmin is not one
//!   number**, and the model says so by construction.
//!
//! A vendor's product in `domain` is right here by the operator's own test:
//! what keeps something out of the domain is a *transport* — base URLs,
//! credential variables, `exercise_template_id`. A Bike+ is equipment he trains
//! on, and § 6 already names Withings and `InBody` as what makes two body-fat
//! readings different series. The instrument is subject matter.
//!
//! **The vocabulary is ours.** Peloton says `output`; the domain says power —
//! the operator, asked: *"I would call it power"*. Nothing in this module knows
//! the source's names for anything, and the mapping onto these types lives in
//! that source's adapter.

use std::fmt;

use crate::landing::{LandingRecordId, Provenance, SourceRecordId};
use crate::measure::{Duration, InvalidQuantity, Metres};
use crate::normalised::{NormalisedEntity, StartedAt};
use crate::sequence::NonEmpty;

use super::zone::Watts;

/// How fast the pedals turned, in revolutions per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cadence(u32);

impl Cadence {
    pub const fn as_revolutions_per_minute(self) -> u32 {
        self.0
    }

    pub const fn from_revolutions_per_minute(rpm: u32) -> Self {
        Self(rpm)
    }
}

impl TryFrom<String> for Cadence {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse().map(Self).map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "a cadence",
                    value,
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "revolutions per minute",
                    value,
                }
            }
        })
    }
}

impl fmt::Display for Cadence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}rpm", self.0)
    }
}

/// How hard the brake was set, as a percentage of the bike's range.
///
/// Bounded at construction rather than checked downstream (§ 24). It is a
/// share of a fixed range, so 101% is not a stiff setting — it is a value the
/// bike could not have produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Resistance(u8);

impl Resistance {
    pub const fn as_percentage(self) -> u8 {
        self.0
    }

    /// # Errors
    ///
    /// [`InvalidQuantity::OutOfRange`] above 100.
    pub const fn from_percentage(percentage: u8) -> Result<Self, InvalidQuantity> {
        if percentage > 100 {
            return Err(InvalidQuantity::OutOfRange {
                unit: "a resistance percentage",
            });
        }
        Ok(Self(percentage))
    }
}

impl TryFrom<String> for Resistance {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let percentage: u8 = value.parse().map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "a resistance",
                    value: value.clone(),
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "percent",
                    value: value.clone(),
                }
            }
        })?;
        Self::from_percentage(percentage)
    }
}

impl fmt::Display for Resistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

/// How fast the bike said the rider was going, in millimetres per hour.
///
/// **Millimetres per hour, and the unit is the interesting part.** Every other
/// quantity here canonicalises to metres and seconds, and this one cannot:
/// speed arrives as kilometres per hour to one decimal place, and dividing by
/// 3.6 is not exact — 25.4 km/h is 7.0555… metres per second and no fixed-point
/// metres-per-second value holds it. Since the value is persisted and compared
/// against rows written by earlier versions (§ 7), what is stored has to be
/// exact, so the hour stays and the metre is subdivided instead. It is still
/// metric and still integral; nothing rounds on the way in or out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Speed(u64);

impl Speed {
    pub const ZERO: Self = Self(0);

    pub const fn as_millimetres_per_hour(self) -> u64 {
        self.0
    }

    pub const fn from_millimetres_per_hour(millimetres: u64) -> Self {
        Self(millimetres)
    }
}

/// A kilometre in millimetres. What a decimal string of km/h is scaled by.
const MILLIMETRES_PER_KILOMETRE: u64 = 1_000_000;

impl TryFrom<String> for Speed {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        scale_decimal(&value, MILLIMETRES_PER_KILOMETRE, "kilometres per hour").map(Self)
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / MILLIMETRES_PER_KILOMETRE;
        let fraction = self.0 % MILLIMETRES_PER_KILOMETRE;
        if fraction == 0 {
            write!(f, "{whole}km/h")
        } else {
            let fraction = format!("{fraction:06}");
            write!(f, "{whole}.{}km/h", fraction.trim_end_matches('0'))
        }
    }
}

/// A decimal string scaled to a whole number of the smaller unit.
///
/// Exact, because the text is split rather than parsed as a float — the same
/// reasoning [`Metres`] is stored in millimetres for. `scale` fixes how many
/// fractional digits are representable: any more and the value is refused
/// rather than truncated, since silently dropping a digit is the source and the
/// store disagreeing about what was recorded.
fn scale_decimal(value: &str, scale: u64, unit: &'static str) -> Result<u64, InvalidQuantity> {
    let malformed = || InvalidQuantity::NotANumber {
        unit,
        value: value.to_owned(),
    };
    if value.starts_with('-') {
        return Err(InvalidQuantity::Negative {
            unit,
            value: value.to_owned(),
        });
    }

    let digits = usize::try_from(scale.ilog10()).map_err(|_| malformed())?;
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > digits {
        return Err(malformed());
    }
    let mut padded = fraction.to_owned();
    while padded.len() < digits {
        padded.push('0');
    }

    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| malformed())?
    };
    let fraction: u64 = if padded.is_empty() {
        0
    } else {
        padded.parse().map_err(|_| malformed())?
    };
    whole
        .checked_mul(scale)
        .and_then(|scaled| scaled.checked_add(fraction))
        .ok_or_else(malformed)
}

/// A heart rate, in beats per minute.
///
/// **Zero is unrepresentable, which is the whole point of the type.** Peloton
/// serves a 0 for every second the watch was not broadcasting — 4,528 of them
/// across the operator's record, 2,037 consecutively on one ride where the
/// watch stopped a third of the way in. A heart does not beat zero times a
/// minute while its owner is pedalling, so a 0 is the absence of a reading, and
/// a type that cannot hold one turns "drop the dropouts" from a rule the
/// translator has to remember into a shape it cannot avoid (§ 24, § 37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatsPerMinute(std::num::NonZeroU32);

impl BeatsPerMinute {
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// # Errors
    ///
    /// [`InvalidQuantity::NotBeating`] for a zero, which is a sensor saying
    /// nothing rather than a rate.
    pub const fn new(beats: u32) -> Result<Self, InvalidQuantity> {
        match std::num::NonZeroU32::new(beats) {
            Some(beats) => Ok(Self(beats)),
            None => Err(InvalidQuantity::NotBeating),
        }
    }
}

impl TryFrom<String> for BeatsPerMinute {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let beats: u32 = value.parse().map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "a heart rate",
                    value: value.clone(),
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "beats per minute",
                    value: value.clone(),
                }
            }
        })?;
        Self::new(beats)
    }
}

impl fmt::Display for BeatsPerMinute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}bpm", self.0)
    }
}

crate::newtype::from_str_via_string!(Cadence, InvalidQuantity);
crate::newtype::from_str_via_string!(Resistance, InvalidQuantity);
crate::newtype::from_str_via_string!(Speed, InvalidQuantity);
crate::newtype::from_str_via_string!(BeatsPerMinute, InvalidQuantity);

/// One second of what the bike measured.
///
/// **The four travel together because the bike produced them together**, at one
/// index, by one method. Splitting them into four series would invite them to be
/// stored at four resolutions, which § II.3 forbids, and would lose the fact
/// that this cadence and this resistance are what that speed was computed from.
///
/// `at` is seconds since pedalling started, and it is carried rather than
/// implied by position. The index the source serves is genuinely sparse — on
/// 143 of the operator's 285 rides it starts at 4, or skips 38 seconds in the
/// middle — so row order would silently restate a gap as continuous recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RideSample {
    pub at: Duration,
    pub power: Watts,
    pub cadence: Cadence,
    pub resistance: Resistance,
    pub speed: Speed,
}

/// One second of what the watch broadcast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartRateSample {
    pub at: Duration,
    pub beats_per_minute: BeatsPerMinute,
}

/// The heart-rate series, where one was recorded.
///
/// **Its own series rather than a fifth column on [`RideSample`]**, because it
/// is the one measurement here that is not the bike's. It comes from a watch on
/// broadcast, occasionally a strap, relayed through Peloton — a different
/// method from the other four, which § 6 says makes it a different series even
/// inside one entity. As a separate series, "nothing was worn" is
/// [`None`] and "the strap dropped out for every second" is a series with no
/// samples, which cannot be built; as a nullable column the two would be the
/// same rows of nulls.
///
/// **Which device supplied it is not recoverable.** Peloton records no sensor
/// identity anywhere, and the operator has parked that: it would have to come
/// from Garmin directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartRateSeries {
    samples: NonEmpty<HeartRateSample>,
    declared_missing: Option<Duration>,
}

impl HeartRateSeries {
    /// `declared_missing` is what the *source* says was not measured, and is
    /// not derived from the samples: it counts seconds Peloton knows it held a
    /// value forward for as well as the ones it served a zero for, so it is
    /// larger than the gaps here and cannot be recomputed from them. [`None`]
    /// where the source stated nothing, or stated something unreadable.
    pub const fn new(
        samples: NonEmpty<HeartRateSample>,
        declared_missing: Option<Duration>,
    ) -> Self {
        Self {
            samples,
            declared_missing,
        }
    }

    pub const fn samples(&self) -> &NonEmpty<HeartRateSample> {
        &self.samples
    }

    /// How many seconds the source says it did not measure. Never a count of
    /// the gaps in [`Self::samples`], which is a different number.
    pub const fn declared_missing(&self) -> Option<Duration> {
        self.declared_missing
    }
}

/// The landing records one ride is composed from.
///
/// **Two, and that is what needed constitution 3.1.0.** Peloton serves a ride's
/// start, duration and device from its workout list, and that ride's sample
/// streams from a performance graph fetched per workout. The graph names no
/// workout and states no time, no zone and no device, so neither response is an
/// entity on its own and nothing is being reconciled: the two do not overlap,
/// so there are no rival claims to prefer between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposedFrom {
    /// The record the ride's own account came from.
    pub ride: LandingRecordId,
    /// The record its samples came from.
    pub samples: LandingRecordId,
}

/// One ride on a Peloton Bike+, as Peloton recorded it.
///
/// Identified by the landing records it came from rather than by the source's
/// record id, for [`crate::gym::GymWorkout`]'s reason: two records sharing a
/// source id are the same source contradicting itself, § 10 puts that at the
/// canonical layer, and keying on the source id here would collapse the pair
/// silently.
///
/// **The summary is distance and nothing else.** The operator, 2026-09-07:
/// *"just keep distance in our normalised entity, if we find we want total
/// output or AVG output at some later date, we have the raw data but, for now,
/// I don't see why we'd need them and we could derive them"*. § 5 puts derived
/// metrics in the analytical layer and raw keeps everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BikePlusRide {
    started_at: StartedAt,
    duration: Duration,
    distance: Metres,
    samples: NonEmpty<RideSample>,
    heart_rate: Option<HeartRateSeries>,
    provenance: Provenance,
    source_record_id: SourceRecordId,
    landed_as: ComposedFrom,
}

/// What a ride is built from, so the constructor is not eight positional
/// arguments of which three are identifiers.
pub struct RideRecord {
    pub started_at: StartedAt,
    /// How long the ride lasted: the span of the workout itself.
    ///
    /// **Not the class's length**, which Peloton also serves and which the
    /// performance graph echoes. Those are the class's, not this ride's (§ 11),
    /// and they differ — the operator has ridden 82 seconds past the end of a
    /// class. Nor is it the span of the samples, which is derivable from them
    /// (§ 5) and shorter again.
    pub duration: Duration,
    pub distance: Metres,
    pub samples: NonEmpty<RideSample>,
    pub heart_rate: Option<HeartRateSeries>,
    pub provenance: Provenance,
    pub source_record_id: SourceRecordId,
    pub landed_as: ComposedFrom,
}

impl BikePlusRide {
    /// Provenance is a constructor argument rather than a setter, so a ride
    /// that exists is a ride that knows where it came from (§ II.3).
    pub fn new(record: RideRecord) -> Self {
        Self {
            started_at: record.started_at,
            duration: record.duration,
            distance: record.distance,
            samples: record.samples,
            heart_rate: record.heart_rate,
            provenance: record.provenance,
            source_record_id: record.source_record_id,
            landed_as: record.landed_as,
        }
    }

    pub const fn started_at(&self) -> &StartedAt {
        &self.started_at
    }

    pub const fn duration(&self) -> Duration {
        self.duration
    }

    pub const fn distance(&self) -> Metres {
        self.distance
    }

    pub const fn samples(&self) -> &NonEmpty<RideSample> {
        &self.samples
    }

    pub const fn heart_rate(&self) -> Option<&HeartRateSeries> {
        self.heart_rate.as_ref()
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    pub const fn landed_as(&self) -> ComposedFrom {
        self.landed_as
    }
}

impl NormalisedEntity for BikePlusRide {
    fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }
}

impl fmt::Display for BikePlusRide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {}, {}, {} samples",
            self.started_at,
            self.duration,
            self.distance,
            self.samples.count()
        )
    }
}
