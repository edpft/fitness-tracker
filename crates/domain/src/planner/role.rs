//! What a session is, relative to the others in its microcycle.
//!
//! **Two axes, not one vocabulary.** A role pairs an intensity with a volume,
//! and the pair is the role: *higher intensity, lower volume* is the heavy gym
//! session and the shorter ride; *lower intensity, higher volume* is the light
//! gym session and the longer ride. The operator, 2026-09-19: length is not
//! independent of intensity, so neither axis says enough on its own.
//!
//! **The volume comparison admits equality**, and the intensity one does not.
//! The operator, 2026-09-20, settling how a holding microcycle of two
//! 45-minute rides can be roled at all: the higher-intensity session's volume
//! is *no more than* the other's, and the lower-intensity session's *no less*.
//! Two sessions of equal volume therefore carry different roles, told apart by
//! intensity alone; two of equal intensity never do.
//!
//! **So a role cannot be derived from volume.** A week of a 45-minute Power
//! Zone ride and a 45-minute Power Zone Endurance ride satisfies the rule
//! whichever way round the roles are put, and only the *kind* of class says
//! which is which. Where a mesocycle is assembled rather than provided (#180)
//! the role is stated at authoring; where it is read off a published
//! programme's durations, a tie leaves both orderings legitimate and the
//! reader picks one.
//!
//! **The gym said `light` and `heavy` until 2026-09-20 and cycling said
//! nothing at all.** One vocabulary of two words could not carry two axes, and
//! it could not carry cycling either: "a heavy ride" reads as a long one, which
//! is the opposite of what the gym's `heavy` means. A `cycling_weekday` row
//! held a bare session ordinal, and that the Sunday takes the longer ride
//! survived only as a test assertion.
//!
//! **Every term here is a comparison, never a measurement.** Not "a
//! high-intensity session" — "the higher-intensity one". There is no scale, no
//! threshold and nothing to calibrate: the operator, 2026-09-20, *"it's
//! relative to the other sessions, and the comparison is after the number of
//! sessions have been decided"*. A published Peloton microcycle offers three
//! rides and the operator takes two; the two are roled against each other, and
//! what the third would have been does not enter into it.
//!
//! **In the gym the comparison is over the primary lift, not the session.**
//! The operator, 2026-09-20. Two gym sessions can carry the same accessory
//! work at the same loads and still be a higher- and a lower-intensity
//! session, because what separates them is what the primary is done at and how
//! much of it there is. A role read off a session's total tonnage would
//! therefore be reading the wrong thing.
//!
//! **Four roles are representable and two are asked for today.** The planner
//! asks each discipline for one higher-intensity, lower-volume session and one
//! lower-intensity, higher-volume one. Nothing here fixes it there — a week
//! wanting two higher-volume sessions, or three sessions of a discipline, is a
//! question for the planner rather than a shape this type refuses.

/// One side of a comparison between the sessions of a microcycle.
///
/// Two values because two is what a comparison between the members of a set
/// needs. A third would be a scale, and a scale is the thing this deliberately
/// is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Relative {
    Lower,
    Higher,
}

impl Relative {
    pub const ALL: &'static [Self] = &[Self::Lower, Self::Higher];

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lower => "lower",
            Self::Higher => "higher",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is neither higher nor lower")]
pub struct UnknownRelative {
    value: String,
}

impl TryFrom<String> for Relative {
    type Error = UnknownRelative;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|side| side.as_str() == value)
            .copied()
            .ok_or(UnknownRelative { value })
    }
}

impl TryFrom<&str> for Relative {
    type Error = UnknownRelative;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_from(value.to_owned())
    }
}

impl std::str::FromStr for Relative {
    type Err = UnknownRelative;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl std::fmt::Display for Relative {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What one session is, against the others of its discipline in the microcycle.
///
/// In the gym both axes are read off the primary lift alone; on the bike they
/// are the ride's.
///
/// **Ordered intensity-first**, so a set of them reads lower-intensity to
/// higher. That is an ordering for display and for a `BTreeMap` key, not a
/// ranking: neither role is the better one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionRole {
    intensity: Relative,
    volume: Relative,
}

impl SessionRole {
    pub const fn new(intensity: Relative, volume: Relative) -> Self {
        Self { intensity, volume }
    }

    /// Every role, intensity-first. Four, of which the planner asks for two.
    pub const ALL: &'static [Self] = &[
        Self::new(Relative::Lower, Relative::Lower),
        Self::new(Relative::Lower, Relative::Higher),
        Self::new(Relative::Higher, Relative::Lower),
        Self::new(Relative::Higher, Relative::Higher),
    ];

    pub const fn intensity(self) -> Relative {
        self.intensity
    }

    /// How much of it there is: sets and repetitions of the primary lift in the
    /// gym, minutes on the bike.
    ///
    /// **One axis under one name.** Cycling expresses volume as duration and
    /// the gym as work done, and carrying two names for one axis would make a
    /// planner joining the two disciplines translate between them for no gain.
    ///
    /// **`Lower` means no more, and `Higher` no less** (the operator,
    /// 2026-09-20). Unlike [`Self::intensity`], this side of the comparison is
    /// not strict: two sessions of equal volume are a legitimate week, and the
    /// intensity is what separates them.
    pub const fn volume(self) -> Relative {
        self.volume
    }
}

impl std::fmt::Display for SessionRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} intensity, {} volume", self.intensity, self.volume)
    }
}
