//! Building the canonical gym session from whichever normalised records hold
//! it (#247).
//!
//! § II.4's two halves: deterministic matching decides which normalised
//! accounts are accounts of one visit, and the merge assembles one session
//! from them field by field. Both are code and neither consults an overlay
//! (§ 9).
//!
//! **The day is the match.** No source in the operator's record holds two gym
//! sessions on one day — not the watch across 478 sessions, not Hevy across
//! 143 — so a day names a visit, and the sources that hold a clock agree to
//! within 33 minutes on the 133 days two of them recorded. The exception is
//! the spreadsheets, which hold two accounts on 20 days: 13 are `1RM.xlsx`
//! beside `Strength training 2019.xlsx`, two workbooks of one visit, and 7 are
//! one workbook deriving one session twice (#298). Both want merging, so
//! neither is a second visit.
//!
//! **A day-dated account may belong to the day before or after.** The operator's
//! sheets name a day in their title and he did not always train that day:
//! `Bench day (2019-03-06)` is the watch's activity on the 7th and
//! `Deadlift day (2019-03-17)` the 18th. So a group of accounts that holds no
//! clock at all joins an adjacent day where exactly one adjacent day holds
//! one. It fires twice on the record and never ambiguously — no day-only group
//! has a clocked day on both sides.
//!
//! **A visit with no exercises is no canonical session.** The operator,
//! 2026-10-01: *"A canonical gym session must have exercises and may have
//! heart rate data. Heart rate only isn't a meaningful gym session."* 160 of
//! his 551 gym days are a watch recording and nothing else, and they are
//! normalised sessions with no canonical one rather than canonical sessions
//! with a hole in them (§ 37).

use std::collections::BTreeMap;

use jiff::civil::Date;

use crate::canonical::{Attributed, NormalisedSessionId, Occurred};
use crate::measure::PositiveDuration;
use crate::sequence::{AtLeastTwo, NonEmpty};

use super::{
    canonical::{CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified},
    load::Load,
    measured::MeasuredHeartRate,
    normalised::{NormalisedGymSession, Recorder},
    outcome::Performed,
};

/// Every canonical gym session the normalised layer gives, oldest first.
///
/// `accounts` is the whole normalised layer. The order within a day decides
/// nothing — every tie is broken on the normalised session's id — but the
/// result is ordered by when each visit happened.
pub fn canonical_sessions(accounts: Vec<NormalisedGymSession>) -> Vec<CanonicalGymSession> {
    let mut visits = group_by_day(accounts);
    migrate_day_only_groups(&mut visits);
    visits
        .into_values()
        .filter_map(|accounts| merge(&accounts))
        .collect()
}

/// The accounts of each day, oldest day first.
fn group_by_day(accounts: Vec<NormalisedGymSession>) -> BTreeMap<Date, Vec<NormalisedGymSession>> {
    let mut days: BTreeMap<Date, Vec<NormalisedGymSession>> = BTreeMap::new();
    for account in accounts {
        days.entry(account.occurred().day())
            .or_default()
            .push(account);
    }
    for group in days.values_mut() {
        group.sort_by_key(NormalisedGymSession::id);
    }
    days
}

/// Move each day-only group onto the adjacent day, where exactly one adjacent
/// day holds a clock and this one holds none.
fn migrate_day_only_groups(visits: &mut BTreeMap<Date, Vec<NormalisedGymSession>>) {
    let moves: Vec<(Date, Date)> = visits
        .iter()
        .filter(|(_, group)| !group.iter().any(is_clocked))
        .filter_map(|(day, _)| adjacent_clocked_day(visits, *day).map(|to| (*day, to)))
        .collect();
    for (from, to) in moves {
        let Some(group) = visits.remove(&from) else {
            continue;
        };
        let destination = visits.entry(to).or_default();
        destination.extend(group);
        destination.sort_by_key(NormalisedGymSession::id);
    }
}

/// The one adjacent day holding a clock, where there is exactly one.
fn adjacent_clocked_day(
    visits: &BTreeMap<Date, Vec<NormalisedGymSession>>,
    day: Date,
) -> Option<Date> {
    let mut found = None;
    for offset in [-1, 1] {
        let Ok(neighbour) = day.checked_add(jiff::Span::new().days(offset)) else {
            continue;
        };
        let clocked = visits
            .get(&neighbour)
            .is_some_and(|group| group.iter().any(is_clocked));
        if clocked {
            if found.is_some() {
                // A day with a clocked day on each side. Nothing decides
                // between them, so the overlay's job rather than a guess.
                return None;
            }
            found = Some(neighbour);
        }
    }
    found
}

const fn is_clocked(account: &NormalisedGymSession) -> bool {
    account.occurred().instant().is_some()
}

/// One visit's accounts, merged. [`None`] where none of them holds an exercise.
fn merge(accounts: &[NormalisedGymSession]) -> Option<CanonicalGymSession> {
    let items = NonEmpty::new(merge_items(accounts)).ok()?;
    let occurred = accounts
        .iter()
        .map(NormalisedGymSession::occurred)
        .min_by(|left, right| finest(left, right))
        .cloned()?;
    Some(CanonicalGymSession::new(
        occurred,
        items,
        heart_rate(accounts),
        duration(accounts),
    ))
}

/// An instant before a day, and the earlier instant before the later one.
///
/// A day and an instant on it do not disagree — [`Occurred`] says so — so the
/// finer account is not being preferred over a rival, it is the same fact
/// known more precisely.
fn finest(left: &Occurred, right: &Occurred) -> std::cmp::Ordering {
    match (left.instant(), right.instant()) {
        (Some(left), Some(right)) => left.instant().cmp(&right.instant()),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// The visit's heart rate: one account's whole, never merged.
///
/// § 10 keeps method-dependent quantities (§ 6) out of the merge — there is no
/// single value two accounts of a heart rate could reduce to — and only a
/// watch records one, so on this record there is never more than one to choose
/// between. The lowest id where there is, so a rebuild gives the same answer.
fn heart_rate(accounts: &[NormalisedGymSession]) -> Option<Attributed<MeasuredHeartRate>> {
    accounts
        .iter()
        .filter_map(NormalisedGymSession::heart_rate)
        .min_by_key(|rate| rate.normalised_session())
        .cloned()
}

/// How long the visit took: the longest account of it.
///
/// **Not a disagreement to settle.** A watch states how long it recorded for
/// and Hevy how long the logging ran; each is a floor on the visit, and the
/// visit is at least as long as the longest of them. Ties go to the lowest id.
fn duration(accounts: &[NormalisedGymSession]) -> Option<Attributed<PositiveDuration>> {
    accounts
        .iter()
        .filter_map(NormalisedGymSession::duration)
        .max_by_key(|stated| {
            (
                stated.copied().as_seconds(),
                -stated.normalised_session().as_i64(),
            )
        })
        .copied()
}

/// Where in a visit an exercise sits: its key, and which time round it is.
///
/// **An exercise key is not unique within a session.** A session can return to
/// a lift after another — the watch's own grouping says so, since consecutive
/// sets it placed the same way are one exercise and a later run of them is
/// another — so the nth `back-squat-barbell` of one account corresponds to the
/// nth of another, and the key alone would merge two separate blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Slot {
    key: Option<&'static str>,
    occurrence: usize,
}

/// One account's exercises, each with the slot it fills and the item it sits
/// in, worked out once.
struct Flattened<'a> {
    id: NormalisedSessionId,
    recorder: Recorder,
    /// Slot, the exercise, and which of this account's items holds it.
    exercises: Vec<(Slot, &'a CanonicalExercise, usize)>,
    /// Which items this account recorded as supersets.
    supersets: Vec<usize>,
}

impl<'a> Flattened<'a> {
    fn of(account: &'a NormalisedGymSession) -> Self {
        let mut seen: BTreeMap<Option<&'static str>, usize> = BTreeMap::new();
        let mut exercises = Vec::new();
        let mut supersets = Vec::new();
        for (index, item) in account.items().iter().enumerate() {
            if matches!(item, CanonicalItem::Superset(_)) {
                supersets.push(index);
            }
            for exercise in item.exercises() {
                let key = exercise.exercise_key();
                let occurrence = seen.entry(key).or_default();
                exercises.push((
                    Slot {
                        key,
                        occurrence: *occurrence,
                    },
                    exercise,
                    index,
                ));
                *occurrence += 1;
            }
        }
        Self {
            id: account.id(),
            recorder: account.recorder(),
            exercises,
            supersets,
        }
    }

    fn version_of(&self, slot: Slot) -> Option<&'a CanonicalExercise> {
        self.exercises
            .iter()
            .find(|(held, _, _)| *held == slot)
            .map(|(_, exercise, _)| *exercise)
    }

    /// The slots this account performed back to back with `slot`, where it
    /// recorded a superset containing it.
    fn superset_with(&self, slot: Slot) -> Option<Vec<Slot>> {
        let (_, _, item) = self
            .exercises
            .iter()
            .find(|(held, _, _)| *held == slot)
            .copied()?;
        if !self.supersets.contains(&item) {
            return None;
        }
        Some(
            self.exercises
                .iter()
                .filter(|(_, _, held)| *held == item)
                .map(|(slot, _, _)| *slot)
                .collect(),
        )
    }
}

/// Every item of the visit, in the order performed.
fn merge_items(accounts: &[NormalisedGymSession]) -> Vec<CanonicalItem> {
    let mut flattened: Vec<Flattened<'_>> = accounts.iter().map(Flattened::of).collect();
    realign_guesses(&mut flattened);
    let spine = spine(&flattened);
    let mut items = Vec::with_capacity(spine.len());
    let mut done: Vec<Slot> = Vec::new();

    for slot in spine {
        if done.contains(&slot) {
            continue;
        }
        // A superset is a thing a source recorded, never worked out here, so
        // one account recording it is enough and its members are that
        // account's. Lowest id where two did, as every other tie is broken.
        let grouped = flattened
            .iter()
            .find_map(|account| account.superset_with(slot));
        match grouped {
            Some(members) if members.len() > 1 => {
                let merged: Vec<CanonicalExercise> = members
                    .iter()
                    .filter_map(|member| merge_slot(&flattened, *member))
                    .collect();
                done.extend(members);
                match AtLeastTwo::new(merged) {
                    Ok(members) => items.push(CanonicalItem::Superset(Box::new(members))),
                    // Every member but one dropped out, so what is left was
                    // not performed back to back with anything.
                    Err(_) => {
                        if let Some(one) = merge_slot(&flattened, slot) {
                            items.push(CanonicalItem::Exercise(one));
                        }
                    }
                }
            }
            _ => {
                done.push(slot);
                if let Some(exercise) = merge_slot(&flattened, slot) {
                    items.push(CanonicalItem::Exercise(exercise));
                }
            }
        }
    }
    items
}

/// A watch's guess never names an exercise the operator's own record already
/// accounts for.
///
/// **Because a guess is not a claim about identity.** 2019-03-14: the watch
/// classified three sets of 10 × 70 as `BARBELL_DEADLIFT` and the operator's
/// own sheet for that block calls the same three sets Romanian deadlifts. Two
/// keys, one exercise, and keying the slot on the name alone put both in the
/// canonical session — 23 sets where the visit had 19, with three of them
/// counted twice.
///
/// So where a watch's sets coincide with a recorded exercise's, set for set,
/// that is the exercise it was of, and the log names it. `Identified` already
/// says why the other way round is not available: *"Flattening them would let
/// a reader take `BARBELL_DEADLIFT` for the operator's word when his own sheet
/// for that block says Romanian deadlift."*
///
/// **Set for set, and nothing weaker.** A watch exercise that merely overlaps
/// a log's is the ordinary case — the watch records sets a sheet summarised
/// away — and on this record it always shares the log's key, so there is
/// nothing to realign and no reason to risk pairing two exercises that only
/// resemble each other.
fn realign_guesses(flattened: &mut [Flattened<'_>]) {
    let recorded: Vec<(Slot, &CanonicalExercise)> = flattened
        .iter()
        .filter(|account| account.recorder == Recorder::Operator)
        .flat_map(|account| {
            account
                .exercises
                .iter()
                .map(|(slot, exercise, _)| (*slot, *exercise))
        })
        .collect();
    if recorded.is_empty() {
        return;
    }
    for account in flattened
        .iter_mut()
        .filter(|account| account.recorder == Recorder::Watch)
    {
        let mut claimed: Vec<Slot> = Vec::new();
        for (slot, exercise, _) in &mut account.exercises {
            if recorded.iter().any(|(held, _)| held == slot) {
                // The watch's own name for it is already the log's.
                claimed.push(*slot);
                continue;
            }
            let found = recorded
                .iter()
                .find(|(held, theirs)| !claimed.contains(held) && same_sets(theirs, exercise));
            if let Some((held, _)) = found {
                claimed.push(*held);
                *slot = *held;
            }
        }
    }
}

/// The order the visit's slots are in.
///
/// **The fullest account's order, then whatever it did not hold.** The account
/// with the most exercises is the one that saw most of the visit, so its order
/// is the visit's; a slot only another account holds goes after it, in that
/// account's own order. Ties on count go to the lowest id, so the spine does
/// not depend on the order rows arrived in.
fn spine(flattened: &[Flattened<'_>]) -> Vec<Slot> {
    let mut ordered: Vec<&Flattened<'_>> = flattened.iter().collect();
    ordered.sort_by_key(|account| {
        (
            std::cmp::Reverse(account.exercises.len()),
            account.id.as_i64(),
        )
    });
    let mut spine: Vec<Slot> = Vec::new();
    for account in ordered {
        for (slot, _, _) in &account.exercises {
            if !spine.contains(slot) {
                spine.push(*slot);
            }
        }
    }
    spine
}

/// One slot, merged from every account that holds it.
fn merge_slot(flattened: &[Flattened<'_>], slot: Slot) -> Option<CanonicalExercise> {
    let mut versions: Vec<Account<'_>> = flattened
        .iter()
        .filter_map(|account| {
            account.version_of(slot).map(|exercise| Account {
                exercise,
                recorder: account.recorder,
                id: account.id,
            })
        })
        .collect();
    corroborated(&mut versions);
    merged(&versions)
}

/// One account's version of a slot.
#[derive(Clone, Copy)]
struct Account<'a> {
    exercise: &'a CanonicalExercise,
    recorder: Recorder,
    id: NormalisedSessionId,
}

/// Drop the versions a second account does not back where another is backed.
///
/// **The rule 2019-03-14 wants.** Three accounts of that visit: `1RM.xlsx`
/// has the face pulls at 27, 27, 27 and the front squats at 40 × 3,
/// `Strength training 2019.xlsx` has 32, 32, 27 and 45, 45, 45 × 12, and the
/// watch has what the second sheet has. The first is the block's template and
/// the second is what he performed, and nothing about either *file* says which
/// — what says it is that a third account holds one of them and not the other.
/// The operator, in #247: *"The canonical session takes the sets that agree
/// with Garmin."*
///
/// Where nothing corroborates anything, every version stands and the merge
/// keeps what each adds. That is the ordinary case: a sheet recording the top
/// set of an exercise the watch recorded whole.
fn corroborated(versions: &mut Vec<Account<'_>>) {
    if versions.len() < 3 {
        // Two accounts cannot corroborate each other against a third that is
        // not there, and one needs no filtering.
        return;
    }
    let backed: Vec<(Account<'_>, usize)> = versions
        .iter()
        .map(|version| {
            let backing = versions
                .iter()
                .filter(|other| {
                    other.id != version.id && same_sets(version.exercise, other.exercise)
                })
                .count();
            (*version, backing)
        })
        .collect();
    let most = backed
        .iter()
        .map(|(_, backing)| *backing)
        .max()
        .unwrap_or(0);
    if most == 0 {
        return;
    }
    *versions = backed
        .into_iter()
        .filter(|(_, backing)| *backing == most)
        .map(|(version, _)| version)
        .collect();
}

/// Whether two accounts of one exercise say the same thing, set for set.
///
/// Set for set, because this decides corroboration and an account that agrees
/// about three sets of four is not another account of the same thing — it is
/// the other case, a fuller record of it.
fn same_sets(left: &CanonicalExercise, right: &CanonicalExercise) -> bool {
    match (left, right) {
        (
            CanonicalExercise::ForReps { sets: left, .. },
            CanonicalExercise::ForReps { sets: right, .. },
        ) => pairwise(
            &left.iter().collect::<Vec<_>>(),
            &right.iter().collect::<Vec<_>>(),
        ),
        (
            CanonicalExercise::ForDuration { sets: left, .. },
            CanonicalExercise::ForDuration { sets: right, .. },
        ) => pairwise(
            &left.iter().collect::<Vec<_>>(),
            &right.iter().collect::<Vec<_>>(),
        ),
        (
            CanonicalExercise::ForDistance { sets: left, .. },
            CanonicalExercise::ForDistance { sets: right, .. },
        ) => pairwise(
            &left.iter().collect::<Vec<_>>(),
            &right.iter().collect::<Vec<_>>(),
        ),
        _ => false,
    }
}

fn pairwise<M: Copy + PartialEq>(left: &[&CanonicalSet<M>], right: &[&CanonicalSet<M>]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| same_measure(left, right) && same_load(left, right))
}

/// Whether two sets record the same amount of work, where both say.
///
/// A set whose count nothing states — the gym's log writing "Burpee" in a
/// round — neither agrees nor disagrees with one that has a count, so it is
/// not evidence either way.
fn same_measure<M: Copy + PartialEq>(left: &CanonicalSet<M>, right: &CanonicalSet<M>) -> bool {
    match (left.outcome.value(), right.outcome.value()) {
        (Performed::Completed(Some(left)), Performed::Completed(Some(right))) => left == right,
        (Performed::Completed(None), Performed::Completed(_))
        | (Performed::Completed(_), Performed::Completed(None))
        | (Performed::Failed, Performed::Failed) => true,
        _ => false,
    }
}

/// Whether two sets' loads are one value.
fn same_load<M>(left: &CanonicalSet<M>, right: &CanonicalSet<M>) -> bool {
    match (
        left.load.as_ref().map(Attributed::copied),
        right.load.as_ref().map(Attributed::copied),
    ) {
        (Some(left), Some(right)) => one_value(left, right),
        (None, None) => true,
        _ => false,
    }
}

/// Whether two loads are the same load recorded at two resolutions.
///
/// The operator, 2026-10-03: *"the watch input only allows for integers, kg or
/// lbs, if Garmin has a decimal kg value I must have updated it after the
/// fact. So, 43kg Vs 42.5kg is no disagreement, it's the same value at
/// different levels of resolution."*
///
/// So a load the watch holds as a whole kilogramme stands for anything within
/// half a kilogramme of it, and one it holds as a whole pound for anything
/// within half a pound — 131 of its 2,610 loads are pounds converted, and
/// 2019-11-01's 18.13, 24.06 and 24.94 kg are 40, 53 and 55 lb. Two figures
/// that differ by more than the dial could have rounded away are a real
/// difference and this says so.
fn one_value(left: Load, right: Load) -> bool {
    if left == right {
        return true;
    }
    let (Load::Absolute(left), Load::Absolute(right)) = (left, right) else {
        // A relative load is a delta the operator typed; no watch records one,
        // so there is no coarser account of it to allow for.
        return false;
    };
    let (left, right) = (left.as_grams(), right.as_grams());
    let apart = left.abs_diff(right);
    (dialled_in_kilogrammes(left) || dialled_in_kilogrammes(right)) && apart < GRAMS_IN_KILOGRAMME
        || (dialled_in_pounds(left) || dialled_in_pounds(right)) && apart < GRAMS_IN_POUND
}

const GRAMS_IN_KILOGRAMME: u64 = 1_000;
/// A pound in grams, to the gram: 0.45359237 kg exactly, by definition.
const GRAMS_IN_POUND: u64 = 454;

const fn dialled_in_kilogrammes(grams: u64) -> bool {
    grams.is_multiple_of(GRAMS_IN_KILOGRAMME)
}

/// Whether a figure is a whole number of pounds, within the rounding that
/// converting it to grams leaves behind.
fn dialled_in_pounds(grams: u64) -> bool {
    let pounds = (grams * 100).div_euclid(GRAMS_IN_POUND * 100);
    [pounds, pounds + 1]
        .into_iter()
        .any(|whole| (whole * GRAMS_IN_POUND).abs_diff(grams) <= 5)
}

/// What is known about the account a value came from, for the one decision
/// that needs it.
#[derive(Clone, Copy)]
struct Meta {
    recorder: Recorder,
    /// Whether this account's own loads change across the exercise's sets.
    ///
    /// The operator, 2026-10-03, on a load that differs between two accounts
    /// of one set: *"A change of weight within a exercise sounds like a record
    /// of performance. I'm unlikely to have prescribed 2x 12 @ 40kg + 1x 12 @
    /// 42.5kg, but I am likely to have changed within the workout because 40kg
    /// was too easy."* So where two figures really differ, the account that
    /// shows the bar changing is the one recording what happened.
    varies: bool,
}

/// Every surviving version of one slot, merged field by field.
fn merged(versions: &[Account<'_>]) -> Option<CanonicalExercise> {
    let (base, rest) = fullest(versions)?;
    let meta: BTreeMap<NormalisedSessionId, Meta> = versions
        .iter()
        .map(|version| {
            (
                version.id,
                Meta {
                    recorder: version.recorder,
                    varies: varies(version.exercise),
                },
            )
        })
        .collect();

    let mut exercise = base.exercise.clone();
    for other in rest {
        exercise = fold(exercise, other.exercise, &meta);
    }
    Some(exercise)
}

/// The version that saw most of the exercise, and the rest in id order.
fn fullest<'v>(versions: &'v [Account<'v>]) -> Option<(&'v Account<'v>, Vec<&'v Account<'v>>)> {
    let mut ordered: Vec<&Account<'_>> = versions.iter().collect();
    ordered.sort_by_key(|version| {
        (
            std::cmp::Reverse(version.exercise.set_count()),
            version.id.as_i64(),
        )
    });
    let (base, rest) = ordered.split_first()?;
    Some((base, rest.to_vec()))
}

/// Whether an account's loads change across the exercise's sets.
fn varies(exercise: &CanonicalExercise) -> bool {
    let loads: Vec<Option<Load>> = match exercise {
        CanonicalExercise::ForReps { sets, .. } => sets.iter().map(load_of).collect(),
        CanonicalExercise::ForDuration { sets, .. } => sets.iter().map(load_of).collect(),
        CanonicalExercise::ForDistance { sets, .. } => sets.iter().map(load_of).collect(),
    };
    loads.iter().zip(loads.iter().skip(1)).any(|(a, b)| a != b)
}

fn load_of<M>(set: &CanonicalSet<M>) -> Option<Load> {
    set.load.as_ref().map(Attributed::copied)
}

/// One more account folded into the exercise so far.
fn fold(
    into: CanonicalExercise,
    other: &CanonicalExercise,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> CanonicalExercise {
    match (into, other) {
        (
            CanonicalExercise::ForReps { identified, sets },
            CanonicalExercise::ForReps {
                identified: theirs,
                sets: other,
            },
        ) => CanonicalExercise::ForReps {
            identified: name(identified, theirs),
            sets: align(sets, other, meta),
        },
        (
            CanonicalExercise::ForDuration { exercise, sets },
            CanonicalExercise::ForDuration { sets: other, .. },
        ) => CanonicalExercise::ForDuration {
            exercise,
            sets: align(sets, other, meta),
        },
        (
            CanonicalExercise::ForDistance { exercise, sets },
            CanonicalExercise::ForDistance { sets: other, .. },
        ) => CanonicalExercise::ForDistance {
            exercise,
            sets: align(sets, other, meta),
        },
        // Two accounts whose measures differ are not accounts of one exercise,
        // whatever key they share. What is already built stands.
        (built, _) => built,
    }
}

/// Which account's name for the exercise the canonical one takes.
///
/// **A record beats a guess.** A log says what the operator did; a watch's
/// classifier says what it made of it, and `Identified` keeps them apart
/// precisely so the second cannot pass for the first. Between two records, or
/// two guesses, the lowest id.
fn name(mine: Attributed<Identified>, theirs: &Attributed<Identified>) -> Attributed<Identified> {
    match (mine.copied().is_recorded(), theirs.copied().is_recorded()) {
        (true, false) => mine,
        (false, true) => *theirs,
        _ if theirs.normalised_session() < mine.normalised_session() => *theirs,
        _ => mine,
    }
}

/// Another account's sets, aligned onto the sets so far and merged into them.
///
/// **Aligned on what was done, not on position.** A sheet records the top set
/// of an exercise the watch recorded whole — 2018-04-28's bench press is 4 ×
/// 36 in the sheet and 10 × 40, 6 × 40, 4 × 36 on the watch — so the sheet's
/// one set is the watch's third and not its first. A set that aligns with
/// nothing is a set only this account saw, and it is kept.
fn align<M: Copy + PartialEq>(
    sets: NonEmpty<CanonicalSet<M>>,
    other: &NonEmpty<CanonicalSet<M>>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> NonEmpty<CanonicalSet<M>> {
    /// A set built so far, and whether another account has already been
    /// merged into it. Taken rather than removed, so order is preserved.
    struct Built<M> {
        set: CanonicalSet<M>,
        taken: bool,
    }

    let mut built: Vec<Built<M>> = sets
        .iter()
        .cloned()
        .map(|set| Built { set, taken: false })
        .collect();
    let mut extra: Vec<CanonicalSet<M>> = Vec::new();

    for theirs in other.iter() {
        let free = |built: &Built<M>| !built.taken && same_measure(&built.set, theirs);
        // One whose load also agrees first, so a set is not merged into a
        // heavier one of the same count while its own twin waits.
        let chosen = built
            .iter()
            .position(|one| free(one) && same_load(&one.set, theirs))
            .or_else(|| built.iter().position(free));
        match chosen.and_then(|index| built.get_mut(index)) {
            Some(one) => {
                one.taken = true;
                one.set = combine(&one.set, theirs, meta);
            }
            None => extra.push(theirs.clone()),
        }
    }

    let mut merged: Vec<CanonicalSet<M>> = built.into_iter().map(|one| one.set).collect();
    merged.extend(extra);
    NonEmpty::new(merged).unwrap_or(sets)
}

/// Two accounts of one set, field by field.
fn combine<M: Copy + PartialEq>(
    mine: &CanonicalSet<M>,
    theirs: &CanonicalSet<M>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> CanonicalSet<M> {
    CanonicalSet {
        outcome: outcome(&mine.outcome, &theirs.outcome, meta),
        load: load(mine.load.as_ref(), theirs.load.as_ref(), meta),
        began: either(mine.began.as_ref(), theirs.began.as_ref(), meta),
        intensity: either(mine.intensity.as_ref(), theirs.intensity.as_ref(), meta),
        kind: either(mine.kind.as_ref(), theirs.kind.as_ref(), meta),
        rest_after: either(mine.rest_after.as_ref(), theirs.rest_after.as_ref(), meta),
    }
}

/// What became of the set: a stated amount over an unstated one.
fn outcome<M: Copy>(
    mine: &Attributed<Performed<Option<M>>>,
    theirs: &Attributed<Performed<Option<M>>>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> Attributed<Performed<Option<M>>> {
    match (mine.value(), theirs.value()) {
        (Performed::Completed(None), Performed::Completed(Some(_))) => *theirs,
        (Performed::Completed(Some(_)), Performed::Completed(None)) => *mine,
        _ => *prefer(mine, theirs, meta),
    }
}

/// The set's load.
///
/// Where the two are one value the finer figure stands and nothing is being
/// chosen between. Where they really differ, the account whose loads change
/// across the exercise is the one recording what happened; failing that, the
/// watch, which reads the bar as the set is performed.
fn load(
    mine: Option<&Attributed<Load>>,
    theirs: Option<&Attributed<Load>>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> Option<Attributed<Load>> {
    let (Some(mine), Some(theirs)) = (mine, theirs) else {
        return mine.or(theirs).copied();
    };
    if one_value(mine.copied(), theirs.copied()) {
        return Some(*finer(mine, theirs, meta));
    }
    let varies = |value: &Attributed<Load>| {
        meta.get(&value.normalised_session())
            .is_some_and(|meta| meta.varies)
    };
    match (varies(mine), varies(theirs)) {
        (true, false) => Some(*mine),
        (false, true) => Some(*theirs),
        _ => Some(*recorded_by(mine, theirs, meta, Recorder::Watch)),
    }
}

/// Of two accounts of one load that agree, the one that states it exactly.
///
/// The operator's own record, where it has one: only the watch's figure is
/// rounded to its dial, so the other is the same value stated to the
/// kilogramme the bar actually held.
fn finer<'v>(
    mine: &'v Attributed<Load>,
    theirs: &'v Attributed<Load>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> &'v Attributed<Load> {
    recorded_by(mine, theirs, meta, Recorder::Operator)
}

/// Whichever of the two came from the given kind of record, else the lowest id.
fn recorded_by<'v, T>(
    mine: &'v Attributed<T>,
    theirs: &'v Attributed<T>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
    wanted: Recorder,
) -> &'v Attributed<T> {
    let is = |value: &Attributed<T>| {
        meta.get(&value.normalised_session())
            .is_some_and(|meta| meta.recorder == wanted)
    };
    match (is(mine), is(theirs)) {
        (true, false) => mine,
        (false, true) => theirs,
        _ => prefer(mine, theirs, meta),
    }
}

/// Whichever account states the field at all; the operator's where both do.
fn either<T: Clone>(
    mine: Option<&Attributed<T>>,
    theirs: Option<&Attributed<T>>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> Option<Attributed<T>> {
    match (mine, theirs) {
        (Some(mine), Some(theirs)) => {
            Some(recorded_by(mine, theirs, meta, Recorder::Operator).clone())
        }
        (one, other) => one.or(other).cloned(),
    }
}

/// The operator's record over a watch's, and the lowest id where that does not
/// decide it — so a rebuild gives the same answer.
fn prefer<'v, T>(
    mine: &'v Attributed<T>,
    theirs: &'v Attributed<T>,
    meta: &BTreeMap<NormalisedSessionId, Meta>,
) -> &'v Attributed<T> {
    let operator = |value: &Attributed<T>| {
        meta.get(&value.normalised_session())
            .is_some_and(|meta| meta.recorder == Recorder::Operator)
    };
    match (operator(mine), operator(theirs)) {
        (true, false) => mine,
        (false, true) => theirs,
        _ if theirs.normalised_session() < mine.normalised_session() => theirs,
        _ => mine,
    }
}
