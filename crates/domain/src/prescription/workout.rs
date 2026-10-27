//! A shape that was issued, and everything that makes that claim true.
//!
//! **Only generation can build one of these**, because only generation holds an
//! anchor, a week and a programme. That is not a convenience: it is what makes a
//! projected shape unstorable as a prescription. A performance projected into
//! prescription's vocabulary yields a [`WorkoutShape`] and stops there, so the
//! record can never come to hold a prescription reverse-engineered from the
//! performance it exists to be compared against — which would make expectation
//! against reality unrecoverable, and is what § 11 protects.
//!
//! **The anchor and every set are recorded by value.** An operator reading a
//! prescription issued six months earlier sees every number it was derived from
//! without consulting anything else. Which versions of the programme and its
//! progression produced them is not recorded here: it is the plan's, derived
//! from when the plan was made (#192).

use std::fmt;

use jiff::{Timestamp, civil::Date};

use crate::{measure::Kg, schedule::SessionRole};

use super::{anchor::Anchor, schedule::WeekKind, shape::WorkoutShape};

/// Which mesocycle issued a prescription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MesocycleId(i64);

impl MesocycleId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for MesocycleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What a session's primary loads were derived from.
///
/// **A sum rather than two nullable fields.** A session issued from a programme
/// that climbs derives every primary load from a fixed anchor; a session issued
/// from a standalone test derives them from the target the record put it at
/// (decision 0011), and that programme has no anchor at all. Both are recorded
/// by value for the same reason — what was issued has to stay readable as
/// issued — and exactly one of them is ever the answer, which two `Option`s
/// would let a caller get wrong in both directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedFrom {
    /// The anchor this session's loads were shares of, resolved as it stood on
    /// the day the session was issued.
    Anchor(Anchor),
    /// What the test was an attempt at. Recorded because it is a function of
    /// where the record stood when the session was issued, so nothing can
    /// recompute afterwards what it was at the time.
    Target(Kg),
}

impl DerivedFrom {
    /// The anchor, where the session derived from one.
    #[must_use]
    pub const fn anchor(self) -> Option<Anchor> {
        match self {
            Self::Anchor(maximum) => Some(maximum),
            Self::Target(_) => None,
        }
    }

    /// The target, where the session was a standalone test.
    #[must_use]
    pub const fn target(self) -> Option<Kg> {
        match self {
            Self::Target(load) => Some(load),
            Self::Anchor(_) => None,
        }
    }
}

/// A prescription that was issued.
///
/// Every field is a constructor argument, so a prescription that exists is one
/// that knows its date, its ladder position and what it derived its loads from.
/// There is no setter and no partial form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescribedWorkout {
    shape: WorkoutShape,
    /// A date, not an instant — and also the key a later correspondence feature
    /// joins on. Designing without it would make correspondence a migration
    /// rather than a query.
    issued_for: Date,
    session_role: SessionRole,
    week: WeekKind,
    derived_from: DerivedFrom,
    mesocycle: MesocycleId,
    issued_at: Timestamp,
}

impl PrescribedWorkout {
    pub const fn new(
        shape: WorkoutShape,
        issued_for: Date,
        session_role: SessionRole,
        week: WeekKind,
        derived_from: DerivedFrom,
        mesocycle: MesocycleId,
        issued_at: Timestamp,
    ) -> Self {
        Self {
            shape,
            issued_for,
            session_role,
            week,
            derived_from,
            mesocycle,
            issued_at,
        }
    }

    pub const fn shape(&self) -> &WorkoutShape {
        &self.shape
    }

    pub const fn issued_for(&self) -> Date {
        self.issued_for
    }

    pub const fn session_role(&self) -> SessionRole {
        self.session_role
    }

    pub const fn week(&self) -> WeekKind {
        self.week
    }

    /// What the primary loads were derived from.
    pub const fn derived_from(&self) -> DerivedFrom {
        self.derived_from
    }

    /// The anchor, where this session derived from one.
    pub const fn anchor(&self) -> Option<Anchor> {
        self.derived_from.anchor()
    }

    pub const fn mesocycle(&self) -> MesocycleId {
        self.mesocycle
    }

    pub const fn issued_at(&self) -> Timestamp {
        self.issued_at
    }
}

impl fmt::Display for PrescribedWorkout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, {}) — {}",
            self.issued_for, self.session_role, self.week, self.shape
        )
    }
}
