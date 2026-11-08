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

use crate::support;

use application::{MesocycleStore as _, PlanAuthor as _, prescribe::Authoring};
use domain::{
    prescription::{GymMesocycle, Progression, authored::Shape},
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
};
use jiff::civil::Date;
use support::mesocycle;

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
    let answers = mesocycle::authored(
        Date::new(2026, 9, 14)?,
        Shape::Provided { from: provided()? },
    )?;
    Ok(mesocycle::authoring(answers, &[])??)
}

#[test]
fn the_answers_author_an_sbs_cycle() {
    let mesocycle = cycle().expect("the answers author");

    assert_eq!(mesocycle.template(), "sbs");
    let GymMesocycle::Progression(Progression::Provided { from, .. }) = &mesocycle else {
        panic!("a provided cycle knows where it came from")
    };
    assert_eq!(from.programme().name().as_str(), "Squat 2x Int");
    assert_eq!(from.to_string(), "micros 1-2-3-4");
    assert!(
        matches!(
            mesocycle,
            GymMesocycle::Progression(Progression::Provided { .. })
        ),
        "and it is a periodisation, beside linear and block",
    );
    assert_eq!(
        mesocycle.calendar().duration_weeks(),
        4,
        "four weeks, taken from the chart rather than from the answers",
    );
}

#[test]
fn the_cycle_leaves_a_maximum_behind_it() {
    let mesocycle = cycle().expect("the answers author");

    assert!(
        mesocycle.produces_maximum().is_some(),
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
            mesocycle::record_the_week(&pool).await?;
            let zone = support::corpus::zone()?;

            let plan = mesocycle::plan(vec![cycle()?])?;
            mesocycle::design(&pool).await?;
            Authoring::new(infrastructure::SqlitePlanStore::new(
                pool.clone(),
                zone.clone(),
            ))
            .author(&plan)
            .await?;

            let store = infrastructure::SqliteGymMesocycleStore::new(pool, zone);
            let read = store.on(Date::new(2026, 9, 21)?).await?;
            let Some(read) = read.map(|found| found.mesocycle) else {
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

/// **An SBS week can be missed without being run again, except its test**
/// (#732). Its essential session is a repetition-maximum day that sets the
/// next week's training maximum, so the programme regulates itself; week 4's
/// is the one-repetition maximum, which anchors what follows.
#[test]
fn the_cycle_regulates_itself_except_in_its_test_week() {
    let mesocycle = cycle().expect("the answers author");
    let calendar = mesocycle.calendar();

    let regulates: Vec<bool> = [14, 21, 28]
        .into_iter()
        .map(|day| Date::new(2026, 9, day).expect("a date"))
        .chain([Date::new(2026, 10, 5).expect("a date")])
        .map(|monday| {
            let week = calendar.place_week(monday).expect("a week of the cycle");
            mesocycle.regulates_itself_in(week)
        })
        .collect();

    assert_eq!(regulates, vec![true, true, true, false]);
}

/// **A cycle cut to x-2-3-4** (#732): three calendar weeks running chart weeks
/// 2, 3 and 4, its test still the last.
#[test]
fn a_cut_cycle_runs_the_weeks_it_took() {
    let answers = mesocycle::authored(
        Date::new(2026, 11, 16).expect("a date"),
        Shape::Provided {
            from: ProvidedFrom::new(
                ExternalProgramme::new(
                    Provider::try_from("Stronger By Science".to_owned()).expect("a provider"),
                    ProgrammeName::try_from("Squat 2x Int".to_owned()).expect("a name"),
                ),
                vec![2, 3, 4],
            )
            .expect("a selection"),
        },
    )
    .expect("answers");
    let mesocycle = mesocycle::authoring(answers, &[])
        .expect("authoring runs")
        .expect("the cut authors");
    let calendar = mesocycle.calendar();
    assert_eq!(calendar.duration_weeks(), 3);

    let weeks: Vec<(Option<u32>, bool)> = [16, 23, 30]
        .into_iter()
        .map(|day| {
            let week = calendar
                .place_week(Date::new(2026, 11, day).expect("a date"))
                .expect("a week of the cycle");
            (mesocycle.published_week(week), mesocycle.tests_in(week))
        })
        .collect();
    assert_eq!(
        weeks,
        vec![(Some(2), false), (Some(3), false), (Some(4), true)]
    );
}

/// **Only the chart's weeks, in its order.** A fifth week is not SBS run
/// longer, and 3-2-4 is not a selection of it.
#[test]
fn a_selection_outside_the_chart_is_refused() {
    for weeks in [vec![3, 2, 4], vec![2, 3, 4, 5]] {
        let answers = mesocycle::authored(
            Date::new(2026, 11, 16).expect("a date"),
            Shape::Provided {
                from: ProvidedFrom::new(
                    ExternalProgramme::new(
                        Provider::try_from("Stronger By Science".to_owned()).expect("a provider"),
                        ProgrammeName::try_from("Squat 2x Int".to_owned()).expect("a name"),
                    ),
                    weeks.clone(),
                )
                .expect("a selection"),
            },
        )
        .expect("answers");
        assert!(
            mesocycle::authoring(answers, &[])
                .expect("authoring runs")
                .is_err(),
            "{weeks:?} is refused"
        );
    }
}
