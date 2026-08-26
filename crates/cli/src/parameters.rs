//! What this build ships as the operator's generation parameters (§ 14).
//!
//! **A seed, never a fallback** (decision 0015). `init` writes these into the
//! store, dated, and every derivation reads them from there — nothing on the
//! generation path can reach this module. What that buys is that the value in
//! force at a time is recoverable, and a default that changes later does not
//! rewrite anything already authored.
//!
//! **Preference, not domain fact**, which is why it lives here and not in
//! `domain` — the same standing [`candidates`](crate::candidates) has. That a
//! back-off is a share of the top set is true of anyone; that the share is 85%
//! is true of this operator, stated by him on the dates named below.
//!
//! **They were in a TOML document until 2026-08-26**, which is where the
//! comments come from. The document is gone and the numbers are not: each one
//! keeps the reasoning that settled it, because a parameter whose reason is
//! lost is a parameter nobody can argue with. Anything still marked
//! `INFERRED` was read off the record rather than stated, and is waiting to be
//! confirmed or corrected.

use std::collections::BTreeMap;

use domain::{
    gym::{Duration, Kg, NonEmpty, RepCount, exercise::Implement},
    prescription::{
        AccessoryScheme, BackOff, BlockRest, GenerationParameters, LoadSteps, PerRole, Percentage,
        ResetProtocol, RestScheme, Scales, Step, Target, TopSetReps, WarmupStep,
    },
};

use crate::{Failure, exit};

/// A value in this file that will not build.
///
/// Every one of them is written a few lines above, so this is a defect in the
/// build rather than anything an operator did — but panicking is forbidden and
/// so is pretending it cannot happen, and [`pinned`](self) is the test that
/// keeps it from happening.
fn wrong(what: &str, error: impl std::fmt::Display) -> Failure {
    Failure::message(
        format!("the shipped {what} will not build ({error}) — this is a defect in this build"),
        exit::USAGE,
    )
}

fn percentage(what: &str, stated: &str) -> Result<Percentage, Failure> {
    Percentage::try_from(stated.to_owned()).map_err(|error| wrong(what, error))
}

fn mass(what: &str, kilos: &str) -> Result<Kg, Failure> {
    Kg::try_from(kilos.to_owned()).map_err(|error| wrong(what, error))
}

fn count(what: &str, reps: u32) -> Result<RepCount, Failure> {
    RepCount::new(reps).map_err(|error| wrong(what, error))
}

/// A rest of one duration.
const fn exactly(seconds: u64) -> BlockRest {
    BlockRest {
        between_sets: Target::Exactly(Duration::from_seconds(seconds)),
        after_superset: None,
    }
}

/// A rest stated as a band, and what the block rests once a superset ends.
///
/// **The domain holds a span rather than two bounds**, so an inverted range is
/// not expressible past this point — which is why the seed states `120, 180`
/// and gets `None` rather than a backwards rest if anyone swaps them.
fn banded(what: &str, between: (u64, u64), after: (u64, u64)) -> Result<BlockRest, Failure> {
    let span = |low: u64, high: u64| {
        Target::between(Duration::from_seconds(low), Duration::from_seconds(high))
            .ok_or_else(|| wrong(what, "a rest range runs low-high and must span"))
    };
    Ok(BlockRest {
        between_sets: span(between.0, between.1)?,
        after_superset: Some(span(after.0, after.1)?),
    })
}

/// The double-progression scheme a block's non-primary slots run.
fn scheme(what: &str, reps: (u32, u32), sets: u32) -> Result<AccessoryScheme, Failure> {
    Ok(AccessoryScheme {
        reps: Target::between(count(what, reps.0)?, count(what, reps.1)?)
            .ok_or_else(|| wrong(what, "a rep range runs low-high and must span"))?,
        sets: count(what, sets)?,
    })
}

/// The set this build ships.
///
/// # Errors
///
/// [`Failure`] if a value written here does not build, which is a defect in
/// this build rather than anything the operator can correct.
pub fn seed() -> Result<GenerationParameters, Failure> {
    // The operator's own ramp: 4 at 40%, 3 at 60%, 2 at 80%, 1 at 90%, all of
    // the top set rather than of the anchor.
    let mut warmup = Vec::with_capacity(4);
    for (of_top_set, reps) in [("40%", 4), ("60%", 3), ("80%", 2), ("90%", 1)] {
        warmup.push(WarmupStep {
            of_top_set: percentage("warm-up ramp", of_top_set)?,
            reps: count("warm-up ramp", reps)?,
        });
    }
    let warmup = NonEmpty::new(warmup).map_err(|_| wrong("warm-up ramp", "a ramp needs a step"))?;

    // **A scale, not an increment.** One increment for everything prescribed a
    // dumbbell at 12.5kg and another at 9.5kg — neither of which is a dumbbell.
    // A bare step is uniform; a list is banded, lightest first, and the first
    // band must start at 0kg so that every load has a step.
    //
    // An implement absent from here is not defaulted to the barbell's. A slot
    // loaded on it re-issues what it last did and reports as underivable only
    // on the week it would have stepped up — which is the week to come back and
    // state the scale.
    let mut scales = BTreeMap::new();
    for (implement, size) in [
        (Implement::Barbell, "2.5"),
        (Implement::Machine, "2.5"),
        // INFERRED. The operator was checking the stack at the gym on
        // 2026-08-21.
        (Implement::Cable, "2.5"),
    ] {
        let steps = LoadSteps::uniform(mass("load scale", size)?)
            .map_err(|error| wrong("load scale", error))?;
        scales.insert(implement, steps);
    }
    // Whole kilos to 10kg, twos above it. 9 -> 10 -> 12 in the record.
    let dumbbell = LoadSteps::new(vec![
        Step {
            from: Kg::NONE,
            size: mass("dumbbell scale", "1")?,
        },
        Step {
            from: mass("dumbbell scale", "10")?,
            size: mass("dumbbell scale", "2")?,
        },
    ])
    .map_err(|error| wrong("dumbbell scale", error))?;
    scales.insert(Implement::Dumbbell, dumbbell);

    Ok(GenerationParameters {
        warmup,

        // **The primary's back-off sets, per session role — its own pattern,
        // not the strength block's accessory scheme.** They used to be read off
        // the accessory scheme on the grounds that the primary is a strength
        // slot and nobody had stated otherwise, which issued the light
        // session's three sets of six on the heavy day. Stated by the operator
        // on 2026-08-20; the record agrees on every session since the July
        // test.
        back_off: PerRole {
            heavy: BackOff {
                sets: count("heavy back-off", 2)?,
                reps: count("heavy back-off", 4)?,
                of_top_set: percentage("heavy back-off", "85%")?,
            },
            light: BackOff {
                sets: count("light back-off", 3)?,
                reps: count("light back-off", 6)?,
                of_top_set: percentage("light back-off", "85%")?,
            },
        },

        // The light session's top set, as a percentage of that week's heavy top
        // set. Stated by the operator on 2026-08-18, and the first version of
        // this number was wrong in an instructive way.
        //
        // It was 88.5%, solved from three weeks of light/heavy pairs — 72.5 /
        // 75 / 77.5 against 82.5 / 85 / 87.5. Every one of those pairs is a flat
        // -10kg. The percentage was a ratio fitted to an offset: it reproduces
        // all three only because quantisation rounds it back onto the plate
        // grid, and it drifts across them (87.9%, 88.2%, 88.6%) where the offset
        // does not drift at all. Fitting a parameter to the record produced a
        // number with a decimal place and no decision behind it.
        //
        // 85% is a decision. It is one plate lighter than the record's light
        // days, it is inside the 70-90% band that Starr, the Texas Method and
        // DUP all land in, and it is a percentage rather than an offset so it
        // stays meaningful when the anchor moves.
        light_of_heavy: percentage("light session's share", "85%")?,

        // **The ladder has no endpoint and no opening.** The climb runs at its
        // stated rate until the calendar stops it, and what regulates it is the
        // reset protocol rather than a stated top. Where it opens comes from the
        // entry test. Both settled by the operator on 2026-08-19; see decisions
        // 0008 and 0009.
        //
        // The rate is a mass rather than a percentage, and it is the same kind
        // of thing as the two reclimb rates below: a reset is this climb run at
        // a different rate off a lower start. 2.5kg is also the smallest plate,
        // so the climb lands on the grid at every anchor — the fault
        // `light_of_heavy` was caught with was a ratio standing in for an
        // offset, and this is the offset stated as one.
        ladder_climb_per_week: mass("ladder climb", "2.5")?,

        // What a *derived* opening drops off the load the entry test failed.
        //
        // Authored rather than read off the first reset's drop, which it happens
        // to equal. Two values agreeing by decision is not one value used twice,
        // and only the first survives either of them being changed.
        entry_drop: percentage("entry drop", "-10%")?,

        // INFERRED. The primary's top set, per session role, read off every
        // session since the July test. Well evidenced — they have not varied
        // within a role — and still not stated.
        //
        // Constant within a block either way: descending reps across the block,
        // fives then threes then singles, is the textbook linear variant and is
        // deferred.
        top_set_reps: PerRole {
            light: TopSetReps::new(count("light top set", 3)?),
            heavy: TopSetReps::new(count("heavy top set", 1)?),
        },

        // INFERRED. The ranges were eyeballed from pull-ups at six, curls around
        // four to six and wrist work at six, and are unconfirmed. One scheme per
        // block rather than one per slot: the slots within a block are
        // prescribed alike, and the two blocks differ from each other.
        strength: scheme("strength scheme", (4, 6), 3)?,
        hypertrophy: scheme("hypertrophy scheme", (4, 6), 3)?,

        // How long to rest between sets, block by block. All of it stated by the
        // operator on 2026-08-23.
        //
        // **Zero between the members of a superset is not stated here**, because
        // it is what a superset is rather than a value to choose: a set another
        // member follows rests for zero by definition, and `after_superset` is
        // what the group rests for once it ends.
        //
        // `after_superset` absent means the block rests the same however its
        // work is grouped — which is not the same as resting for zero, and is
        // why plyometric and power leave it out rather than set it to zero.
        //
        // The warm-up ramp is not here either. It instructs no rest at all —
        // changing the plates is the rest — except for the step into the working
        // set, which takes the bottom of its block's range. That is a rule
        // rather than a number, so it lives in `domain::prescription::rest` and
        // nothing authors it.
        rest: RestScheme {
            plyometric: exactly(30),
            power: exactly(90),
            strength: banded("strength rest", (120, 180), (90, 150))?,
            // The same as strength, written out rather than pointing at it. The
            // operator is not sure the two should agree, and two numbers to
            // change is the shape that makes disagreeing a one-line edit
            // instead of a restructuring.
            hypertrophy: banded("hypertrophy rest", (120, 180), (90, 150))?,
            // Mobility work runs straight through.
            mobility: exactly(0),
        },

        // How long a static hold is held for. The mobility work does not
        // progress; it is held, and for the same length every time.
        static_hold: Duration::from_seconds(60),

        scales: Scales::new(scales),

        // From docs/primary-lift-progression.md. The drop and the increment are
        // chosen as a pair so both land on the plate grid and both cost four
        // weeks — so a stall has a fixed price whichever reset is in play.
        first_reset: ResetProtocol {
            drop: percentage("first reset", "-10%")?,
            reclimb_per_week: mass("first reset", "5")?,
        },
        second_reset: ResetProtocol {
            drop: percentage("second reset", "-5%")?,
            reclimb_per_week: mass("second reset", "2.5")?,
        },
    })
}

/// The seed's own test.
///
/// **A composed default is invisible to every other test.** The contract suites
/// and the store suites all take parameters from a fixture, so a value written
/// wrong here would prescribe wrong loads on a real machine and pass everywhere
/// else — the same class of fault as a base URL that already ended in `/v1`. So
/// the numbers are pinned where they are written.
#[cfg(test)]
mod pinned {
    use super::seed;

    #[test]
    fn the_shipped_set_builds() {
        seed().expect("the shipped parameters build");
    }

    #[test]
    fn the_numbers_are_the_operators() {
        let seeded = seed().expect("the shipped parameters build");

        assert_eq!(seeded.light_of_heavy.to_string(), "85%");
        assert_eq!(seeded.ladder_climb_per_week.to_string(), "2.5");
        assert_eq!(seeded.entry_drop.to_string(), "-10%");
        assert_eq!(seeded.static_hold.as_seconds(), 60);

        assert_eq!(seeded.first_reset.drop.to_string(), "-10%");
        assert_eq!(seeded.first_reset.reclimb_per_week.to_string(), "5");
        assert_eq!(seeded.second_reset.drop.to_string(), "-5%");
        assert_eq!(seeded.second_reset.reclimb_per_week.to_string(), "2.5");

        // 4 at 40%, 3 at 60%, 2 at 80%, 1 at 90%. Of the top set, never of the
        // anchor.
        let ramp: Vec<(String, u32)> = seeded
            .warmup
            .iter()
            .map(|step| (step.of_top_set.to_string(), step.reps.as_u32()))
            .collect();
        assert_eq!(
            ramp,
            vec![
                ("40%".to_owned(), 4),
                ("60%".to_owned(), 3),
                ("80%".to_owned(), 2),
                ("90%".to_owned(), 1),
            ]
        );

        // Heavy is `1 @ x, 2 × 4`; light is `3 @ x, 3 × 6`. The two roles differ,
        // which is the whole reason back-off is not read off the accessory
        // scheme.
        assert_eq!(seeded.back_off.heavy.sets.as_u32(), 2);
        assert_eq!(seeded.back_off.heavy.reps.as_u32(), 4);
        assert_eq!(seeded.top_set_reps.heavy.as_rep_count().as_u32(), 1);
        assert_eq!(seeded.back_off.light.sets.as_u32(), 3);
        assert_eq!(seeded.back_off.light.reps.as_u32(), 6);
        assert_eq!(seeded.top_set_reps.light.as_rep_count().as_u32(), 3);
    }

    /// **The dumbbell scale is banded and the rest are not**, and getting that
    /// wrong prescribes a 9.5kg dumbbell — a load no rack holds.
    #[test]
    fn every_implement_that_is_loaded_has_a_scale() {
        use domain::gym::{Kg, exercise::Implement};

        let seeded = seed().expect("the shipped parameters build");

        for implement in [Implement::Barbell, Implement::Machine, Implement::Cable] {
            let steps = seeded
                .scales
                .for_implement(implement)
                .expect("a scale for every implement the seed states");
            assert_eq!(steps.bands().count(), 1, "{implement} is a uniform scale");
            assert_eq!(steps.bands().first().size.to_string(), "2.5");
        }

        let dumbbells = seeded
            .scales
            .for_implement(Implement::Dumbbell)
            .expect("a dumbbell scale");
        // Whole kilos to 10kg, twos above it. 9 -> 10 -> 12 in the record.
        assert_eq!(dumbbells.bands().count(), 2);
        assert_eq!(dumbbells.step_at(Kg::from_grams(9_000)).to_string(), "1");
        assert_eq!(dumbbells.step_at(Kg::from_grams(12_000)).to_string(), "2");
    }
}
