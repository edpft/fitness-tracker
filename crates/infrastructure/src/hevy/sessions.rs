//! Which of Hevy's workouts make up one of our sessions.
//!
//! **This is where Hevy's record boundary stops showing through.** The operator,
//! 2026-09-08:
//!
//! > on the gym side, there was a time when I used different Hevy routines to
//! > programme different parts of my workout so I could compose them. they were
//! > all still part of the same gym session.
//!
//! Constitution § 3.1 makes the session the normalised unit, and grouping is
//! deterministic translation (§ 9): recorded values and nothing else.
//!
//! **One signal, because Hevy publishes one.** [`super::sessions`]'s Peloton
//! counterpart has three — a `workout_type`, a `class_type_ids` and a
//! `series_id` — and reads a role off each. Hevy's payload has nine keys, none
//! naming a kind, and `routine_id` is null on 155 of the operator's 167 records
//! because he deleted the routines those sessions were composed from. So the
//! only thing that says two workouts were one session is that they were
//! performed back to back, and the record says it loudly.
//!
//! Nothing here matches on a title. A title is copy, and here it is copy the
//! operator wrote himself.

use domain::landing::LandedRecord;

use super::{
    account::{SessionAccount, supersede},
    payload::WorkoutEnvelope,
};

/// How long a break makes the next workout a different session.
///
/// **Thirty minutes, and his record makes the choice free.** Across the
/// operator's 167 workouts every same-day break — the end of one workout to the
/// start of the next — falls between 3 seconds and 8 minutes 20, and 14 of the
/// 27 are under eleven seconds: he finished one routine and started the next
/// immediately. The shortest break *between* days is 34 hours. So every
/// threshold from nine minutes to a day groups his history identically, and
/// this one is not fitted to anything.
///
/// **The same number as cycling's, for the same reason rather than by sharing
/// one.** How long a break makes the next thing a different session is a fact
/// about how this person trains, and neither discipline's record argues with
/// half an hour — but they are separate constants because they are separate
/// facts, and a day the gym wants a different answer should not have to move
/// the bike's.
///
/// **Measured end to start.** Issue #104 originally reported these breaks as
/// 7 to 46 minutes and concluded that the gym needed a number of its own; that
/// list was each workout's start minus the *previous workout's start*, so it
/// counted the previous workout as part of the gap.
const SESSION_BREAK_SECONDS: i64 = 1_800;

/// When a workout started and ended, in Unix seconds.
///
/// `None` where the payload states neither, which leaves the workout ungroupable
/// and alone. That is every deleted event: Hevy serves a deletion as an id and a
/// timestamp with no workout body at all, so there is nothing to place it beside
/// — and it needs none, because a retraction withdraws whichever session
/// composed the record it names, wherever that session sat.
fn span_of(record: &LandedRecord) -> Option<(i64, i64)> {
    let envelope = WorkoutEnvelope::read(record.payload().as_bytes()).ok()?;
    let workout = envelope.workout?;
    let start: jiff::Timestamp = workout.start_time.parse().ok()?;
    let end: jiff::Timestamp = workout.end_time?.parse().ok()?;
    Some((start.as_second(), end.as_second()))
}

/// Group landed workouts into the sessions they were performed as.
///
/// **Every workout ends up in exactly one account**, including the ones that
/// will be refused. A record that vanished during grouping would be a record
/// with no outcome, which § 38's reconciliation exists to catch and which
/// grouping is the likeliest place to introduce.
pub fn group(records: Vec<LandedRecord>) -> Vec<SessionAccount> {
    // Before anything else: one serving per workout (§ 10). Grouping a
    // re-serving as a second part would invent a session nobody trained.
    let (records, superseded) = supersede(records);

    let mut dated: Vec<(Option<(i64, i64)>, LandedRecord)> = records
        .into_iter()
        .map(|record| (span_of(&record), record))
        .collect();
    // By start, so adjacency is the order it happened in rather than the order
    // it landed in. A record with no time sorts to the front — `None` orders
    // before `Some` — and groups with nothing either way: it breaks from
    // whatever precedes it, and whatever follows breaks from it.
    dated.sort_by_key(|(span, _)| span.map(|(start, _)| start));

    let mut grouped: Vec<Vec<LandedRecord>> = Vec::new();
    let mut current: Vec<LandedRecord> = Vec::new();
    let mut ended_at: Option<i64> = None;

    for (span, record) in dated {
        let breaks = match (span, ended_at) {
            (Some((start, _)), Some(previous)) => {
                start.saturating_sub(previous) >= SESSION_BREAK_SECONDS
            }
            // Either this workout states no span or the one before it did not.
            // Neither can be placed beside the other, so the session ends here.
            _ => true,
        };

        if breaks && !current.is_empty() {
            grouped.push(std::mem::take(&mut current));
        }
        ended_at = span.map(|(_, end)| end);
        current.push(record);
    }
    if !current.is_empty() {
        grouped.push(current);
    }

    // Each superseded serving is filed with the account holding the serving
    // that replaced it, which is the only place it can be counted without being
    // mistaken for a part of something.
    let mut superseded = superseded;
    grouped
        .into_iter()
        .filter_map(|records| {
            let ids: Vec<String> = records
                .iter()
                .map(|record| record.source_record_id().as_str().to_owned())
                .collect();
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut superseded)
                .into_iter()
                .partition(|record| {
                    ids.iter()
                        .any(|id| id == record.source_record_id().as_str())
                });
            superseded = rest;
            SessionAccount::of(records, mine)
        })
        .collect()
}
