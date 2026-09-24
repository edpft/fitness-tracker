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
    prescription::{GymMesocycle, Progression, authored::Shape},
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

/// The cycle the autumn opens with: four weeks from Monday 14 September.
///
/// **It states nothing about what it opens from**, because no cycle does any
/// more: the anchor belongs to the microcycle and is read off whatever measured
/// the lift before it, when a session is asked for.
fn cycle() -> Result<GymMesocycle, Box<dyn std::error::Error>> {
    let answers = programme::authored(
        Date::new(2026, 9, 14)?,
        Shape::Provided { from: provided()? },
    )?;
    Ok(programme::authoring(answers, &[])??)
}

#[test]
fn the_answers_author_an_sbs_cycle() {
    let programme = cycle().expect("the answers author");

    assert_eq!(programme.template(), "sbs");
    let GymMesocycle::Progression(Progression::Provided { from, .. }) = &programme else {
        panic!("a provided cycle knows where it came from")
    };
    assert_eq!(from.programme().name().as_str(), "Squat 2x Int");
    assert_eq!(from.to_string(), "micros 1-2-3-4");
    assert!(
        matches!(
            programme,
            GymMesocycle::Progression(Progression::Provided { .. })
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
fn the_cycle_leaves_a_maximum_behind_it() {
    let programme = cycle().expect("the answers author");

    assert!(
        programme.produces_maximum().is_some(),
        "week 4 day 2 is a single, and it is what opens the next cycle",
    );
}

/// **A cycle that states no anchor survives the store**, which is the half a
/// type cannot prove.
///
/// The anchor columns are a test's alone now, and a provided cycle carries none
/// — so what this asserts is that a row with all four null reads back as a
/// cycle rather than as one that got past the database.
#[test]
fn a_cycle_states_no_anchor_and_round_trips_through_the_store() {
    runtime()
        .expect("a tokio runtime")
        .block_on(async {
            let directory = tempfile::tempdir()?;
            let pool = infrastructure::connect(&directory.path().join("test.db")).await?;
            // A block's calendar is rebuilt from the operator's week on every
            // read (issue #63), so a store with no week in it cannot hold a
            // plan.
            programme::record_the_week(&pool).await?;
            let zone = support::corpus::zone()?;

            let plan = programme::plan(vec![cycle()?])?;
            Authoring::new(
                infrastructure::SqlitePlanStore::new(pool.clone(), zone.clone()),
                infrastructure::SqliteGenerationParameterStore::new(pool.clone()),
            )
            .author(&plan, &programme::parameters()?)
            .await?;

            let store = infrastructure::SqliteGymMesocycleStore::new(pool, zone);
            let read = store.on(Date::new(2026, 9, 21)?).await?;
            let Some((_, _, read)) = read else {
                panic!("the cycle authored above answers for a day inside it")
            };
            assert!(
                matches!(
                    read,
                    GymMesocycle::Progression(Progression::Provided { .. })
                ),
                "a row with no anchor reads back as a provided cycle, not as a \
                 row that got past the database",
            );
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .expect("the round trip");
}
