//! How fast a heart beats.
//!
//! **Here rather than in `cycling`**, because a Body Scan weigh-in reads one as
//! well as a ride does.

use std::fmt;

use super::InvalidQuantity;

/// A heart rate, in beats per minute.
///
/// **Zero is unrepresentable, which is the whole point of the type.** Peloton
/// serves a 0 for every second the watch was not broadcasting — 4,528 of them
/// across the operator's record, 2,037 consecutively on one ride where the
/// watch stopped a third of the way in. A heart does not beat zero times a
/// minute while its owner is pedalling, so a 0 is the absence of a reading, and
/// a type that cannot hold one turns "drop the dropouts" from a rule the
/// translator has to remember into a shape it cannot avoid (§ 24, § 37).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatsPerMinute(std::num::NonZeroU32);

impl BeatsPerMinute {
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// # Errors
    ///
    /// [`InvalidQuantity::NotBeating`] for a zero, which is a sensor saying
    /// nothing rather than a rate.
    pub const fn new(beats: u32) -> Result<Self, InvalidQuantity> {
        match std::num::NonZeroU32::new(beats) {
            Some(beats) => Ok(Self(beats)),
            None => Err(InvalidQuantity::NotBeating),
        }
    }
}

impl TryFrom<String> for BeatsPerMinute {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let beats: u32 = value.parse().map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "a heart rate",
                    value: value.clone(),
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "beats per minute",
                    value: value.clone(),
                }
            }
        })?;
        Self::new(beats)
    }
}

impl fmt::Display for BeatsPerMinute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}bpm", self.0)
    }
}

crate::newtype::from_str_via_string!(BeatsPerMinute, InvalidQuantity);
