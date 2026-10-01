//! When a canonical entity happened, at the precision its sources knew it to.

use std::fmt;

use jiff::civil::Date;

use crate::normalised::StartedAt;

/// When something happened: at an instant, or on a day.
///
/// **Two variants because the sources have two precisions, not because one is
/// a degenerate form of the other.** Hevy and a watch state when a session
/// started; no spreadsheet does, and `CT 2017` states only the week the
/// programme put the workout in (operator, 2026-09-27). A day carried as
/// midnight would be a clock reading nothing observed, and the arithmetic over
/// it would be wrong every time a session was compared against one that has a
/// real instant.
///
/// The instant wins where a canonical entity has both, because it is the finer
/// of two accounts of one fact rather than a competing account of it: a sheet
/// saying 14 March and a watch saying 07:43 on 14 March do not disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Occurred {
    At(StartedAt),
    On(Date),
}

impl Occurred {
    /// The day it happened on, resolved through the zone where there is an
    /// instant to resolve.
    ///
    /// Calendar bucketing goes through this rather than through a UTC date:
    /// a session at 00:30 in Europe/London in summer is the 14th here and the
    /// 13th in UTC (§ II.3).
    pub fn day(&self) -> Date {
        match self {
            Self::At(started_at) => started_at.wall_clock().date(),
            Self::On(day) => *day,
        }
    }

    /// The instant, where a source stated one.
    pub const fn instant(&self) -> Option<&StartedAt> {
        match self {
            Self::At(started_at) => Some(started_at),
            Self::On(_) => None,
        }
    }
}

impl fmt::Display for Occurred {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::At(started_at) => write!(f, "{started_at}"),
            Self::On(day) => write!(f, "{day}"),
        }
    }
}
