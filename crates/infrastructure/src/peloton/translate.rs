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
        BeatsPerMinute, BikePlusRide, ComposedFrom, HeartRateSample, HeartRateSeries,
        PerformedSession, RideRecord, RideSample,
    },
    landing::{EventKind, Provenance},
    measure::{Duration, Metres},
    normalised::{OperatorZone, RefusalLocus, RefusalReason, StartedAt},
    sequence::NonEmpty,
};
use jiff::Timestamp;

use crate::{scribe::Scribe, store::PelotonRideSampleLandingStore};

use super::{
    account::{LandedRide, SessionAccount},
    payload::{PerformanceGraph, WorkoutRecord, number},
    sessions::{RideRole, is_low_impact, role_of},
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
pub struct PelotonSessionTranslator;

impl Translator for PelotonSessionTranslator {
    type Account = SessionAccount;
    type Entity = PerformedSession;

    fn translate(
        &self,
        account: &SessionAccount,
        zone: &OperatorZone,
    ) -> Result<Translation<PerformedSession>, NormalisationError> {
        // Anchored to the session's first ride: a refusal has to point the
        // operator at something they can look up, and a session is not
        // something the source names.
        let mut scribe = Scribe::new(&account.rides().first().ride);

        // A retraction is the source withdrawing a record, and this source
        // never does — a workout that is deleted simply stops being served. The
        // arm exists because the type makes us say so, and it withdraws the
        // whole session, because a session missing one of its rides is not that
        // session.
        for landed in account.rides().iter() {
            let Provenance::Event(event) = landed.ride.provenance();
            match event.kind() {
                EventKind::Deleted => {
                    return Ok(Translation::Retraction {
                        of: landed.ride.source_record_id().clone(),
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

        // Every ride, or none. A session with one ride missing is not a session
        // with a gap in it — it is a different session, and asserting it would
        // be this layer inventing what the source did not say.
        let mut rides = Vec::with_capacity(account.rides().count());
        for landed in account.rides().iter() {
            match ride_from(landed, zone) {
                Ok((ride, noted)) => {
                    for reason in noted {
                        scribe.note(RefusalLocus::Record, reason);
                    }
                    rides.push(ride);
                }
                Err(reason) => return Ok(scribe.only(RefusalLocus::Record, reason)),
            }
        }

        let rides_in_order: Vec<&LandedRide> = account.rides().iter().collect();
        let roles: Vec<RideRole> = rides_in_order
            .iter()
            .map(|landed| effective_role(landed, &rides_in_order))
            .collect();

        let Some(session) = session_from(roles.as_slice(), rides) else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::Unmodelled {
                    detail: described(roles.as_slice()),
                },
            ));
        };

        Ok(Translation::Entity {
            entity: Box::new(session),
            refusals: scribe.into_refusals(),
        })
    }
}

/// The role a ride plays in the session it sits in.
///
/// **Position matters as well as declaration.** A low-impact class is an
/// ordinary ride when it stands alone and a cool-down when a session ends with
/// it — which is the operator's own reading of the FTP test he finished with
/// one: *"looks like I picked the wrong type of ride for a Cool Down"*. So the
/// class says what it is and the sequence says what it was for.
fn effective_role(landed: &LandedRide, rides: &[&LandedRide]) -> RideRole {
    let role = role_of(&landed.ride);
    let is_last = rides
        .last()
        .is_some_and(|last| last.ride.id() == landed.ride.id());
    if role == RideRole::Main && is_last && rides.len() > 1 && is_low_impact(&landed.ride) {
        return RideRole::CoolDown;
    }
    role
}

/// The session those roles make, if they make one this model holds.
///
/// The two variants and nothing else. Both anomalies in the operator's record
/// fall out here — a warm-up and a cool-down with no ride between them
/// (2023-12-21, his first session) and two main rides back to back (2023-12-24,
/// from the Discover programme) — and he called both anomalies rather than
/// shapes to model.
fn session_from(roles: &[RideRole], mut rides: Vec<BikePlusRide>) -> Option<PerformedSession> {
    let mut take = |index: usize| -> BikePlusRide { rides.remove(index) };
    match roles {
        [RideRole::Main] => Some(PerformedSession::Ride {
            main: take(0),
            cool_down: None,
        }),
        [RideRole::Main, RideRole::CoolDown] => {
            let main = take(0);
            Some(PerformedSession::Ride {
                main,
                cool_down: Some(take(0)),
            })
        }
        [RideRole::WarmUp, RideRole::Effort] => {
            let warm_up = take(0);
            Some(PerformedSession::Test {
                warm_up,
                effort: take(0),
                cool_down: None,
            })
        }
        [RideRole::WarmUp, RideRole::Effort, RideRole::CoolDown] => {
            let warm_up = take(0);
            let effort = take(0);
            Some(PerformedSession::Test {
                warm_up,
                effort,
                cool_down: Some(take(0)),
            })
        }
        _ => None,
    }
}

/// What the refused shape was, in words an operator can act on.
fn described(roles: &[RideRole]) -> String {
    if roles == [RideRole::NotARide] {
        return "not a ride on a Bike+, so part of no cycling session".to_owned();
    }
    let named: Vec<&str> = roles
        .iter()
        .map(|role| match role {
            RideRole::WarmUp => "warm-up",
            RideRole::Effort => "FTP test",
            RideRole::Main => "main",
            RideRole::CoolDown => "cool-down",
            RideRole::NotARide => "not a ride",
        })
        .collect();
    format!("a session of {}", named.join(" then "))
}

/// One ride, from the workout record and the graph that carries its samples.
fn ride_from(
    landed: &LandedRide,
    zone: &OperatorZone,
) -> Result<(BikePlusRide, Vec<RefusalReason>), RefusalReason> {
    let record = &landed.ride;
    let workout = WorkoutRecord::read(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.detail,
        }
    })?;

    if let Some(reason) = not_a_bike_plus_ride(&workout) {
        return Err(reason);
    }

    let (instant, duration) = span_of(&workout)?;

    // Everything above is the workout record's. Everything below needs the
    // graph, which is the other half of what the source says about this ride.
    let landed_graph =
        landed
            .samples
            .as_ref()
            .ok_or_else(|| RefusalReason::CompanionNotLanded {
                stream: PelotonRideSampleLandingStore::STREAM.to_owned(),
            })?;

    let graph = PerformanceGraph::read(landed_graph.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.detail,
        }
    })?;

    let distance = distance_of(&graph)?;
    let samples = samples_of(&graph)?;
    let samples = NonEmpty::new(samples).map_err(|_| RefusalReason::NoReadingsInSeries {
        series: "bike sample",
    })?;

    // The one part of the graph a ride can do without: the operator wore
    // nothing, or wore something that failed, and the four series the bike
    // produced are unaffected either way. A problem with it is carried back
    // rather than raised, so it costs the declaration and not the ride.
    let (heart_rate, noted) = heart_rate_of(&graph);

    Ok((
        BikePlusRide::new(RideRecord {
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
        }),
        noted,
    ))
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
fn heart_rate_of(graph: &PerformanceGraph<'_>) -> (Option<HeartRateSeries>, Vec<RefusalReason>) {
    let mut noted = Vec::new();
    let Some(metric) = graph.metric(HEART_RATE) else {
        return (None, noted);
    };
    let offsets = &graph.seconds_since_pedaling_start;

    if metric.values.len() != offsets.len() {
        noted.push(RefusalReason::UnreadableValue {
            field: "values",
            detail: format!(
                "heart_rate has {} readings for {} seconds",
                metric.values.len(),
                offsets.len()
            ),
        });
        return (None, noted);
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
        noted.push(RefusalReason::NoReadingsInSeries {
            series: "heart rate",
        });
        return (None, noted);
    };

    let declared_missing = number(metric.missing_data_duration).and_then(|stated| {
        match Duration::try_from(stated.to_owned()) {
            Ok(duration) => Some(duration),
            Err(error) => {
                noted.push(RefusalReason::UnreadableValue {
                    field: "missing_data_duration",
                    detail: error.to_string(),
                });
                None
            }
        }
    });

    (Some(HeartRateSeries::new(samples, declared_missing)), noted)
}
