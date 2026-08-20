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

/// A prescription that was issued.
///
/// Every field is a constructor argument, so a prescription that exists is one
/// that knows its date, its ladder position, the anchor it derived from and the
/// parameters in force when it did. There is no setter and no partial form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrescribedWorkout {
    shape: WorkoutShape,
    /// A date, not an instant — and also the key a later correspondence feature
    /// joins on. Designing without it would make correspondence a migration
    /// rather than a query.
    issued_for: Date,
    session_role: SessionRole,
    week: WeekKind,
    anchor: Anchor,
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
        anchor: Anchor,
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
            anchor,
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

    pub const fn anchor(&self) -> Anchor {
        self.anchor
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
