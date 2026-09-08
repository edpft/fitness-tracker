//! The cycling session that was performed: what was done in one go.
//!
//! **The session is the unit, and Peloton's record boundary is not it.** The
//! operator, 2026-09-08:
//!
//! > the domain model is the session, which could contain more than one record
//! > from a provider or contain data from more than one provider endpoint
//!
//! > when we prescribe a cycling session, we prescribe a main ride and a cool
//! > down ride because, from our perspective, they're the same thing. This is
//! > most evident with the FTP warm up and FTP test, we would never consider
//! > these to be two separate things that could be planned separately but
//! > Peloton does split them.
//!
//! So splitting a session into rides is Peloton's architecture showing through,
//! exactly as splitting a ride's summary from its sample streams is — and
//! constitution § 3.1 says neither belongs in the shape of our entity.
//!
//! **This is the performed side.** [`super::session::CyclingSession`] is what
//! was prescribed, and § 11 keeps the two apart: prescribed data never
//! satisfies a question about what happened.

use std::fmt;

use crate::landing::SourceRecordId;
use crate::measure::{Duration, Metres};
use crate::normalised::{NormalisedEntity, StartedAt};

use super::ride::BikePlusRide;

/// A session performed on a Peloton Bike+.
///
/// **Two variants, and the second is not the first with a field added.** The
/// operator, asked whether an FTP test was a kind of session or its own thing:
/// *"It's a variant and it has to be because a Test session must include a warm
/// up, a main ride, and maybe a cool down"*. So each variant states the parts it
/// requires, and a test with no warm-up is unrepresentable rather than rejected
/// (§ 24). Modelling the warm-up as an `Option` on one shared struct would have
/// made a test a session with something missing, which is the shape this
/// codebase gets wrong most often.
///
/// **Where the warm-up went in the ordinary case.** A Power Zone class contains
/// its own warm-up, inside the one ride — which is why the prescribed
/// [`super::session::CyclingSession`] carries a warm-up *duration* rather than a
/// ride. The FTP test is the case where the warm-up is a class of its own, and
/// so the case where it is a ride here.
// 576 bytes against 864: a test holds one more ride than a ride does, and the
// rides themselves are where the memory is — a session's samples are a `Vec`
// behind each of them. Boxing the larger variant would put an allocation
// between a session and its parts to save 288 bytes on 6 of the operator's 150
// sessions, and would make the two variants read differently for no reason a
// reader of the model would recognise.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerformedSession {
    /// A ride, and the cool-down it was ridden down from.
    ///
    /// 143 of the operator's 151 sessions, 114 of them with a cool-down.
    Ride {
        main: BikePlusRide,
        cool_down: Option<BikePlusRide>,
    },
    /// A measurement of FTP: a warm-up, the effort, and usually a cool-down.
    ///
    /// Six of the operator's sessions, spanning 2023-12 to 2026-07. The warm-up
    /// is required because a test without one measures something else — the
    /// twenty-minute protocol assumes a rider who is warm, and its result is
    /// multiplied by 0.95 and used as the anchor for a quarter's prescription.
    Test {
        warm_up: BikePlusRide,
        effort: BikePlusRide,
        cool_down: Option<BikePlusRide>,
    },
}

impl PerformedSession {
    /// Every ride, in the order it was ridden.
    ///
    /// The order is the session's, not a sort: a warm-up precedes its effort
    /// and a cool-down follows what it cools down from, and that is what the
    /// variants encode.
    pub fn rides(&self) -> Vec<&BikePlusRide> {
        match self {
            Self::Ride { main, cool_down } => {
                let mut rides = vec![main];
                rides.extend(cool_down.as_ref());
                rides
            }
            Self::Test {
                warm_up,
                effort,
                cool_down,
            } => {
                let mut rides = vec![warm_up, effort];
                rides.extend(cool_down.as_ref());
                rides
            }
        }
    }

    /// The ride the session is *about* — the one whose numbers mean something
    /// about training rather than about getting ready or winding down.
    pub const fn working_ride(&self) -> &BikePlusRide {
        match self {
            Self::Ride { main, .. } => main,
            Self::Test { effort, .. } => effort,
        }
    }

    /// When the session started, which is when its first ride did.
    pub const fn started_at(&self) -> &StartedAt {
        match self {
            Self::Ride { main, .. } => main.started_at(),
            Self::Test { warm_up, .. } => warm_up.started_at(),
        }
    }

    /// How long the session lasted, summed over its rides.
    ///
    /// **The rides, not the wall clock.** The gaps between them — a minute or
    /// two of choosing the next class — are not training and are not recorded
    /// by anything. Summing what the source stated beats subtracting two
    /// instants and quietly including them.
    pub fn duration(&self) -> Duration {
        Duration::from_seconds(
            self.rides()
                .iter()
                .map(|ride| ride.duration().as_seconds())
                .sum(),
        )
    }

    /// How far the session went, summed over its rides.
    pub fn distance(&self) -> Metres {
        Metres::from_millimetres(
            self.rides()
                .iter()
                .map(|ride| ride.distance().as_millimetres())
                .sum(),
        )
    }

    /// The stable key for which variant this is. Persisted.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ride { .. } => "ride",
            Self::Test { .. } => "test",
        }
    }
}

impl NormalisedEntity for PerformedSession {
    fn composes(&self) -> Vec<&SourceRecordId> {
        self.rides()
            .into_iter()
            .map(BikePlusRide::source_record_id)
            .collect()
    }
}

impl fmt::Display for PerformedSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} {}, {} rides, {}",
            self.started_at(),
            self.kind(),
            self.duration(),
            self.rides().len(),
            self.distance()
        )
    }
}
