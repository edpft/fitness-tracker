//! The programme: the Teo template and the operator's preferences (#192).
//!
//! **Set when the programme is designed, never asked when a plan is made**
//! (§ 14.1, amended 2026-10-26). It is kept as dated versions, and a plan is
//! generated from the version in force when it was planned, so a later version
//! reaches only the plans made after it.
//!
//! **What it holds is everything a session prescribes that is not the primary
//! lift**: the sets and repetitions each section's slots run under double
//! progression, the slots that run their own, how long each section rests, and
//! how long a static hold is held. The primary lift's sets, repetitions and
//! loads are its progression's — [`LinearProgression`] or
//! [`SbsProgression`] — and are not here.
//!
//! [`LinearProgression`]: super::linear::LinearProgression
//! [`SbsProgression`]: super::sbs::SbsProgression

use std::collections::BTreeMap;

use crate::{
    measure::{Duration, RepCount},
    prescription::{Section, SlotId, Target, rest::RestScheme},
};

/// The double-progression scheme a section's slots run.
///
/// Work the range, and when the top of it is reached at every working set, add an
/// increment and start again at the bottom.
///
/// **One scheme per section, unless a slot has its own.** The slots within a
/// section are prescribed alike — every non-primary strength slot shares a
/// scheme, and so does every hypertrophy slot — except where a slot is given a
/// scheme of its own, which the wrist pair has been since 2026-10-22.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessoryScheme {
    /// The rep target, as a target. **Not two loose bounds**: a `low` and a
    /// `high` beside each other can be written down inverted, and the check that
    /// would catch it belongs in the type rather than at every call site
    /// (§ 24). See [`Target`].
    pub reps: Target<RepCount>,
    pub sets: RepCount,
}

/// One version of the programme.
///
/// **Held together rather than reached for one at a time** because it is one
/// statement about how the operator trains, and a derivation that took the
/// strength scheme from one version and the rest from another would be
/// describing two programmes at once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Programme {
    /// Every non-primary strength slot.
    pub strength: AccessoryScheme,
    /// Every hypertrophy slot.
    pub hypertrophy: AccessoryScheme,
    /// The slots that run a scheme of their own rather than their section's.
    pub slots: BTreeMap<SlotId, AccessoryScheme>,
    /// How long to rest, section by section.
    pub rest: RestScheme,
    /// How long a static hold is held for.
    ///
    /// The mobility work does not progress — it is held, and the same length
    /// every time — so its prescription comes from here rather than from
    /// history. One duration for every static slot.
    pub static_hold: Duration,
}

impl Programme {
    /// The scheme a non-primary slot runs: its own where it has one, and
    /// otherwise its section's.
    #[must_use]
    pub fn scheme_for(&self, slot: SlotId) -> &AccessoryScheme {
        self.slots
            .get(&slot)
            .unwrap_or_else(|| match slot.section() {
                Section::Hypertrophy => &self.hypertrophy,
                _ => &self.strength,
            })
    }
}
