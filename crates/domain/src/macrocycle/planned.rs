//! The planned macrocycle: the whole term, each mesocycle planned, in progress
//! or completed (#789).
//!
//! The operator, 2026-11-12: *"planning the macrocycle does plan the entire
//! term, but the transitions from mesocycle to mesocycle are still gated, what
//! changes is that the next mesocycle goes from being `PLANNED` to `IN_PROGRESS`
//! when the current one goes from being `IN_PROGRESS` to `COMPLETED`."*
//!
//! **Written once it starts, derived while it is planned.** A mesocycle is
//! written when it goes from planned to in progress, because things are then
//! recorded against it and it must hold still. A planned one is meant to move,
//! so it is projected again after each transition and stored nowhere. The plan
//! this is built from is the written mesocycles followed by that projection.
//!
//! **The state is the record's, not the store's.** It follows from where the
//! microcycle walk stands, never from whether a mesocycle is written: a hold
//! writes its test week with its holding weeks, and that test week is planned
//! until it starts.

use jiff::civil::Date;

use crate::plan::{Mesocycle, MesocycleOrdinal, Plan, Span};

use super::Macrocycle;

/// Where a mesocycle stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MesocycleState {
    /// It has not started. Only a planned mesocycle can change.
    Planned,
    /// The current microcycle is one of its.
    InProgress,
    /// Its last microcycle is behind the current one.
    Completed,
}

impl MesocycleState {
    /// Where a mesocycle occupying `span` stands while the microcycle
    /// commencing on `commencing` is current.
    #[must_use]
    pub fn of(span: Span, commencing: Date) -> Self {
        if span.end() <= commencing {
            Self::Completed
        } else if span.start() <= commencing {
            Self::InProgress
        } else {
            Self::Planned
        }
    }
}

/// One concurrent mesocycle of the term, and where it stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlannedMesocycle<'a> {
    ordinal: MesocycleOrdinal,
    mesocycle: Mesocycle<'a>,
    state: MesocycleState,
}

impl<'a> PlannedMesocycle<'a> {
    /// Its number in the term: its place in this list, and nowhere else (#781).
    pub const fn ordinal(&self) -> MesocycleOrdinal {
        self.ordinal
    }

    /// Both disciplines' halves of it.
    pub const fn mesocycle(&self) -> Mesocycle<'a> {
        self.mesocycle
    }

    pub const fn span(&self) -> Span {
        self.mesocycle.span()
    }

    pub const fn state(&self) -> MesocycleState {
        self.state
    }
}

/// A term, every mesocycle planned for it, and the microcycle that is current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedMacrocycle {
    macrocycle: Macrocycle,
    plan: Plan,
    commencing: Date,
}

impl PlannedMacrocycle {
    /// `plan` is the written mesocycles followed by those projected after
    /// them; `commencing` is the Monday the current microcycle commences.
    #[must_use]
    pub const fn new(macrocycle: Macrocycle, plan: Plan, commencing: Date) -> Self {
        Self {
            macrocycle,
            plan,
            commencing,
        }
    }

    pub const fn macrocycle(&self) -> &Macrocycle {
        &self.macrocycle
    }

    /// Both disciplines' mesocycles, as they would be stored if every one of
    /// them were written.
    pub const fn plan(&self) -> &Plan {
        &self.plan
    }

    /// The Monday the current microcycle commences.
    pub const fn commencing(&self) -> Date {
        self.commencing
    }

    /// Every mesocycle of the term, oldest first.
    #[must_use]
    pub fn mesocycles(&self) -> Vec<PlannedMesocycle<'_>> {
        self.plan
            .mesocycle_spans()
            .into_iter()
            .filter_map(|span| self.plan.mesocycle_on(span.start()))
            .zip(1..)
            .filter_map(|(mesocycle, number)| {
                Some(PlannedMesocycle {
                    ordinal: MesocycleOrdinal::new(number).ok()?,
                    mesocycle,
                    state: MesocycleState::of(mesocycle.span(), self.commencing),
                })
            })
            .collect()
    }

    /// Where the mesocycle covering `date` stands; `None` where none does.
    #[must_use]
    pub fn state_on(&self, date: Date) -> Option<MesocycleState> {
        self.plan
            .mesocycle_on(date)
            .map(|mesocycle| MesocycleState::of(mesocycle.span(), self.commencing))
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::MesocycleState;
    use crate::plan::Span;

    /// **Gated**: the block from 26 October is in progress through its last
    /// week, the next is planned until the week after it commences, and the
    /// one before was completed when it ended.
    #[test]
    fn a_mesocycle_is_planned_until_its_first_microcycle_is_current() {
        let earlier = Span::new(date(2026, 9, 28), 4);
        let current = Span::new(date(2026, 10, 26), 4);
        let next = Span::new(date(2026, 11, 23), 4);
        let states =
            |commencing| [earlier, current, next].map(|span| MesocycleState::of(span, commencing));
        assert_eq!(
            states(date(2026, 11, 9)),
            [
                MesocycleState::Completed,
                MesocycleState::InProgress,
                MesocycleState::Planned
            ]
        );
        assert_eq!(
            states(date(2026, 11, 16)),
            [
                MesocycleState::Completed,
                MesocycleState::InProgress,
                MesocycleState::Planned
            ]
        );
        assert_eq!(
            states(date(2026, 11, 23)),
            [
                MesocycleState::Completed,
                MesocycleState::Completed,
                MesocycleState::InProgress
            ]
        );
    }
}
