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

use application::{MesocycleStore as _, PlanAuthor as _, prescribe::Authoring};
use domain::{
    prescription::{
        Anchor, AnchorProvenance, Anchoring, Entry, Mesocycle, Progression, authored::Shape,
    },
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
};
use jiff::civil::Date;
use support::programme;

/// Built by hand: `#[tokio::test]` expands with an `allow` this crate forbids.
fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

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

/// The same cycle, opening from whatever the mesocycle before it measures.
fn inheriting() -> Result<Mesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        Date::new(2026, 9, 14)?,
        Shape::Provided {
            from: provided()?,
            anchor: Anchoring::Inherited,
        },
    )?;
    Ok(programme::authoring(answers, &[])??)
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
            anchor: Anchoring::Stated(Entry::derived(anchor)),
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

/// **A cycle may open from the one before it rather than from a number** (issue
/// #86). The autumn holds three, and authoring it in September means the second
/// and third open from tests that have not happened — so they state nothing and
/// defer instead.
#[test]
fn a_cycle_may_inherit_what_it_opens_from() {
    let programme = inheriting().expect("the answers author");

    let Mesocycle::Progression(Progression::Provided { cycle, .. }) = &programme else {
        panic!("a provided cycle")
    };
    assert_eq!(cycle.entry(), Anchoring::Inherited);
    assert_eq!(
        programme.anchor(),
        None,
        "there is no authored number, and that absence is the statement",
    );
}

/// **And it claims nothing about the past, so there is nothing to check.**
///
/// `claims_an_earlier_maximum` gates the one rule that reads another mesocycle
/// in order to refuse this one. A stated `tested` anchor asserts that a test
/// happened and must be right about it; an inherited opening asserts nothing —
/// it defers to whichever test did happen, and is resolved against the record
/// when a session is asked for rather than when the plan is written.
#[test]
fn an_inherited_opening_makes_no_claim_to_check() {
    let programme = inheriting().expect("the answers author");

    assert!(
        !programme.claims_an_earlier_maximum(),
        "deferring to a test is not claiming one happened",
    );
    assert!(
        programme.produces_maximum().is_some(),
        "it still leaves one behind: week 4 day 2 is a single either way",
    );
}

/// The rule that a cycle may not contain the test anchoring it has nothing to
/// say here: an inherited opening names no date to be out of order.
#[test]
fn an_inherited_opening_has_no_date_to_be_wrong() {
    assert!(
        inheriting().is_ok(),
        "no anchor date, so the ordering rule cannot fire",
    );
}

/// **The absence survives the store**, which is the half a type cannot prove.
///
/// A null anchor is how `gym_mesocycle` records an inherited opening, and 0024's
/// `CHECK` allowed one only for a test — so this would have been refused on the
/// way in until 0025 relaxed it. Reading it back as `Anchoring::Inherited` rather
/// than as a corrupt row is the other half: the same three null columns mean
/// "corrupt" for a ladder and "inherits" for a cycle, and only the template
/// tells them apart.
#[test]
fn an_inherited_opening_round_trips_through_the_store() {
    runtime()
        .expect("a tokio runtime")
        .block_on(async {
            let directory = tempfile::tempdir()?;
            let pool = infrastructure::connect(&directory.path().join("test.db")).await?;
            let zone = support::corpus::zone()?;

            let plan = programme::plan(vec![inheriting()?])?;
            Authoring::new(
                infrastructure::SqlitePlanStore::new(pool.clone(), zone.clone()),
                infrastructure::SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
                infrastructure::SqliteGenerationParameterStore::new(pool.clone()),
            )
            .author(&plan, &programme::parameters()?)
            .await?;

            let store = infrastructure::SqliteGymMesocycleStore::new(pool, zone);
            let read = store.on(Date::new(2026, 9, 21)?).await?;
            let Some((_, _, Mesocycle::Progression(Progression::Provided { cycle, .. }))) = read
            else {
                panic!("the cycle authored above answers for a day inside it")
            };
            assert_eq!(
                cycle.entry(),
                Anchoring::Inherited,
                "a null anchor on a provided cycle reads back as inheritance, \
                 not as a row that got past the database",
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .expect("the round trip");
}
