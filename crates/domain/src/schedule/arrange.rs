//! Which discipline and which session each free slot of a week takes (#673).
//!
//! **Worked out, never stated.** The operator, 2026-10-31: a pattern says when
//! he is free and whether each slot is longer or shorter — *"evenings are
//! shorter sessions and sunday morning is longer, it doesn't matter how
//! long"* — and the rest follows from rules. Moving Friday evening to Saturday
//! evening is then one changed slot, and the week rearranges itself.
//!
//! The rules, in the order they are applied:
//!
//! 1. **At most [`Limits`]**: a session a day and six days a week. Where a day
//!    has to go it is the last of the week, so the next starts fresh.
//! 2. **The slots a day uses are the ones that make the shortest gap in the
//!    week as long as possible**, counted in parts of a day. With Friday and
//!    Sunday evenings fixed and all of Saturday free, Saturday evening wins: a
//!    morning and an afternoon either side.
//! 3. **The gym takes half the sessions, rounded up, and cycling the rest, and
//!    they alternate.**
//! 4. **A longer slot takes the long, easier ride** where cycling has two or
//!    more rides and one of them can have it; with two, the later. Every other
//!    ride is the shorter, harder one, and a single ride is the harder one.
//! 5. **The harder gym session is the one with the most recovery before it**,
//!    counted between sessions rather than calendar days. That is the
//!    full-body split of one or two gym sessions; three alternate their
//!    primaries (#676), so a role is not this module's to give them.
//!
//! **A week is read as though it repeats**, so the gap before Monday's session
//! is from the week's last. That is what makes Sunday morning to Monday evening
//! a shorter recovery than Wednesday to Friday.

use std::collections::BTreeMap;

use super::{Allocation, Discipline, PartOfDay, Relative, SessionRole, TrainingSlot};

/// Whether a slot has room for the longer session of a week or only the
/// shorter. How long exactly does not matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotLength {
    Shorter,
    Longer,
}

impl SlotLength {
    pub const ALL: &'static [Self] = &[Self::Shorter, Self::Longer];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Shorter => "shorter",
            Self::Longer => "longer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is neither longer nor shorter")]
pub struct UnknownSlotLength {
    value: String,
}

impl TryFrom<String> for SlotLength {
    type Error = UnknownSlotLength;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|length| length.as_str() == value)
            .copied()
            .ok_or(UnknownSlotLength { value })
    }
}

impl TryFrom<&str> for SlotLength {
    type Error = UnknownSlotLength;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl std::str::FromStr for SlotLength {
    type Err = UnknownSlotLength;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl std::fmt::Display for SlotLength {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How much of the free time a week may use.
///
/// **Parameters, not constants**: the operator stated them on 2026-10-31 and
/// said they are liable to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub sessions_a_day: u8,
    pub days_a_week: u8,
}

impl Limits {
    /// What the operator stated on 2026-10-31.
    pub const STATED: Self = Self {
        sessions_a_day: 1,
        days_a_week: 6,
    };
}

impl Default for Limits {
    fn default() -> Self {
        Self::STATED
    }
}

/// The shorter, harder session.
const HARDER: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);
/// The longer, easier session.
const EASIER: SessionRole = SessionRole::new(Relative::Lower, Relative::Higher);

/// Parts of a day in a week, which the gaps are counted in.
const PARTS_A_WEEK: usize = 21;

/// Where a slot sits in the week, in parts of a day from Monday morning.
fn position(slot: TrainingSlot) -> usize {
    let part = match slot.part {
        PartOfDay::Morning => 0,
        PartOfDay::Afternoon => 1,
        PartOfDay::Evening => 2,
    };
    usize::try_from(slot.weekday.to_monday_zero_offset()).unwrap_or(0) * 3 + part
}

/// The parts of a day between each session and the one before it, the first
/// reaching back to the week's last.
fn gaps_before(positions: &[usize]) -> Vec<usize> {
    let Some(last) = positions.last() else {
        return Vec::new();
    };
    let mut previous = *last;
    positions
        .iter()
        .map(|at| {
            let gap = (at + PARTS_A_WEEK - previous - 1) % PARTS_A_WEEK;
            previous = *at;
            // A single session has the whole rest of the week either side.
            if positions.len() == 1 {
                PARTS_A_WEEK - 1
            } else {
                gap
            }
        })
        .collect()
}

/// Every way of choosing `take` of `from`, in order.
fn choices(from: &[TrainingSlot], take: usize) -> Vec<Vec<TrainingSlot>> {
    if take == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for (at, slot) in from.iter().enumerate() {
        let rest = from.get(at + 1..).unwrap_or_default();
        for mut tail in choices(rest, take - 1) {
            tail.insert(0, *slot);
            all.push(tail);
        }
    }
    all
}

/// The disciplines of a week's sessions, and the ride that is the long one.
///
/// The gym takes the first of an odd number, so it has the extra one. With an
/// even number either can go first, and the one that lets a longer slot take
/// the long ride wins — the later long ride where both do, and the gym first
/// where neither does.
fn disciplines(slots: &[(TrainingSlot, SlotLength)]) -> (Vec<Discipline>, Option<usize>) {
    let alternate = |gym_first: bool| -> Vec<Discipline> {
        (0..slots.len())
            .map(|at| {
                if (at % 2 == 0) == gym_first {
                    Discipline::Gym
                } else {
                    Discipline::Cycling
                }
            })
            .collect()
    };
    let long_ride = |held: &[Discipline]| -> Option<usize> {
        let rides = held.iter().filter(|d| **d == Discipline::Cycling).count();
        if rides < 2 {
            return None;
        }
        slots
            .iter()
            .zip(held)
            .enumerate()
            .filter(|(_, ((_, length), discipline))| {
                **discipline == Discipline::Cycling && *length == SlotLength::Longer
            })
            .map(|(at, _)| at)
            .next_back()
    };

    let gym_first = alternate(true);
    let gym_long = long_ride(&gym_first);
    if slots.len() % 2 == 1 {
        return (gym_first, gym_long);
    }
    let cycling_first = alternate(false);
    let cycling_long = long_ride(&cycling_first);
    match (gym_long, cycling_long) {
        (Some(gym), Some(cycling)) if cycling > gym => (cycling_first, cycling_long),
        (None, Some(_)) => (cycling_first, cycling_long),
        _ => (gym_first, gym_long),
    }
}

/// Which discipline and which session each free slot of a week takes.
///
/// A slot the week does not use is not in the answer. A gym session is given
/// no role where the gym has more than two, because which session each one is
/// depends on a split that is not built yet (#676).
#[must_use]
pub fn arrange(
    free: &BTreeMap<TrainingSlot, SlotLength>,
    limits: Limits,
) -> BTreeMap<TrainingSlot, (Discipline, Option<SessionRole>)> {
    // Monday first, which `TrainingSlot` orders by, so equal days are adjacent
    // and the days that go are the last of the week.
    let mut days: Vec<Vec<TrainingSlot>> = Vec::new();
    for slot in free.keys() {
        match days.last_mut() {
            Some(day)
                if day
                    .first()
                    .is_some_and(|first| first.weekday == slot.weekday) =>
            {
                day.push(*slot);
            }
            _ => days.push(vec![*slot]),
        }
    }
    days.truncate(usize::from(limits.days_a_week));

    let per_day = usize::from(limits.sessions_a_day);
    let mut weeks: Vec<Vec<TrainingSlot>> = vec![Vec::new()];
    for day in &days {
        let options = choices(day, per_day.min(day.len()));
        weeks = weeks
            .iter()
            .flat_map(|week| {
                options.iter().map(move |option| {
                    let mut next = week.clone();
                    next.extend(option);
                    next
                })
            })
            .collect();
    }

    // Best first: the shortest gap longest, then a long ride where one can be
    // had, then the next shortest gap and so on, then the later parts of a
    // day, which is when an ordinary week trains. **The long ride comes before
    // balancing the rest**: a completely free week is six mornings, because
    // only the mornings give Saturday's long ride a longer slot, though
    // evenings from Tuesday would even out the gaps either side of Monday.
    let chosen = weeks
        .into_iter()
        .map(|week| {
            let positions: Vec<usize> = week.iter().map(|slot| position(*slot)).collect();
            let mut gaps = gaps_before(&positions);
            gaps.sort_unstable();
            let shortest = gaps.first().copied().unwrap_or(PARTS_A_WEEK);
            let with_lengths: Vec<(TrainingSlot, SlotLength)> = week
                .iter()
                .map(|slot| {
                    (
                        *slot,
                        free.get(slot).copied().unwrap_or(SlotLength::Shorter),
                    )
                })
                .collect();
            let has_long_ride = disciplines(&with_lengths).1.is_some();
            ((shortest, has_long_ride, gaps, positions), with_lengths)
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, week)| week)
        .unwrap_or_default();

    let (held, long_ride) = disciplines(&chosen);
    let positions: Vec<usize> = chosen.iter().map(|(slot, _)| position(*slot)).collect();
    let recovery = gaps_before(&positions);

    let gym: Vec<usize> = held
        .iter()
        .enumerate()
        .filter(|(_, discipline)| **discipline == Discipline::Gym)
        .map(|(at, _)| at)
        .collect();
    // The later of two with equal recovery, as Friday is in a week of Monday,
    // Wednesday and Friday evenings with nothing on the Sunday.
    let harder_gym = gym
        .iter()
        .copied()
        .max_by_key(|at| (recovery.get(*at).copied().unwrap_or(0), *at));

    chosen
        .iter()
        .zip(&held)
        .enumerate()
        .map(|(at, ((slot, _), discipline))| {
            let role = match discipline {
                Discipline::Cycling if Some(at) == long_ride => Some(EASIER),
                Discipline::Cycling => Some(HARDER),
                Discipline::Gym if gym.len() > 2 => None,
                Discipline::Gym if Some(at) == harder_gym => Some(HARDER),
                Discipline::Gym => Some(EASIER),
            };
            (*slot, (*discipline, role))
        })
        .collect()
}

/// [`arrange`], as the allocations a week's slots carry.
#[must_use]
pub fn allocations(
    free: &BTreeMap<TrainingSlot, SlotLength>,
    limits: Limits,
) -> BTreeMap<TrainingSlot, Allocation> {
    arrange(free, limits)
        .into_iter()
        .map(|(slot, (discipline, role))| (slot, Allocation { discipline, role }))
        .collect()
}
