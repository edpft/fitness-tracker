//! Reading the two payloads a Bike+ ride is composed from.
//!
//! **Numbers arrive as text.** Every quantity is held as a [`RawValue`] and
//! handed to the domain as the characters the source wrote, for the reason
//! [`crate::hevy::payload`] does it: a decimal parsed exactly cannot round, and
//! a speed of `25.4` km/h through an `f64` is not reliably `25.4` on the way
//! back out.
//!
//! Nothing here decides anything. What a field *means* — that `output` is
//! power, that a zero heart rate is a sensor saying nothing, that `distance`
//! without a unit is not to be trusted — is the translator's, and lives next
//! door. This module's job is to hand it the source's words.

use serde::Deserialize;
use serde_json::value::RawValue;

/// Why a payload could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadablePayload {
    pub detail: String,
}

/// The workout record: what Peloton says about the ride itself.
///
/// Everything is optional, because a field this adapter requires and the source
/// omits must become a refusal naming it rather than a deserialisation error
/// naming the whole payload.
#[derive(Debug, Deserialize)]
pub struct WorkoutRecord {
    /// What kind of training it was. `cycling` for a ride.
    #[serde(default)]
    pub fitness_discipline: Option<String>,
    /// What recorded it. `home_bike_plus` for the Bike+.
    ///
    /// **The platform, not the instrument**, when read alone: a yoga class
    /// taken on the bike's screen is also `home_bike_plus`. The bike is the
    /// instrument only when it is being pedalled, which is why the discipline
    /// is checked beside it.
    #[serde(default)]
    pub device_type: Option<String>,
    /// Whether the ride happened outdoors. An outdoor ride is a different
    /// entity, and Garmin syncs them in through this source wearing Peloton's
    /// clothes.
    #[serde(default)]
    pub is_outdoor: Option<bool>,
    /// Unix seconds.
    #[serde(default)]
    pub start_time: Option<i64>,
    /// Unix seconds.
    #[serde(default)]
    pub end_time: Option<i64>,
}

impl WorkoutRecord {
    /// # Errors
    ///
    /// [`UnreadablePayload`] if the bytes are not a workout record.
    pub fn read(bytes: &[u8]) -> Result<Self, UnreadablePayload> {
        serde_json::from_slice(bytes).map_err(|error| UnreadablePayload {
            detail: error.to_string(),
        })
    }
}

/// One of the graph's per-second series.
#[derive(Debug, Deserialize)]
pub struct Metric<'a> {
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub display_unit: Option<String>,
    #[serde(borrow, default)]
    pub values: Vec<&'a RawValue>,
    /// How many seconds of this series the source says it did not measure.
    ///
    /// **Peloton puts it on the heart-rate series and nowhere else**, on every
    /// one of the operator's 233 rides that has one. It is a total rather than
    /// a position: it counts seconds held forward from the previous reading as
    /// well as seconds served as a zero, so it is larger than the gaps in
    /// `values` and cannot be recomputed from them.
    #[serde(borrow, default)]
    pub missing_data_duration: Option<&'a RawValue>,
}

/// One of the graph's whole-ride totals.
#[derive(Debug, Deserialize)]
pub struct Summary<'a> {
    #[serde(default)]
    pub slug: Option<String>,
    /// **The unit is the point.** The workout record states a distance with no
    /// unit anywhere and it is miles, following an account preference; the
    /// graph states the same distance and says which unit it is in. Reading
    /// either without the other is a silent 38% error that changes size if the
    /// preference is ever flipped.
    #[serde(default)]
    pub display_unit: Option<String>,
    #[serde(borrow, default)]
    pub value: Option<&'a RawValue>,
}

/// The performance graph: the series the bike produced, and the ride's totals.
///
/// **It names nothing.** No id, no start time, no zone, no device — a graph is
/// identifiable only by the URL it was fetched from, which is why the entity
/// composes it with the workout record rather than standing on it alone.
#[derive(Debug, Deserialize)]
pub struct PerformanceGraph<'a> {
    /// Seconds since pedalling started, one per sample.
    ///
    /// **Genuinely sparse.** On 143 of the operator's 285 rides it does not run
    /// `1..n`: it starts at 4 or 5, or skips 38 seconds in the middle. Every
    /// series in `metrics` is indexed by this, so the offsets are carried
    /// through to the entity rather than implied by position.
    #[serde(borrow, default)]
    pub seconds_since_pedaling_start: Vec<&'a RawValue>,
    #[serde(borrow, default)]
    pub metrics: Vec<Metric<'a>>,
    #[serde(borrow, default)]
    pub summaries: Vec<Summary<'a>>,
}

impl<'a> PerformanceGraph<'a> {
    /// # Errors
    ///
    /// [`UnreadablePayload`] if the bytes are not a performance graph.
    pub fn read(bytes: &'a [u8]) -> Result<Self, UnreadablePayload> {
        serde_json::from_slice(bytes).map_err(|error| UnreadablePayload {
            detail: error.to_string(),
        })
    }

    /// The series of that name, if the graph served one.
    pub fn metric(&self, slug: &str) -> Option<&Metric<'a>> {
        self.metrics
            .iter()
            .find(|metric| metric.slug.as_deref() == Some(slug))
    }

    /// The total of that name, if the graph served one.
    pub fn summary(&self, slug: &str) -> Option<&Summary<'a>> {
        self.summaries
            .iter()
            .find(|summary| summary.slug.as_deref() == Some(slug))
    }
}

/// The characters a number was written with, or `None` where it was absent or
/// null.
pub fn number(raw: Option<&RawValue>) -> Option<&str> {
    raw.map(RawValue::get).filter(|token| *token != "null")
}
