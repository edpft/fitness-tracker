//! Power zones, and the number every one of them is a share of.
//!
//! **Seven zones, and they are the discipline's rather than a vendor's.** The
//! seven-band model is Coggan's and is what every power-based cycling programme
//! speaks; Peloton uses it, but Peloton did not invent it, so it belongs here
//! and not in an adapter. The test § II.3 applies is whether a vendor shaped it,
//! not whose numbers they are.
//!
//! **The bands corroborate themselves against the record.** Coggan names the
//! zones for what they train, and the operator's transcribed programme
//! (`docs/cycling-peak-your-power-zones.md`) uses each one at exactly the
//! duration its name implies:
//!
//! ```text
//! Z5  VO2max                1–3 minute efforts        the week 3 and 5 rides
//! Z6  anaerobic capacity    30–60 second efforts      the week 6 and 7 rides
//! Z7  neuromuscular power   15 second efforts only    the Max Ride, and nothing else
//! ```
//!
//! Nothing here was fitted to that. The bands are published and the durations
//! were transcribed, and they agree — which is the sort of corroboration a
//! number chosen here could never have.
//!
//! **A zone is a share of FTP, so watts are derived and never stored.** Two
//! consequences follow, and § 13 wants both: the same class prescribes different
//! watts before and after a test, and a prescription issued last month stays
//! reproducible because the FTP in force then is still recorded.

use std::{fmt, num::NonZeroU32};

/// One of the seven power zones.
///
/// A closed enum rather than a validated integer: there are exactly seven, they
/// are named, and an eighth is not a thing that can happen (§ 23, § 24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PowerZone {
    /// Active recovery.
    One,
    /// Endurance.
    Two,
    /// Tempo.
    Three,
    /// Lactate threshold.
    Four,
    /// VO2 max.
    Five,
    /// Anaerobic capacity.
    Six,
    /// Neuromuscular power.
    Seven,
}

impl PowerZone {
    pub const ALL: [Self; 7] = [
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
    ];

    /// The zone's number, as everyone writes it.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
            Self::Five => 5,
            Self::Six => 6,
            Self::Seven => 7,
        }
    }

    /// What the zone trains. Coggan's own names.
    pub const fn purpose(self) -> &'static str {
        match self {
            Self::One => "active recovery",
            Self::Two => "endurance",
            Self::Three => "tempo",
            Self::Four => "lactate threshold",
            Self::Five => "VO2 max",
            Self::Six => "anaerobic capacity",
            Self::Seven => "neuromuscular power",
        }
    }

    /// The share of FTP this zone spans.
    pub const fn band(self) -> ZoneBand {
        match self {
            Self::One => ZoneBand::upto(55),
            Self::Two => ZoneBand::between(56, 75),
            Self::Three => ZoneBand::between(76, 90),
            Self::Four => ZoneBand::between(91, 105),
            Self::Five => ZoneBand::between(106, 120),
            Self::Six => ZoneBand::between(121, 150),
            Self::Seven => ZoneBand::above(150),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a power zone — they run 1 to 7")]
pub struct UnknownZone {
    value: String,
}

impl TryFrom<u8> for PowerZone {
    type Error = UnknownZone;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|zone| zone.as_u8() == value)
            .ok_or_else(|| UnknownZone {
                value: value.to_string(),
            })
    }
}

impl TryFrom<String> for PowerZone {
    type Error = UnknownZone;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim().trim_start_matches(['z', 'Z']);
        trimmed
            .parse::<u8>()
            .ok()
            .and_then(|number| Self::try_from(number).ok())
            .ok_or(UnknownZone { value })
    }
}

impl TryFrom<&str> for PowerZone {
    type Error = UnknownZone;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl std::str::FromStr for PowerZone {
    type Err = UnknownZone;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl fmt::Display for PowerZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Z{}", self.as_u8())
    }
}

/// The share of FTP a zone spans, in whole percentage points.
///
/// **The open ends are real and are not a missing number.** Zone one has no
/// floor because soft-pedalling is still zone one, and zone seven has no ceiling
/// because a sprint is bounded by the rider rather than by the scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneBand {
    lower: Option<u16>,
    upper: Option<u16>,
}

/// Where zone one is taken to sit when a plan is scored.
///
/// **One of only two invented numbers in this file, and it was checked rather
/// than chosen.** Zone one has no floor, so it has no midpoint, and scoring a
/// zone plan needs one. Across the whole of *Boost Your Base* — twenty-four
/// classes, in which zone one is 2.6% of the riding — moving this anywhere
/// between 40 and 55 changes no microcycle's [`tss`] by a whole point, and
/// changes no ordering and no arc. 45 is the number the operator's scratchpad
/// used, so the figures he has already seen reproduce exactly.
///
/// [`tss`]: crate::cycling::ZoneProfile::tss
const ZONE_ONE_MIDPOINT_PERCENT: f64 = 45.0;

/// Where zone seven is taken to sit when a plan is scored.
///
/// The other invented number, and it matters less still. The only zone seven in
/// anything read so far is the fifteen seconds of the *Power Zone Max Ride*,
/// worth 0.9 TSS at 150 and 1.7 at 200 against a microcycle scoring 141.
const ZONE_SEVEN_MIDPOINT_PERCENT: f64 = 170.0;

impl ZoneBand {
    const fn upto(upper: u16) -> Self {
        Self {
            lower: None,
            upper: Some(upper),
        }
    }

    const fn between(lower: u16, upper: u16) -> Self {
        Self {
            lower: Some(lower),
            upper: Some(upper),
        }
    }

    const fn above(lower: u16) -> Self {
        Self {
            lower: Some(lower),
            upper: None,
        }
    }

    pub const fn lower_percent(self) -> Option<u16> {
        self.lower
    }

    pub const fn upper_percent(self) -> Option<u16> {
        self.upper
    }

    /// The share of FTP this band is scored at, as one number.
    ///
    /// **The midpoint, except where there is no midpoint to take.** Zones one
    /// and seven are open at one end by design — see this type's own note — so
    /// each answers with a stated constant instead. Both constants were checked
    /// against the programmes read so far for how far they move the answer, and
    /// neither moves it enough to change anything.
    #[must_use]
    pub fn midpoint_percent(self) -> f64 {
        match (self.lower, self.upper) {
            (Some(lower), Some(upper)) => f64::from(lower + upper) / 2.0,
            (None, Some(_)) => ZONE_ONE_MIDPOINT_PERCENT,
            (Some(_), None) => ZONE_SEVEN_MIDPOINT_PERCENT,
            // No zone is open at both ends, so this is unreachable — but § 26
            // forbids asserting that with a panic, and FTP itself is the one
            // answer that assumes nothing.
            (None, None) => 100.0,
        }
    }

    /// What this band is in watts, for a rider at `ftp`.
    ///
    /// Rounded to the nearest watt, which is finer than any bike displays.
    #[must_use]
    pub fn watts_at(self, ftp: Ftp) -> WattRange {
        let share = |percent: u16| Watts(u32::from(percent) * ftp.watts().as_u32() / 100);
        WattRange {
            lower: self.lower.map(share),
            upper: self.upper.map(share),
        }
    }
}

/// A band expressed in watts, for one rider at one FTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WattRange {
    lower: Option<Watts>,
    upper: Option<Watts>,
}

impl WattRange {
    pub const fn lower(self) -> Option<Watts> {
        self.lower
    }

    pub const fn upper(self) -> Option<Watts> {
        self.upper
    }
}

impl fmt::Display for WattRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.lower, self.upper) {
            (None, Some(upper)) => write!(f, "up to {upper}"),
            (Some(lower), None) => write!(f, "{lower}+"),
            (Some(lower), Some(upper)) => write!(f, "{lower}–{upper}"),
            (None, None) => f.write_str("any"),
        }
    }
}

/// Power, in whole watts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Watts(u32);

impl Watts {
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    pub const fn from_u32(watts: u32) -> Self {
        Self(watts)
    }
}

impl fmt::Display for Watts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}W", self.0)
    }
}

/// How an FTP was arrived at.
///
/// The same three the gym's anchor carries, for the same reason: the difference
/// matters six months later and is not recoverable from the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FtpProvenance {
    /// Measured under test.
    Tested,
    /// Arithmetic over a test — the twenty-minute protocol's 95%, say.
    Estimated,
    /// Neither. A bootstrap.
    Asserted,
}

impl FtpProvenance {
    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tested => "tested",
            Self::Estimated => "estimated",
            Self::Asserted => "asserted",
        }
    }
}

impl fmt::Display for FtpProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an FTP of zero watts is not a threshold anyone can ride at")]
pub struct InvalidFtp;

/// Functional threshold power: the number every zone is a share of.
///
/// **An interpretive parameter under § 13**, so it is effect-dated and retained.
/// The value in force when a session was prescribed is the one that applies to
/// it, and a later test supersedes without rewriting anything already issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ftp {
    watts: Watts,
    from: jiff::civil::Date,
    provenance: FtpProvenance,
}

impl Ftp {
    /// # Errors
    ///
    /// [`InvalidFtp`] for zero watts.
    pub const fn new(
        watts: Watts,
        from: jiff::civil::Date,
        provenance: FtpProvenance,
    ) -> Result<Self, InvalidFtp> {
        if NonZeroU32::new(watts.as_u32()).is_none() {
            return Err(InvalidFtp);
        }
        Ok(Self {
            watts,
            from,
            provenance,
        })
    }

    pub const fn watts(self) -> Watts {
        self.watts
    }

    /// The date this value took effect — the test that produced it, or the day
    /// it was asserted.
    pub const fn from(self) -> jiff::civil::Date {
        self.from
    }

    pub const fn provenance(self) -> FtpProvenance {
        self.provenance
    }
}

impl fmt::Display for Ftp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}, from {})",
            self.watts, self.provenance, self.from
        )
    }
}
