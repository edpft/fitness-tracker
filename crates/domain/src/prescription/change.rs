//! A slot's fill, changed partway through a mesocycle (#454).
//!
//! The operator's superset pairings follow the gym's layout and his
//! preference, and change when either does: from 2026-10-16 a band-assisted
//! front lever raise replaced the pull-ups, which put the dips back on bands.
//! A change holds from its date. A session prescribed before it keeps what it
//! was prescribed, because a prescription is never rewritten.
//!
//! **A change names the exercise and what assists it**, since a prescription
//! names a specific exercise: a band-assisted dip and a machine-assisted dip
//! are one exercise performed and two prescribed (the operator, 2026-10-15).

use jiff::{Timestamp, civil::Date};

use crate::gym::{Exercise, equipment::Specific};

use super::{Fill, SlotFills, SlotId};

/// One slot's fill from a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillChange {
    pub slot: SlotId,
    pub from: Date,
    pub recorded_at: Timestamp,
    pub exercise: Exercise,
    /// The specific that assists it, where one does.
    pub assisted_by: Option<Specific>,
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

    /// The change in force for a slot on a date.
    #[must_use]
    pub fn on(&self, slot: SlotId, date: Date) -> Option<&FillChange> {
        self.0
            .iter()
            .rev()
            .find(|change| change.slot == slot && change.from <= date)
    }

    /// The fills as they stood on a date: a changed slot holds its change for
    /// both roles.
    #[must_use]
    pub fn apply(&self, fills: &SlotFills, date: Date) -> SlotFills {
        let mut changed = fills.clone();
        for slot in SlotId::ALL {
            if let Some(change) = self.on(*slot, date)
                && let Some(fill) = changed.single_mut(*slot)
            {
                *fill = Fill::Same(change.exercise);
            }
        }
        changed
    }

    /// What assists a slot on a date, if a change says.
    #[must_use]
    pub fn assisted_by(&self, slot: SlotId, date: Date) -> Option<&Specific> {
        self.on(slot, date)
            .and_then(|change| change.assisted_by.as_ref())
    }

    pub fn iter(&self) -> impl Iterator<Item = &FillChange> {
        self.0.iter()
    }
}
