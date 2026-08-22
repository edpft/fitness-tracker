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
//! **The anchor and the parameters are recorded by value.** That is what makes
//! § 14 correct: only the current parameter value is required precisely because
//! what it generated is captured here. An operator reading a prescription issued
//! six months earlier sees every number it was derived from without consulting
//! anything else.

use std::fmt;

use jiff::{Timestamp, civil::Date};

use crate::gym::Kg;

use super::{
    anchor::Anchor,
    parameters::GenerationParameters,
    schedule::{SessionRole, WeekKind},
    shape::WorkoutShape,
};

/// Which authored programme issued a prescription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgrammeId(i64);

impl ProgrammeId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for ProgrammeId {
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
    /// The starting 1RM the programme is anchored on.
    Anchor(Anchor),
    /// What the test was an attempt at. Recorded because it is a function of
    /// where the record stood when the session was issued, so nothing can
    /// recompute afterwards what it was at the time.
    Target(Kg),
}

impl DerivedFrom {
    /// The anchor, where the session had one.
    #[must_use]
    pub const fn anchor(self) -> Option<Anchor> {
        match self {
            Self::Anchor(anchor) => Some(anchor),
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
/// that knows its date, its ladder position, what it derived its loads from and
/// the parameters in force when it did. There is no setter and no partial form.
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
    parameters: GenerationParameters,
    /// Which authored version those parameters came from.
    ///
    /// The values are recorded here too, which is what § 14 rests on — but the
    /// version is the join back to the authored set, and without it two
    /// prescriptions generated under different parameters are indistinguishable
    /// except by comparing every field.
    parameters_authored_at: Timestamp,
    programme: ProgrammeId,
    issued_at: Timestamp,
}

impl PrescribedWorkout {
    #[expect(
        clippy::too_many_arguments,
        reason = "every one of these is what makes the prescription an issued \
                  fact rather than a shape; grouping them into a struct would \
                  just move the same list one level down, and making any of \
                  them optional is the § 11 hazard this type exists to close"
    )]
    pub const fn new(
        shape: WorkoutShape,
        issued_for: Date,
        session_role: SessionRole,
        week: WeekKind,
        derived_from: DerivedFrom,
        parameters: GenerationParameters,
        parameters_authored_at: Timestamp,
        programme: ProgrammeId,
        issued_at: Timestamp,
    ) -> Self {
        Self {
            shape,
            issued_for,
            session_role,
            week,
            derived_from,
            parameters,
            parameters_authored_at,
            programme,
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

    /// The anchor, where this session had one.
    pub const fn anchor(&self) -> Option<Anchor> {
        self.derived_from.anchor()
    }

    pub const fn parameters(&self) -> &GenerationParameters {
        &self.parameters
    }

    pub const fn parameters_authored_at(&self) -> Timestamp {
        self.parameters_authored_at
    }

    pub const fn programme(&self) -> ProgrammeId {
        self.programme
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
