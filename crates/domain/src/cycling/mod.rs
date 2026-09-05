//! Cycling: the second discipline, and the first that is not the gym.
//!
//! **A session is duration × power zone** (decision 0025). Nothing in this
//! module knows that Peloton exists — the vocabulary is ours, and a source's
//! identifiers live with that source's adapter (§ II.3). What a Peloton class
//! *is*, to this crate, is a thing at a destination that realises one of these
//! sessions.
//!
//! The reasoning is in `docs/decisions/0025-a-cycling-session-is-duration-times-power-zone.md`,
//! and the transcribed programme it rests on is
//! `docs/cycling-peak-your-power-zones.md`.
//!
//! **Where this meets the gym.** Both disciplines anchor on a measured maximum
//! and both open and close on a test of it — FTP for the zones here, a one-rep
//! maximum for the percentages there. The types said so before anyone planned
//! it: [`Ride::Effort`](session::Ride::Effort) carries a duration and no zone
//! for the same reason `WeekPlan::WorkUp` carries repetitions and no load.

pub mod programme;
pub mod seed;
pub mod session;
pub mod shape;
pub mod zone;

pub use programme::{
    CycleDay, CyclingProgramme, CyclingProgrammeName, EmptySelection, InvalidCycleDay,
    InvalidProgrammeName, ProgrammeWeek, Selection,
};
pub use seed::{InvalidCyclingSeed, peak_your_power_zones};
pub use session::{CyclingSession, Interval, Ride, clock};
pub use shape::{ZoneProfile, diverges, is_three_to_one};
pub use zone::{
    Ftp, FtpProvenance, InvalidFtp, PowerZone, UnknownZone, WattRange, Watts, ZoneBand,
};
