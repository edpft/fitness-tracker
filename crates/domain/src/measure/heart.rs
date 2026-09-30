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

/// How much the interval between beats varied, in milliseconds.
///
/// **Here beside [`BeatsPerMinute`], and non-zero for its reasons.** A heart
/// whose beats are perfectly evenly spaced does not exist, so a zero is a sensor
/// saying nothing rather than a reading of no variability (§ 37). The operator's
/// 11,547 landed readings run from 7ms to 133ms and hold no zero.
///
/// **Milliseconds, whole**, because that is what Garmin serves: every figure in
/// 634 nights is an integer, summaries and individual readings alike. Which
/// algorithm produced it is not ours to state and § 6 makes the series
/// Garmin's — a reading from another watch is a different series, not a noisier
/// version of this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeartRateVariability(std::num::NonZeroU32);

impl HeartRateVariability {
    pub const fn as_milliseconds(self) -> u32 {
        self.0.get()
    }

    /// # Errors
    ///
    /// [`InvalidQuantity::NoVariability`] for a zero.
    pub const fn from_milliseconds(milliseconds: u32) -> Result<Self, InvalidQuantity> {
        match std::num::NonZeroU32::new(milliseconds) {
            Some(milliseconds) => Ok(Self(milliseconds)),
            None => Err(InvalidQuantity::NoVariability),
        }
    }
}

impl TryFrom<String> for HeartRateVariability {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let milliseconds: u32 = value.parse().map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "heart-rate variability",
                    value: value.clone(),
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "milliseconds",
                    value: value.clone(),
                }
            }
        })?;
        Self::from_milliseconds(milliseconds)
    }
}

impl fmt::Display for HeartRateVariability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

crate::newtype::from_str_via_string!(HeartRateVariability, InvalidQuantity);

/// What a source says a session's heart rate came to: its average and its
/// highest.
///
/// **Stated, never averaged here.** Both figures are the source's own summary
/// of a recording it holds at a resolution this does not — § II.3 forbids us
/// aggregating component observations, and says nothing against keeping what a
/// source states, which is the call [`crate::cycling::RideRecord`]'s average
/// power made for the same reason. Where the samples themselves are wanted,
/// they come from the recording, not from here.
///
/// **Both or neither**, which is what every source has served so far: Garmin
/// states `averageHR` and `maxHR` together on 547 of the operator's 548 gym
/// activities and neither on the other. A summary with one of them is not a
/// summary of anything, so there is no variant for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartRateSummary {
    average: BeatsPerMinute,
    highest: BeatsPerMinute,
}

impl HeartRateSummary {
    /// Both figures as the source stated them. Nothing compares them: a source
    /// contradicting itself is data to look at rather than a type to refuse,
    /// and no figure here is ours to correct.
    pub const fn new(average: BeatsPerMinute, highest: BeatsPerMinute) -> Self {
        Self { average, highest }
    }

    pub const fn average(self) -> BeatsPerMinute {
        self.average
    }

    pub const fn highest(self) -> BeatsPerMinute {
        self.highest
    }
}

impl fmt::Display for HeartRateSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} average, {} highest", self.average, self.highest)
    }
}

/// One reading, and where in the recording it sits.
///
/// **Here rather than in `cycling`**, for [`BeatsPerMinute`]'s reason: a watch
/// on a gym floor writes the same series a bike relays, and the thing that
/// differs between them is the source, not the measurement.
///
/// `at` is seconds from the start of the recording, and it is carried rather
/// than implied by position. Neither source is regular: Peloton's index starts
/// at 4 or skips 38 seconds in the middle on 143 of the operator's 285 rides,
/// and Garmin's watch writes on its own judgement — 2,785 readings across the
/// 5,935 seconds of 2026-09-25, spaced one to ten seconds apart. Row order
/// would silently restate a gap as continuous recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeartRateSample {
    pub at: super::Duration,
    pub beats_per_minute: BeatsPerMinute,
}

/// The heart-rate series, where one was recorded.
///
/// **A series rather than a nullable column on whatever else was measured.** A
/// heart rate comes from a strap or a watch, which is a different method from
/// the bike's power or the watch's rep counting, and § 6 makes that a different
/// series even inside one entity. As a separate series, "nothing was worn" is
/// [`None`] and "the sensor dropped out for every second" is a series with no
/// samples, which cannot be built; as a nullable column the two would be the
/// same rows of nulls.
///
/// **What a source says it missed is not here**, because it is a total rather
/// than a position: it sits on the entity beside the series, which is also
/// where the store keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartRateSeries {
    samples: crate::sequence::NonEmpty<HeartRateSample>,
}

impl HeartRateSeries {
    pub const fn new(samples: crate::sequence::NonEmpty<HeartRateSample>) -> Self {
        Self { samples }
    }

    pub const fn samples(&self) -> &crate::sequence::NonEmpty<HeartRateSample> {
        &self.samples
    }
}

impl fmt::Display for HeartRateSeries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} readings", self.samples.count())
    }
}
