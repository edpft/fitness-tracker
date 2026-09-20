//! What was written to a destination for one cycling session.

use jiff::Timestamp;
use jiff::civil::Date;

use crate::{
    cycling::{RideVenue, SessionPosition},
    prescription::DestinationName,
    provider::ProgrammeName,
    sequence::NonEmpty,
};

/// A cycling session put where it can be ridden, and what went.
///
/// **The only record a delivered ride leaves, and the first.** A gym
/// prescription is derived and stored before anything is sent (§ 12), so
/// "prescribed" has a row to read. A cycling one is authored in full when the
/// plan is — there is nothing to derive and nothing to lose by forgetting a
/// draft (§ 12.1) — so until 2026-09-20 nothing was written down at any point,
/// and *prescribed* could not be told from *to be prescribed* for a ride. The
/// operator: *"we need some way of recording what we've written to the stack
/// when we write it so that we have something to compare to what was
/// performed."*
///
/// **The classes are what actually went, not what the programme holds.** The
/// cool-down is chosen from the last class's instructor at the moment of
/// delivery and appears in no authored record, so rebuilding this list from
/// the programme afterwards would describe a session nobody was sent.
///
/// **The authored ride is named by value.** A plan may be replaced (#87) and a
/// mesocycle deleted with it; what was sent to a destination happened, and a
/// key into the programme would take the record of it away with the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredRide {
    /// The slot this was the session for.
    pub prescribed_for: Date,
    pub destination: DestinationName,
    /// The published programme the ride was taken from.
    pub programme: ProgrammeName,
    /// Which microcycle of the mesocycle, counting from one.
    pub microcycle: u32,
    /// Where the ride sat in its week, in the mesocycle's own numbering.
    pub session: SessionPosition,
    /// Every class written, in the order they are ridden.
    pub classes: NonEmpty<RideVenue>,
    pub delivered_at: Timestamp,
}

impl DeliveredRide {
    /// Whether one of the classes written was this one.
    ///
    /// What a comparison against the record turns on: a performed ride names
    /// the class it was, and this says whether that class is one we sent.
    #[must_use]
    pub fn wrote(&self, reference: &str) -> bool {
        self.classes
            .iter()
            .any(|venue| venue.reference() == reference)
    }
}
