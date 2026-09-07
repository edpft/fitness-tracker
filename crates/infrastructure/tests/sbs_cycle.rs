//! The SBS cycle as an operator authors it, and what it refuses.
//!
//! **Four of these tests went with the document format on 2026-09-06**, and
//! that is the point of removing it: a chart that states every set has nothing
//! for an `opening`, an `entry_test`, a `duration_weeks` or a `gating_role` to
//! mean, and each of those had a test proving the reader refused one.
//! `Shape::Provided` has nowhere to put any of them, so there is nothing left
//! to refuse and nothing left to test.
//!
//! What the *chart* prescribes is proved in `domain/tests/sbs.rs` against the
//! workbook. This is about the programme built around it.

mod support;

use domain::{
    prescription::{Anchor, AnchorProvenance, Mesocycle, Progression, authored::Shape},
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
};
use jiff::civil::Date;
use support::programme;

/// *Squat 2x Int*, published by Stronger By Science: the whole four-week chart.
fn provided() -> Result<ProvidedFrom, Box<dyn std::error::Error>> {
    Ok(ProvidedFrom::new(
        ExternalProgramme::new(
            Provider::try_from("Stronger By Science".to_owned())?,
            ProgrammeName::try_from("Squat 2x Int".to_owned())?,
        ),
        vec![1, 2, 3, 4],
    )?)
}

/// The cycle the autumn opens with: four weeks from Monday 14 September.
fn cycle(anchored_on: (i16, i8, i8)) -> Result<Mesocycle, Box<dyn std::error::Error>> {
    let (year, month, day) = anchored_on;
    // From the standalone week 4 that runs first, so the cycle opens on a
    // measured number rather than an expectation (decision 0024).
    let anchor = Anchor::new(
        "100".to_owned().try_into()?,
        None,
        AnchorProvenance::Tested,
        Date::new(year, month, day)?,
    )?;
    let answers = programme::authored(
        Date::new(2026, 9, 14)?,
        Shape::Provided {
            from: provided()?,
            anchor,
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

#[test]
fn the_answers_author_an_sbs_cycle() {
    let programme = cycle((2026, 9, 11)).expect("the answers author");

    assert_eq!(programme.template(), "sbs");
    let Mesocycle::Progression(Progression::Provided { from, .. }) = &programme else {
        panic!("a provided cycle knows where it came from")
    };
    assert_eq!(from.programme().name().as_str(), "Squat 2x Int");
    assert_eq!(from.to_string(), "micros 1-2-3-4");
    assert!(
        matches!(
            programme,
            Mesocycle::Progression(Progression::Provided { .. })
        ),
        "and it is a periodisation, beside linear and block",
    );
    assert_eq!(
        programme.calendar().duration_weeks(),
        4,
        "four weeks, taken from the chart rather than from the answers",
    );
}

#[test]
fn the_cycle_leaves_a_maximum_and_claims_one() {
    let programme = cycle((2026, 9, 11)).expect("the answers author");

    assert!(
        programme.produces_maximum().is_some(),
        "week 4 day 2 is a single, and it is what opens the next cycle",
    );
    assert!(
        programme.claims_an_earlier_maximum(),
        "a tested anchor here can only point backwards: the chart's test is its \
         last session, so there is no case where the cycle is about to measure \
         its own opening",
    );
}

#[test]
fn a_cycle_whose_anchor_is_not_before_it_is_refused() {
    // The anchor dated on the cycle's own start day. Refused for the reason the
    // linear template refuses it: a cycle containing the session that anchors it
    // would read that session twice.
    assert!(
        cycle((2026, 9, 14)).is_err(),
        "the test precedes the cycle it anchors"
    );
}
