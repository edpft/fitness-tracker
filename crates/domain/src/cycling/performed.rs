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
use super::zone::{Ftp, FtpProvenance, InvalidFtp, Watts};

/// What the twenty-minute protocol multiplies a test's average power by.
///
/// **A published protocol's constant, not a parameter.** The twenty-minute test
/// is ridden as hard as can be held for twenty minutes, which is by definition
/// longer than an hour's power; 95% is the correction the protocol states, and
/// it is what Peloton's own reported FTP agrees with. § 14.1 is the test — it
/// would not be true of anything if no test were being interpreted — so it is
/// one fixed decision rather than something to configure.
///
/// Changing it does not restate old values more accurately: § 6 makes a
/// derivation choice part of a series' method, so a different multiplier is a
/// different series and never a correction to this one.
const TWENTY_MINUTE_SHARE: u32 = 95;

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

    /// The FTP this session measured, where it measured one.
    ///
    /// **Only a test measures one**, so a ride answers [`None`] — which is the
    /// variant doing the work that recognising a test from a class title or a
    /// duration would otherwise do.
    ///
    /// **The effort's stated average, never a mean of its samples.** Peloton
    /// states the average and it is not the mean of the series beside it; only
    /// the stated figure reproduces all six of the operator's known values.
    /// [`BikePlusRide::average_power`] carries the reasoning.
    ///
    /// Rounded rather than truncated, in whole watts, because that is what
    /// reproduces the record: 209 W truncates to 198 where the operator's own
    /// value, and Peloton's, is 199.
    ///
    /// The arithmetic is integral throughout. A float would make the value
    /// depend on rounding mode for a number that is persisted and compared
    /// against rows written by earlier versions (§ 7).
    ///
    /// # Errors
    ///
    /// [`InvalidFtp`] where the effort averaged no power at all, which is a
    /// ride nobody pedalled rather than a threshold.
    pub fn measured_ftp(&self) -> Option<Result<Ftp, InvalidFtp>> {
        let Self::Test { effort, .. } = self else {
            return None;
        };
        let average = effort.average_power().as_u32();
        // In `u64` so the multiplication cannot overflow, and back in `u32`
        // because a share of a value is smaller than it — the conversion cannot
        // fail, and the average is the only sensible thing to say if it did.
        let hundredths = u64::from(average) * u64::from(TWENTY_MINUTE_SHARE) + 50;
        let watts = u32::try_from(hundredths / 100).unwrap_or(average);
        Some(Ftp::new(
            Watts::from_u32(watts),
            self.started_at().wall_clock().date(),
            // Arithmetic over a measurement, not a measurement: the twenty
            // minutes were ridden, the hour they stand for was not.
            FtpProvenance::Estimated,
        ))
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
