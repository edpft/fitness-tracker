//! The date and the part of the day a page is drawn on, as what it reads them
//! through (#805).
//!
//! **A page reading the date reads it through [`Today`]**, so the date is in
//! what it declares, and a drawing kept yesterday is not served today.

use application::graph::{Input, Node, Reads};
use domain::{normalised::OperatorZone, schedule::DayPart};
use jiff::civil::Date;

/// Today, in the operator's zone.
#[derive(Debug, Clone)]
pub struct Today(OperatorZone);

impl Today {
    #[must_use]
    pub const fn new(zone: OperatorZone) -> Self {
        Self(zone)
    }

    /// The date it is now.
    #[must_use]
    pub fn date(&self) -> Date {
        jiff::Timestamp::now()
            .to_zoned(self.0.as_time_zone())
            .date()
    }
}

impl Input for Today {
    fn reads() -> Reads {
        Reads::of([Node::Date])
    }
}

/// The part of the day it is now, in the operator's zone.
#[derive(Debug, Clone)]
pub struct Now(OperatorZone);

impl Now {
    #[must_use]
    pub const fn new(zone: OperatorZone) -> Self {
        Self(zone)
    }

    /// The part of the day it is now.
    #[must_use]
    pub fn day_part(&self) -> DayPart {
        DayPart::containing(
            jiff::Timestamp::now()
                .to_zoned(self.0.as_time_zone())
                .datetime(),
        )
    }
}

impl Input for Now {
    fn reads() -> Reads {
        Reads::of([Node::DayPart])
    }
}
