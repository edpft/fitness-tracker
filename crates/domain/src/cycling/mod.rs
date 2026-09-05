//! Cycling: the second discipline, and the first that is not the gym.
//!
//! **A session is duration × power zone** (decision 0025). Nothing in this
//! module knows that Peloton exists — the vocabulary is ours, and a source's
//! identifiers live with that source's adapter (§ II.3). What a Peloton class
//! *is*, to this crate, is a thing at a destination that realises one of these
//! sessions.
//!
//! The reasoning is in `docs/decisions/0025-a-cycling-session-is-duration-times-power-zone.md`.
//!
//! **The transcribed seed is gone.** `docs/cycling-peak-your-power-zones.md`
//! was read off screenshots and disagreed with the API by about a minute in all
//! twenty-three of its zoned sessions — it opened every ride at Z3 where the
//! class opens at Z1. What a class contains is read from the source and
//! authored into the store; the document stays as the record of how the method
//! was worked out, and nothing reads it.
//!
//! **Where this meets the gym.** Both disciplines anchor on a measured maximum
//! and both open and close on a test of it — FTP for the zones here, a one-rep
//! maximum for the percentages there. The types said so before anyone planned
//! it: [`Ride::Effort`](session::Ride::Effort) carries a duration and no zone
//! for the same reason `WeekPlan::WorkUp` carries repetitions and no load.

pub mod programme;
pub mod session;
pub mod shape;
pub mod zone;

pub use programme::{
    CyclingMicrocycle, CyclingProgramme, CyclingProgrammeId, CyclingWeekdays,
    InvalidCyclingProgramme, InvalidMicrocycle, InvalidSessionPosition, InvalidVenue,
    InvalidWeekdays, PlannedRide, PublishedMicrocycle, RideVenue, SessionPosition,
};
pub use session::{CyclingSession, Interval, Ride, clock};
pub use shape::{
    Answer, Programme, Refused, ZoneProfile, bottom_level, diverges, is_mesocycle, mesocycles,
    partition, span, zones_lost,
};
pub use zone::{
    Ftp, FtpProvenance, InvalidFtp, PowerZone, UnknownZone, WattRange, Watts, ZoneBand,
};
