//! What a test is taken at: three attempts a step apart, the last the target.
//!
//! **The target is a guess, so nothing is prescribed past it.** The operator,
//! 2026-09-15 (#136): *"we don't actually have any way of knowing, in advance,
//! what I'm going to be able to lift, so, therefore, the whole idea of
//! prescribing it is on shaky ground. I think it's reasonable to pick some
//! number to attempt to hit and then build the warm up and earlier attempts
//! around that. If I end up getting more, great, but I don't think we
//! specifically need to prescribe those."* The record takes the heaviest
//! completed set whatever was prescribed (#127), so a lift past the target
//! still counts.
//!
//! **The attempts below it are what a miss leaves behind.** With one attempt, a
//! missed target left the ramp's last single as the heaviest weight lifted.
//! There are three, each one plate increment apart on the tested lift's own
//! scale, so a 95kg target on a 2.5kg bar is attempted at 90, 92.5 and 95.
//!
//! **The ramp leads into the first attempt, not the target**: *"that first
//! attempt should be the anchor for the warm up sets."*

use crate::gym::Kg;

use super::steps::LoadSteps;

/// The loads a test is attempted at, lightest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempts([Kg; 3]);

impl Attempts {
    /// Work down from the target, one step of `steps` at a time.
    #[must_use]
    pub fn toward(target: Kg, steps: &LoadSteps) -> Self {
        let second = steps.next_below(target);
        Self([steps.next_below(second), second, target])
    }

    /// What the ramp is built off.
    pub const fn first(self) -> Kg {
        let [first, ..] = self.0;
        first
    }

    /// What the test is an attempt at, and the last load prescribed.
    pub const fn target(self) -> Kg {
        let [.., target] = self.0;
        target
    }

    /// Every attempt, lightest first.
    pub const fn loads(self) -> [Kg; 3] {
        self.0
    }
}
