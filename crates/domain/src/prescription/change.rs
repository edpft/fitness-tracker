//! A slot's fill, changed partway through a mesocycle (#454).
//!
//! The operator's superset pairings follow the gym's layout and his
//! preference, and change when either does: from 2026-10-16 a band-assisted
//! front lever raise replaced the pull-ups, which put the dips back on bands.
//! A change holds from its date. A session prescribed before it keeps what it
//! was prescribed, because a prescription is never rewritten.
//!
//! **A change names the exercise, what assists it and what loads it**, since a
//! prescription names a specific exercise: a band-assisted dip and a
//! machine-assisted dip are one exercise performed and two prescribed (the
//! operator, 2026-10-15). Either side of zero may be stated or not, so a dip
//! is assisted below and loaded above, a dead bug only loaded, and a pogo
//! neither (the operator, 2026-10-26).
//!
//! **A change is for one session's intensity, or both.** A slot can alternate
//! — back extension on the heavier session, lying leg curl on the lighter —
//! and a change to one side must leave the other as it was.

use jiff::{Timestamp, civil::Date};

use crate::{
    gym::{Exercise, equipment::Specific, exercise::Implement},
    schedule::{Relative, SessionRole},
};

use super::{ByIntensity, Fill, SlotFills, SlotId};

/// One slot's fill from a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillChange {
    pub slot: SlotId,
    /// The session it changes, by intensity; `None` for both.
    pub intensity: Option<Relative>,
    pub from: Date,
    pub recorded_at: Timestamp,
    pub exercise: Exercise,
    /// The range of bands that assists it, where one does.
    pub assisted_by: Option<Specific>,
    /// The kind held to load it above bodyweight, where one is: its steps are
    /// that kind's scale.
    pub loaded_by: Option<Implement>,
}

/// Every change to one mesocycle's fills.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FillChanges(Vec<FillChange>);

impl FillChanges {
    /// Ordered by start; of two starting together, the later recorded stands.
    #[must_use]
    pub fn new(mut changes: Vec<FillChange>) -> Self {
        changes.sort_by(|a, b| a.from.cmp(&b.from).then(a.recorded_at.cmp(&b.recorded_at)));
        Self(changes)
    }

    /// The change in force for a slot's session of one intensity on a date.
    #[must_use]
    pub fn on(&self, slot: SlotId, intensity: Relative, date: Date) -> Option<&FillChange> {
        self.0.iter().rev().find(|change| {
            change.slot == slot
                && change.from <= date
                && change.intensity.is_none_or(|changed| changed == intensity)
        })
    }

    /// The last change for a slot's session of one intensity, whatever date
    /// it holds from: what a slot is to hold, where it differs from what it
    /// holds now.
    #[must_use]
    pub fn latest(&self, slot: SlotId, intensity: Relative) -> Option<&FillChange> {
        self.0.iter().rev().find(|change| {
            change.slot == slot && change.intensity.is_none_or(|changed| changed == intensity)
        })
    }

    /// The fills as they stood on a date. A slot changed for both
    /// intensities alike holds one exercise; otherwise each keeps its own.
    #[must_use]
    pub fn apply(&self, fills: &SlotFills, date: Date) -> SlotFills {
        let mut changed = fills.clone();
        for slot in SlotId::ALL {
            let lower = self.on(*slot, Relative::Lower, date);
            let higher = self.on(*slot, Relative::Higher, date);
            if lower.is_none() && higher.is_none() {
                continue;
            }
            let Some(fill) = changed.single_mut(*slot) else {
                continue;
            };
            let held = |intensity: Relative| match &*fill {
                Fill::Same(exercise) => *exercise,
                Fill::Alternating(by) => match intensity {
                    Relative::Lower => by.lower,
                    Relative::Higher => by.higher,
                },
            };
            let lower = lower.map_or_else(|| held(Relative::Lower), |change| change.exercise);
            let higher = higher.map_or_else(|| held(Relative::Higher), |change| change.exercise);
            *fill = if lower == higher {
                Fill::Same(lower)
            } else {
                Fill::Alternating(ByIntensity { lower, higher })
            };
        }
        changed
    }

    /// What assists a slot in a session on a date, if a change says.
    #[must_use]
    pub fn assisted_by(&self, slot: SlotId, role: SessionRole, date: Date) -> Option<&Specific> {
        self.on(slot, role.intensity(), date)
            .and_then(|change| change.assisted_by.as_ref())
    }

    /// What loads a slot above bodyweight in a session on a date, if a change
    /// says.
    #[must_use]
    pub fn loaded_by(&self, slot: SlotId, role: SessionRole, date: Date) -> Option<Implement> {
        self.on(slot, role.intensity(), date)
            .and_then(|change| change.loaded_by)
    }

    /// These changes, followed by a later mesocycle's own (#578).
    ///
    /// A change holds from its date until another replaces it, so a plan's
    /// later mesocycle carries every change made in an earlier one: its fills
    /// were stated with the plan, before any change was. A plan authored afresh
    /// states its fills again, and carries nothing.
    #[must_use]
    pub fn followed_by(&self, later: &Self) -> Self {
        Self::new(self.0.iter().chain(&later.0).cloned().collect())
    }

    pub fn iter(&self) -> impl Iterator<Item = &FillChange> {
        self.0.iter()
    }
}
