//! What a session did, against what it was told to do.
//!
//! **The comparison itself is `domain`'s**, and this use case supplies neither
//! half of it: [`project`] reads a performed workout as a prescription shape and
//! [`satisfies`] says how that shape and the issued one parted company. Both are
//! total functions over values, so what is left here is finding the pair — and
//! finding the pair is the whole of the problem, because a performance and a
//! prescription are two records that meet in only one place.
//!
//! **The published id is that place.** The prescription was delivered, the
//! destination named it, and the performance carries that name (§ 12.1). So the
//! pairing is a lookup rather than a guess, and it survives a session performed
//! on a different day from the one it was prescribed for — which is exactly the
//! case a comparison most needs to handle, since a session moved is a session
//! worth asking about.
//!
//! **Where nothing names it, the day is taken instead, and the answer says so.**
//! Every block trained before a prescription could be delivered is in that
//! state, and refusing to compare them would make this useless on the only
//! record that exists. But a pairing by date is an assumption and a pairing by
//! id is a fact, so [`Pairing`] distinguishes them and the caller is expected to
//! show which it got. The two are not interchangeable and nothing here pretends
//! they are.
//!
//! **Nothing is written.** A comparison reads two records and returns a
//! judgement about them; it does not record the judgement, because the judgement
//! re-derives from the two records exactly and § 12 asks us to keep what cannot
//! be regenerated rather than what can.

use domain::{
    gym::GymWorkout,
    prescription::{DeliveryReference, Divergence, ProjectionGap, project, satisfies},
};
use jiff::civil::Date;

use crate::{
    error::ComparisonError,
    ports::{PerformedWorkoutReader, PrescribedWorkoutStore},
};

/// How a performance was paired with the prescription it is compared against.
///
/// **Two states, and they carry different weight.** One is a fact the record
/// holds; the other is an assumption this use case made because the record held
/// nothing better. Collapsing them into an `Option<DeliveryReference>` would
/// leave the difference to a caller's `is_some`, which is the shape that ends
/// with a guess printed as a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pairing {
    /// The performance names the session, through the id the destination gave
    /// it on delivery. Nothing is assumed.
    Published(DeliveryReference),
    /// Nothing named the session, so the workout trained on the day it was
    /// prescribed for was taken to be it. An assumption, and a defensible one
    /// only while a day holds a single session.
    Dated,
}

/// A performance, the prescription it answers, and how far apart they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// The date the session was prescribed for.
    pub prescribed_for: Date,
    /// The day it was actually trained, which is not always the same day.
    pub performed_on: Date,
    pub pairing: Pairing,
    /// What the performed record could not supply, from [`project`]. A gap is
    /// not a divergence: it is this reading being honest about what it cannot
    /// see, and a divergence derived from a gap would be an artefact.
    pub gaps: Vec<ProjectionGap>,
    /// Every way the two parted company. Empty is the session that did what it
    /// was told.
    pub divergences: Vec<Divergence>,
}

impl Comparison {
    /// Did the session do what it was told?
    #[must_use]
    pub const fn satisfied(&self) -> bool {
        self.divergences.is_empty()
    }
}

/// Everything comparison needs from the outside.
pub struct ComparisonPorts<S, W> {
    pub prescriptions: S,
    pub workouts: W,
}

/// The use case.
pub struct Comparing<S, W> {
    ports: ComparisonPorts<S, W>,
}

impl<S, W> Comparing<S, W> {
    pub const fn new(ports: ComparisonPorts<S, W>) -> Self {
        Self { ports }
    }
}

impl<S, W> Comparing<S, W>
where
    S: PrescribedWorkoutStore + Sync,
    W: PerformedWorkoutReader + Sync,
{
    /// Compare what was performed against what was prescribed for a date.
    ///
    /// # Errors
    ///
    /// [`ComparisonError::NothingIssued`] where no session was prescribed for
    /// the date, and [`ComparisonError::NotPerformed`] where one was and no
    /// performance answers it.
    pub async fn compare(&self, date: Date) -> Result<Comparison, ComparisonError> {
        let (id, prescribed) = self
            .ports
            .prescriptions
            .issued_for(date)
            .await?
            .ok_or(ComparisonError::NothingIssued { date })?;

        // The link first, because it is the answer that does not assume
        // anything. Only when the record names no session does the day stand in.
        let (pairing, performed) = match self.ports.workouts.fulfilling(id).await? {
            Some((reference, workout)) => (Pairing::Published(reference), workout),
            None => (Pairing::Dated, self.trained_on(date).await?),
        };

        let projection = project(&performed);
        let divergences = satisfies(&projection.shape, prescribed.shape());

        Ok(Comparison {
            prescribed_for: date,
            performed_on: performed.started_at().wall_clock().date(),
            pairing,
            gaps: projection.gaps,
            divergences,
        })
    }

    /// The session trained on a day, where the record names no session.
    ///
    /// **The first of them, and a second is refused rather than picked between.**
    /// Two sessions on one day is the case where a date says nothing about which
    /// one answered the prescription, and choosing quietly is how a comparison
    /// comes to be run against the wrong workout. Publishing the session
    /// resolves it, which is the remedy the error names.
    async fn trained_on(&self, date: Date) -> Result<GymWorkout, ComparisonError> {
        let mut trained = self.ports.workouts.between(date, date).await?;
        if trained.len() > 1 {
            return Err(ComparisonError::AmbiguousDay {
                date,
                count: trained.len(),
            });
        }
        trained.pop().ok_or(ComparisonError::NotPerformed { date })
    }
}
