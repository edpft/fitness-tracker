//! Turning Peloton's account of one ride into a Bike+ ride.
//!
//! Deterministic and total, as [`crate::hevy::translate`] is: the account's
//! values plus the declared zone resolve the entity with no further input.
//! There is no clock, no request and no overlay in reach (§ 9).
//!
//! **Two things are refused, and they are different kinds of refusal.**
//!
//! - 141 of the operator's 426 workouts are not Bike+ rides — 95 stretching, 28
//!   yoga, 15 strength, 3 cardio. They are correctly recorded and they are
//!   *"separate entities, we're not modelling them right now"*, so they refuse
//!   as `Unmodelled`: evidence for a later feature rather than data to fix.
//! - A ride whose graph has not landed refuses as `CompanionNotLanded`. The two
//!   streams are collected independently, so this is a reason to run the other
//!   walk, and it goes away when that happens.
//!
//! Neither is dropped (§ 37) and neither is forced into a type it does not fit.
//!
//! **`device_type` alone does not identify the instrument.** A yoga class taken
//! on the bike's screen is also `home_bike_plus`, and 13 of the 15 strength
//! records and all 3 cardio came from the operator's Garmin and synced in
//! through this source. The bike is the instrument only when it is being
//! pedalled, so the discipline, the device and `is_outdoor` are checked
//! together.
//!
//! **The distance comes from the graph.** The workout record states one with no
//! unit anywhere — it is miles, following an account preference — and the graph
//! states the same distance with its unit beside it. Reading the workout's
//! number as kilometres is a silent 38% error that changes meaning if that
//! setting is ever flipped, so this reads the graph's, converts by the unit it
//! declares, and refuses a unit it does not know rather than guessing.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    cycling::{
        BeatsPerMinute, BikePlusRide, ComposedFrom, HeartRateSample, HeartRateSeries, RideRecord,
        RideSample,
    },
    landing::{EventKind, Provenance},
    measure::{Duration, Metres},
    normalised::{OperatorZone, RefusalLocus, RefusalReason, StartedAt},
    sequence::NonEmpty,
};
use jiff::Timestamp;

use crate::{scribe::Scribe, store::PelotonWorkoutSampleLandingStore};

use super::{
    account::RideAccount,
    payload::{PerformanceGraph, WorkoutRecord, number},
};

/// What Peloton calls a ride.
const CYCLING: &str = "cycling";
/// What Peloton calls the Bike+.
const BIKE_PLUS: &str = "home_bike_plus";

/// The four series the bike produces, by the source's name for each.
///
/// `output` is Peloton's word. The domain's is power — the operator, asked:
/// *"I would call it power"* — and the vocabulary stays on this side of the
/// port, which is exactly what an adapter is for.
const POWER: &str = "output";
const CADENCE: &str = "cadence";
const RESISTANCE: &str = "resistance";
const SPEED: &str = "speed";
/// The fifth series, which is not the bike's.
const HEART_RATE: &str = "heart_rate";

/// The Peloton adapter's translator.
///
/// Stateless, as its Hevy counterpart is: the zone arrives per call, so the
/// same translator answers for any declared configuration.
#[derive(Debug, Clone, Copy, Default)]
pub struct PelotonRideTranslator;

impl Translator for PelotonRideTranslator {
    type Account = RideAccount;
    type Entity = BikePlusRide;

    fn translate(
        &self,
        account: &RideAccount,
        zone: &OperatorZone,
    ) -> Result<Translation<BikePlusRide>, NormalisationError> {
        let record = &account.ride;
        let mut scribe = Scribe::new(record);

        // Provenance rather than the body, for the reason the Hevy translator
        // gives: the adapter answered this question when the record landed.
        // This source states no event kinds — a workout that is deleted simply
        // stops being served — so `Updated` is the only kind it writes, and the
        // other arms are what the type makes us say out loud.
        let Provenance::Event(event) = record.provenance();
        match event.kind() {
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

        let workout = match WorkoutRecord::read(record.payload().as_bytes()) {
            Ok(workout) => workout,
            Err(error) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: error.detail,
                    },
                ));
            }
        };

        if let Some(reason) = not_a_bike_plus_ride(&workout) {
            return Ok(scribe.only(RefusalLocus::Record, reason));
        }

        let (instant, duration) = match span_of(&workout) {
            Ok(span) => span,
            Err(reason) => return Ok(scribe.only(RefusalLocus::Record, reason)),
        };

        // Everything above is the workout record's. Everything below needs the
        // graph, which is the second half of the account (§ 3.1).
        let Some(landed_graph) = account.samples.as_ref() else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::CompanionNotLanded {
                    stream: PelotonWorkoutSampleLandingStore::STREAM.to_owned(),
                },
            ));
        };

        let graph = match PerformanceGraph::read(landed_graph.payload().as_bytes()) {
            Ok(graph) => graph,
            Err(error) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: error.detail,
                    },
                ));
            }
        };

        let distance = match distance_of(&graph) {
            Ok(distance) => distance,
            Err(reason) => return Ok(scribe.only(RefusalLocus::Record, reason)),
        };

        let samples = match samples_of(&graph) {
            Ok(samples) => samples,
            Err(reason) => return Ok(scribe.only(RefusalLocus::Record, reason)),
        };
        let Ok(samples) = NonEmpty::new(samples) else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::NoReadingsInSeries {
                    series: "bike sample",
                },
            ));
        };

        // The one part of the graph a ride can do without. A refusal here does
        // not cost the ride, exactly as a refused set does not cost its
        // exercise: the operator wore nothing, or wore something that failed,
        // and the four series the bike produced are unaffected either way.
        let heart_rate = heart_rate_of(&graph, &mut scribe);

        Ok(Translation::Entity {
            entity: Box::new(BikePlusRide::new(RideRecord {
                started_at: StartedAt::new(instant, zone.clone()),
                duration,
                distance,
                samples,
                heart_rate,
                provenance: record.provenance().clone(),
                source_record_id: record.source_record_id().clone(),
                landed_as: ComposedFrom {
                    ride: record.id(),
                    samples: landed_graph.id(),
                },
            })),
            refusals: scribe.into_refusals(),
        })
    }
}

/// When the ride started, and how long it lasted.
///
/// **The ride's own span**, from the workout record: not the class's length,
/// which Peloton also serves and the graph echoes back, and not the span of the
/// samples, which is derivable from them (§ 5). The operator has ridden 82
/// seconds past the end of a class, and those three numbers are three different
/// quantities rather than three accounts of one.
fn span_of(workout: &WorkoutRecord) -> Result<(Timestamp, Duration), RefusalReason> {
    let unreadable =
        |field: &'static str, detail: String| RefusalReason::UnreadableValue { field, detail };

    let (Some(start), Some(end)) = (workout.start_time, workout.end_time) else {
        return Err(unreadable(
            "start_time",
            "a ride with no start or no end".to_owned(),
        ));
    };
    let instant =
        Timestamp::from_second(start).map_err(|_| unreadable("start_time", start.to_string()))?;
    let seconds = end
        .checked_sub(start)
        .filter(|span| *span > 0)
        .ok_or_else(|| unreadable("end_time", format!("{end}, which is not after {start}")))?;
    let duration = u64::try_from(seconds)
        .map(Duration::from_seconds)
        .map_err(|_| unreadable("end_time", end.to_string()))?;

    Ok((instant, duration))
}

/// Why this record is not a ride on a Bike+, if it is not.
///
/// Three questions rather than one, because `device_type` is the platform: a
/// yoga class taken on the bike's screen answers `home_bike_plus` too, and an
/// outdoor ride synced in from a Garmin answers `cycling`.
fn not_a_bike_plus_ride(workout: &WorkoutRecord) -> Option<RefusalReason> {
    let discipline = workout.fitness_discipline.as_deref().unwrap_or("unstated");
    let device = workout
        .device_type
        .as_deref()
        .unwrap_or("an unstated device");

    if workout.is_outdoor == Some(true) {
        return Some(RefusalReason::Unmodelled {
            detail: format!("an outdoor {discipline} workout"),
        });
    }
    if discipline != CYCLING {
        return Some(RefusalReason::Unmodelled {
            detail: format!("a {discipline} workout"),
        });
    }
    if workout.device_type.as_deref() != Some(BIKE_PLUS) {
        return Some(RefusalReason::Unmodelled {
            detail: format!("a cycling workout recorded by {device}"),
        });
    }
    None
}

/// How far the bike said the ride went, in metres.
///
/// **By the unit the graph declares**, never by assuming one. Both units the
/// account preference can produce are exact in millimetres, so neither
/// conversion rounds.
fn distance_of(graph: &PerformanceGraph<'_>) -> Result<Metres, RefusalReason> {
    let unreadable = |detail: String| RefusalReason::UnreadableValue {
        field: "distance",
        detail,
    };

    let summary = graph
        .summary("distance")
        .ok_or_else(|| unreadable("the graph stated no distance".to_owned()))?;
    let value = number(summary.value)
        .ok_or_else(|| unreadable("the graph's distance has no value".to_owned()))?;
    let unit = summary
        .display_unit
        .as_deref()
        .ok_or_else(|| unreadable(format!("{value} in no stated unit")))?;

    let kilometres = match unit {
        "km" => value.to_owned(),
        // 1609.344 metres exactly, so scaling the text keeps every digit.
        "mi" => return miles_to_metres(value).ok_or_else(|| unreadable(format!("{value} {unit}"))),
        other => return Err(unreadable(format!("{value} in unknown unit {other:?}"))),
    };

    // Kilometres to metres, by moving the point rather than multiplying a
    // float: `Metres` parses metres with three decimal places, and a kilometre
    // value of five decimal places is a metre value of two.
    Metres::try_from(shift_point(&kilometres, 3).ok_or_else(|| unreadable(kilometres.clone()))?)
        .map_err(|error| unreadable(error.to_string()))
}

/// A decimal string multiplied by a power of ten, exactly.
///
/// Text rather than arithmetic, so nothing rounds and nothing overflows on the
/// way through a float. `None` where the text is not a plain decimal.
fn shift_point(value: &str, places: usize) -> Option<String> {
    if !value
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
    {
        return None;
    }
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let mut digits = String::from(whole);
    let mut fraction = fraction.chars();
    for _ in 0..places {
        digits.push(fraction.next().unwrap_or('0'));
    }
    let remainder: String = fraction.collect();
    let shifted = digits.trim_start_matches('0');
    let shifted = if shifted.is_empty() { "0" } else { shifted };
    Some(if remainder.is_empty() {
        shifted.to_owned()
    } else {
        format!("{shifted}.{remainder}")
    })
}

/// A decimal string of miles, in metres.
///
/// A mile is 1609.344 metres exactly, so this is a multiplication by 1609344
/// and a shift of three places — done on the digits for the reason
/// [`shift_point`] exists.
fn miles_to_metres(value: &str) -> Option<Metres> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 6 || !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut micro_miles = String::from(whole);
    micro_miles.push_str(fraction);
    for _ in fraction.len()..6 {
        micro_miles.push('0');
    }
    let micro_miles: u64 = micro_miles.parse().ok()?;
    // A mile is 1_609_344 millimetres, so millimetres are micro-miles scaled by
    // that and divided back down by the million. Multiplying before dividing is
    // what keeps it exact, and u64 holds it: a million miles is well inside it.
    let millimetres = micro_miles.checked_mul(1_609_344)?.checked_div(1_000_000)?;
    Some(Metres::from_millimetres(millimetres))
}

/// The bike's four series, zipped onto the offsets they were recorded at.
///
/// **All four or none.** They are one measurement by one method at one index;
/// a ride missing one of them is not this entity with a column empty, and
/// admitting it would mean a speed with no cadence to have been computed from.
fn samples_of(graph: &PerformanceGraph<'_>) -> Result<Vec<RideSample>, RefusalReason> {
    let series = |slug: &'static str| {
        graph
            .metric(slug)
            .ok_or(RefusalReason::MissingSeries { series: slug })
    };
    let power = series(POWER)?;
    let cadence = series(CADENCE)?;
    let resistance = series(RESISTANCE)?;
    let speed = series(SPEED)?;

    let offsets = &graph.seconds_since_pedaling_start;
    for (slug, metric) in [
        (POWER, power),
        (CADENCE, cadence),
        (RESISTANCE, resistance),
        (SPEED, speed),
    ] {
        if metric.values.len() != offsets.len() {
            return Err(RefusalReason::UnreadableValue {
                field: "values",
                detail: format!(
                    "{slug} has {} readings for {} seconds",
                    metric.values.len(),
                    offsets.len()
                ),
            });
        }
    }

    let mut samples = Vec::with_capacity(offsets.len());
    for (index, offset) in offsets.iter().enumerate() {
        let at = read(number(Some(offset)), "seconds_since_pedaling_start")?;
        samples.push(RideSample {
            at,
            power: read(number(power.values.get(index).copied()), POWER)?,
            cadence: read(number(cadence.values.get(index).copied()), CADENCE)?,
            resistance: read(number(resistance.values.get(index).copied()), RESISTANCE)?,
            speed: read(number(speed.values.get(index).copied()), SPEED)?,
        });
    }
    Ok(samples)
}

/// One value, as the domain type it belongs to.
///
/// Generic over the type rather than written five times: every quantity here
/// validates itself from the characters the source wrote, and the only thing
/// this adds is naming the field a refusal blames.
fn read<T>(value: Option<&str>, field: &'static str) -> Result<T, RefusalReason>
where
    T: TryFrom<String>,
    T::Error: std::fmt::Display,
{
    let value = value.ok_or_else(|| RefusalReason::UnreadableValue {
        field: "values",
        detail: format!("{field} has a reading that is absent or null"),
    })?;
    T::try_from(value.to_owned()).map_err(|error| RefusalReason::UnreadableValue {
        field: "values",
        detail: format!("{field}: {error}"),
    })
}

/// The heart-rate series, where the watch broadcast one.
///
/// **A zero is not a reading**, and `BeatsPerMinute` cannot hold one, so a
/// second the watch was silent contributes no sample instead of contributing a
/// zero. That is § 37 rather than tidiness: an averaging function over a series
/// containing 2,037 zeros answers 31 bpm for a ride the operator rode at 128.
///
/// The source's own declaration of how much it did not measure is carried
/// beside the samples. It is not derivable from them — it counts seconds
/// Peloton held a value forward for as well as the ones it zeroed — so it is
/// recorded rather than recomputed, and a value that is not a duration is
/// refused without costing the series.
fn heart_rate_of(graph: &PerformanceGraph<'_>, scribe: &mut Scribe) -> Option<HeartRateSeries> {
    let metric = graph.metric(HEART_RATE)?;
    let offsets = &graph.seconds_since_pedaling_start;

    if metric.values.len() != offsets.len() {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::UnreadableValue {
                field: "values",
                detail: format!(
                    "heart_rate has {} readings for {} seconds",
                    metric.values.len(),
                    offsets.len()
                ),
            },
        );
        return None;
    }

    let mut samples = Vec::new();
    for (index, offset) in offsets.iter().enumerate() {
        let Ok(at) = read::<Duration>(number(Some(offset)), "seconds_since_pedaling_start") else {
            continue;
        };
        let Some(reading) = number(metric.values.get(index).copied()) else {
            continue;
        };
        // The zeros land here, and `BeatsPerMinute` is what refuses them.
        if let Ok(beats_per_minute) = BeatsPerMinute::try_from(reading.to_owned()) {
            samples.push(HeartRateSample {
                at,
                beats_per_minute,
            });
        }
    }

    let Ok(samples) = NonEmpty::new(samples) else {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::NoReadingsInSeries {
                series: "heart rate",
            },
        );
        return None;
    };

    let declared_missing = number(metric.missing_data_duration).and_then(|stated| {
        Duration::try_from(stated.to_owned())
            .map_err(|error| {
                scribe.note(
                    RefusalLocus::Record,
                    RefusalReason::UnreadableValue {
                        field: "missing_data_duration",
                        detail: error.to_string(),
                    },
                );
            })
            .ok()
    });

    Some(HeartRateSeries::new(samples, declared_missing))
}
