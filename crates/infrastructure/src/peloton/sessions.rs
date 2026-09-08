//! Which of Peloton's workouts make up one of our sessions.
//!
//! **This is where Peloton's architecture stops showing through.** The operator,
//! 2026-09-08:
//!
//! > when we prescribe a cycling session, we prescribe a main ride and a cool
//! > down ride because, from our perspective, they're the same thing. This is
//! > most evident with the FTP warm up and FTP test, we would never consider
//! > these to be two separate things that could be planned separately but
//! > Peloton does split them.
//!
//! Constitution § 3.1 makes the session the normalised unit, and grouping is
//! deterministic translation (§ 9): source identity plus recorded values, with
//! no further input. Everything here reads what the source stated.
//!
//! **Three signals, all Peloton's own.**
//!
//! - `workout_type` separates a class from a freestyle ride. A freestyle ride
//!   carries no class at all, so nothing says what it was for; it is never part
//!   of a session and is refused on its own.
//! - `class_type_ids` names the *Cool Down Ride* type — the same id
//!   [`super::class`] already uses to find one to prescribe, read off Peloton's
//!   own `class_types` list rather than derived from the words "cool down".
//! - `series_id` names the kind of class. It is what recognises an FTP warm-up
//!   and an FTP test, and the operator is why: *"Peloton publishes lots of
//!   different FTP warm up and FTP test rides"*, so there is no list of class
//!   ids to hold. His six tests, spread over 31 months, used five distinct test
//!   classes and one series.
//!
//! Nothing here matches on a title. A title is copy; a series is a statement.

use std::collections::BTreeSet;

use domain::landing::LandedRecord;

use super::{
    account::{LandedRide, SessionAccount, supersede},
    class::COOL_DOWN_RIDE_CLASS_TYPE,
    payload::WorkoutRecord,
};

/// Peloton's series for its FTP test rides.
pub(crate) const FTP_TEST_SERIES: &str = "7609c9f02ed644e58104af7a8337125c";
/// Peloton's series for the warm-up ridden before one.
pub(crate) const FTP_WARM_UP_SERIES: &str = "ad6c6bc2e8ce4304bb6839f690038271";
/// Peloton's own id for the *Low Impact Ride* class type.
///
/// Here because a low-impact ride is sometimes ridden *as* a cool-down. The
/// operator, on the FTP test he finished with one: *"looks like I picked the
/// wrong type of ride for a Cool Down, not something we should try to model
/// explicitly, though I guess we'll need to allow the cool down to be a low
/// impact ride too so we don't lose this FTP test"*.
pub(crate) const LOW_IMPACT_RIDE_CLASS_TYPE: &str = "59a49f882ea9475faa3110d50a8fb3f3";

/// What Peloton calls a ride that was not a class.
const FREESTYLE: &str = "freestyle";
/// What Peloton calls a ride.
const CYCLING: &str = "cycling";
/// What Peloton calls the Bike+.
const BIKE_PLUS: &str = "home_bike_plus";

/// How long a break makes the next ride a different session.
///
/// **Thirty minutes, and the record makes the choice nearly free.** Across the
/// operator's 285 rides, 129 of the 284 gaps are under five minutes and 153 are
/// over two hours; only two sit between, at 5m10s and 14m29s. Anything from a
/// quarter of an hour to two hours groups his history identically. Thirty
/// minutes joins the first — an FTP test to its cool-down — and splits the
/// second, which is a cool-down followed by a mobility video.
///
/// It is a fact about how a person trains rather than about Peloton, which is
/// why it is a stated constant with its reasoning attached rather than a number
/// fitted to the data.
const SESSION_BREAK_SECONDS: i64 = 1_800;

/// What a ride was, as far as a session is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RideRole {
    WarmUp,
    Effort,
    Main,
    CoolDown,
    /// Not a Bike+ ride at all. Never part of a cycling session.
    ///
    /// A freestyle ride with no class, and everything Peloton files as a
    /// workout that is not cycling on a Bike+ — 141 of the operator's 426, all
    /// of them stretching, yoga, strength or cardio. **They must not group**:
    /// he stretches straight after riding, and letting a stretching class join
    /// the ride before it would turn a session into a shape no variant holds
    /// and refuse the ride along with it.
    NotARide,
}

/// What one landed workout record says it was.
pub(crate) fn role_of(record: &LandedRecord) -> RideRole {
    let Ok(workout) = WorkoutRecord::read(record.payload().as_bytes()) else {
        // Unreadable here is not a decision: the translator reads it again and
        // refuses it with the reason. Grouping only has to not swallow it.
        return RideRole::NotARide;
    };
    if workout.workout_type.as_deref() == Some(FREESTYLE) {
        return RideRole::NotARide;
    }
    // The same three questions the translator asks, for the same reason:
    // `device_type` is the platform, so a yoga class taken on the bike's screen
    // answers `home_bike_plus` too.
    if workout.fitness_discipline.as_deref() != Some(CYCLING)
        || workout.device_type.as_deref() != Some(BIKE_PLUS)
        || workout.is_outdoor == Some(true)
    {
        return RideRole::NotARide;
    }
    let Some(class) = workout.ride.as_ref() else {
        return RideRole::NotARide;
    };

    let types: BTreeSet<&str> = class.class_type_ids.iter().map(String::as_str).collect();
    if types.contains(COOL_DOWN_RIDE_CLASS_TYPE) {
        return RideRole::CoolDown;
    }
    match class.series_id.as_deref() {
        Some(FTP_TEST_SERIES) => RideRole::Effort,
        Some(FTP_WARM_UP_SERIES) => RideRole::WarmUp,
        _ => RideRole::Main,
    }
}

/// Whether this ride is a low-impact class, which a session may end with in
/// place of a cool-down.
pub(crate) fn is_low_impact(record: &LandedRecord) -> bool {
    WorkoutRecord::read(record.payload().as_bytes()).is_ok_and(|workout| {
        workout.ride.is_some_and(|class| {
            class
                .class_type_ids
                .iter()
                .any(|id| id == LOW_IMPACT_RIDE_CLASS_TYPE)
        })
    })
}

/// A ride with what grouping needs of it: when it happened and what it was.
type Dated = (Option<(i64, i64)>, RideRole, LandedRide);

/// When a ride started and ended, in Unix seconds. `None` where the source
/// stated neither, which leaves the ride ungroupable and alone.
fn span_of(record: &LandedRecord) -> Option<(i64, i64)> {
    let workout = WorkoutRecord::read(record.payload().as_bytes()).ok()?;
    Some((workout.start_time?, workout.end_time?))
}

/// Group landed rides into the sessions they were ridden as.
///
/// **Every ride ends up in exactly one account**, including the ones that will
/// be refused. A record that vanished during grouping would be a record with no
/// outcome, which § 38's reconciliation exists to catch and which grouping is
/// the likeliest place to introduce.
///
/// Anything that is not a Bike+ ride is an account of one. The operator's own
/// record is why: he stretches straight after riding, so 95 stretching classes
/// sit within half an hour of the ride before them; and two freestyle rides sit
/// inside a session's span — a two-minute Just Ride started by mistake, and a
/// mobility video watched on the bike after a cool-down, *"conceptually the
/// same as a main ride + cool down + stretch or yoga"*. Grouping any of them in
/// would refuse the ride they sat beside.
pub fn group(rides: Vec<LandedRide>) -> Vec<SessionAccount> {
    // Before anything else: one serving per ride (§ 10). Grouping a re-serving
    // as a second ride would invent a session nobody rode, and leaving one
    // ungrouped would let its graph be counted twice.
    let (rides, superseded) = supersede(rides);

    let mut dated: Vec<Dated> = rides
        .into_iter()
        .map(|ride| (span_of(&ride.ride), role_of(&ride.ride), ride))
        .collect();
    // By start, so adjacency is the order it happened in rather than the order
    // it landed in. A ride the source gave no time for sorts last and groups
    // with nothing.
    dated.sort_by_key(|(span, _, _)| span.map(|(start, _)| start));

    let mut grouped: Vec<Vec<LandedRide>> = Vec::new();
    let mut current: Vec<LandedRide> = Vec::new();
    let mut ended_at: Option<i64> = None;

    for (span, role, ride) in dated {
        let breaks = match (span, ended_at) {
            (Some((start, _)), Some(previous)) => {
                start.saturating_sub(previous) >= SESSION_BREAK_SECONDS
            }
            _ => true,
        };

        if role == RideRole::NotARide {
            let held = std::mem::take(&mut current);
            if !held.is_empty() {
                grouped.push(held);
            }
            ended_at = span.map(|(_, end)| end);
            grouped.push(vec![ride]);
            continue;
        }

        if breaks && !current.is_empty() {
            grouped.push(std::mem::take(&mut current));
        }
        ended_at = span.map(|(_, end)| end);
        current.push(ride);
    }
    if !current.is_empty() {
        grouped.push(current);
    }

    // Each superseded serving is filed with the account holding the serving
    // that replaced it, which is the only place it can be counted without
    // being mistaken for a part of something.
    let mut superseded = superseded;
    grouped
        .into_iter()
        .filter_map(|rides| {
            let ids: BTreeSet<String> = rides
                .iter()
                .map(|ride| ride.ride.source_record_id().as_str().to_owned())
                .collect();
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut superseded)
                .into_iter()
                .partition(|ride| ids.contains(ride.ride.source_record_id().as_str()));
            superseded = rest;
            SessionAccount::of(rides, mine)
        })
        .collect()
}
