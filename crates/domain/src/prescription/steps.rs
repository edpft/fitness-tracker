//! The loads a piece of equipment can actually hold.
//!
//! **An increment is not a number, because a dumbbell rack is not a bar.** A
//! barbell moves in one step forever: put a 1.25kg plate on each end and every
//! load from an empty bar upward is 2.5kg apart. A rack does not — ours moves in
//! whole kilos to 10kg and in twos above it — so "the increment" has no single
//! value and a derivation that assumes one prescribes 12.5kg off a 10kg
//! dumbbell, which is a weight nobody owns.
//!
//! So the scale is banded: a step size, and the load it starts applying at. A
//! barbell is the degenerate case with one band, which is why this replaces the
//! old single increment rather than sitting beside it — two ways to say "what
//! can go on the bar" is two things that can disagree.
//!
//! **The bands are data and the rounding is code**, exactly as they were when
//! this was one number (§ 14 against § 9). Nothing here reads a parameter that
//! could make it round the other way, and the ties-resolve-down rule is stated
//! once for every derivation that produces an off-grid load — a back-off at 85%
//! of a top set, a warm-up at 40%, a reset drop at −10%.
//!
//! **What a scale is for one implement, not for one exercise.** Which scale
//! applies is [`crate::gym::exercise::Implement`]'s to say, and an implement
//! nobody has authored a scale for makes the exercise underivable rather than
//! silently borrowing the barbell's. A prescription derived from an invented
//! grid looks exactly like one derived from a real gym.

use std::fmt;

use crate::gym::{Kg, NonEmpty};

/// One band of a scale: a step size, and the load it starts applying at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Step {
    /// The lightest load this step size applies to.
    pub from: Kg,
    pub size: Kg,
}

/// Why a scale could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidLoadSteps {
    #[error("a scale with no bands describes no equipment")]
    NoBands,
    #[error("the first band must start at nothing, so every load falls inside the scale")]
    DoesNotStartAtZero,
    #[error("bands must ascend, and two starting at the same load are two answers")]
    NotAscending,
    #[error("a step of nothing never reaches the next load")]
    StepOfNothing,
}

/// The loads one implement can hold, as bands of step size.
///
/// Validated at construction (§ 24): the first band starts at nothing, bands
/// strictly ascend, and no step is zero. So [`LoadSteps::step_at`] is total —
/// there is no load outside the scale and no load with two answers — and
/// downstream code re-checks none of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadSteps(NonEmpty<Step>);

impl LoadSteps {
    /// A scale of one band. What a barbell is.
    ///
    /// # Errors
    ///
    /// [`InvalidLoadSteps::StepOfNothing`] for a step of zero.
    pub fn uniform(size: Kg) -> Result<Self, InvalidLoadSteps> {
        Self::new(vec![Step {
            from: Kg::NONE,
            size,
        }])
    }

    /// # Errors
    ///
    /// See [`InvalidLoadSteps`]. The bands are taken in the order given and are
    /// not sorted here: a scale authored out of order is a mistake worth
    /// reporting, not one worth quietly repairing.
    pub fn new(bands: Vec<Step>) -> Result<Self, InvalidLoadSteps> {
        let bands = NonEmpty::new(bands).map_err(|_| InvalidLoadSteps::NoBands)?;
        if bands.first().from.as_grams() != 0 {
            return Err(InvalidLoadSteps::DoesNotStartAtZero);
        }
        let mut previous: Option<Step> = None;
        for band in bands.iter() {
            if band.size.as_grams() == 0 {
                return Err(InvalidLoadSteps::StepOfNothing);
            }
            if let Some(previous) = previous
                && band.from.as_grams() <= previous.from.as_grams()
            {
                return Err(InvalidLoadSteps::NotAscending);
            }
            previous = Some(*band);
        }
        Ok(Self(bands))
    }

    pub const fn bands(&self) -> &NonEmpty<Step> {
        &self.0
    }

    /// The step size in force at a load.
    ///
    /// Total: the first band starts at nothing, so every load has one.
    #[must_use]
    pub fn step_at(&self, load: Kg) -> Kg {
        self.band_at(load).1.size
    }

    /// The band a load falls in, with its index.
    fn band_at(&self, load: Kg) -> (usize, Step) {
        let grams = load.as_grams();
        let mut found = (0, *self.0.first());
        for (index, band) in self.0.iter().enumerate() {
            if band.from.as_grams() <= grams {
                found = (index, *band);
            }
        }
        found
    }

    /// The nearest load this equipment can hold, ties resolving down.
    ///
    /// Integer throughout: no float touches a load on the way to a bar, for the
    /// same reason [`Kg`] holds grams.
    ///
    /// Where a band ends before its own next step would land, the neighbour's
    /// first load is what sits above — so the two candidates either side of a
    /// band boundary are both real loads rather than arithmetic.
    #[must_use]
    pub fn quantise(&self, load: Kg) -> Kg {
        let grams = load.as_grams();
        let (index, band) = self.band_at(load);
        let size = band.size.as_grams();
        let start = band.from.as_grams();

        let below = start + (grams - start) / size * size;
        if below == grams {
            return Kg::from_grams(below);
        }

        let stepped = below.saturating_add(size);
        let above = match self.0.iter().nth(index.saturating_add(1)) {
            Some(next) if next.from.as_grams() < stepped => next.from.as_grams(),
            _ => stepped,
        };

        // Strictly greater, which is the whole of "ties resolve down": at
        // exactly half a step the two distances are equal and the lower stands.
        if grams - below > above - grams {
            Kg::from_grams(above)
        } else {
            Kg::from_grams(below)
        }
    }

    /// Quantise, and never return nothing.
    ///
    /// A derived load below half a step quantises to zero, which is a real
    /// answer for an unloaded movement and a wrong one for a barbell. Callers
    /// prescribing a loaded slot use this; callers prescribing mobility work do
    /// not go through the quantiser at all.
    ///
    /// Deliberately not folded into [`LoadSteps::quantise`]: "round to the grid"
    /// and "never prescribe an empty bar" are two rules, and one function doing
    /// both would hide the second from anyone reading the first.
    #[must_use]
    pub fn quantise_loaded(&self, load: Kg) -> Kg {
        let quantised = self.quantise(load);
        if quantised.as_grams() == 0 {
            self.0.first().size
        } else {
            quantised
        }
    }

    /// One step up from a load, on this equipment.
    ///
    /// What double progression adds when the top of a range was reached at every
    /// working set. The step is read at the load being left, so a dumbbell
    /// leaving 10kg adds the 2kg that applies from 10kg and not the 1kg that got
    /// it there.
    #[must_use]
    pub fn next_above(&self, load: Kg) -> Kg {
        let stepped = load
            .as_grams()
            .saturating_add(self.step_at(load).as_grams());
        self.quantise(Kg::from_grams(stepped))
    }
}

impl fmt::Display for LoadSteps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bands = self.0.iter();
        if let Some(first) = bands.next() {
            write!(f, "{}kg", first.size)?;
        }
        for band in bands {
            write!(f, ", {}kg from {}kg", band.size, band.from)?;
        }
        Ok(())
    }
}
