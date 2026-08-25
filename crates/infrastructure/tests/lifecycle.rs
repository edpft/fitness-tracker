//! A prescription is drafted, published, or performed (§ 12.1).
//!
//! **The state is derived from the relations.** No column holds it, so there is
//! nothing to keep in step: a prescription with no delivery is drafted, one
//! whose delivery reference no workout names is published, and one a workout
//! names is performed.
//!
//! What each state permits is the point of having them. The first two are cheap
//! and may be thrown away; the third is pinned by an observation and may not.

mod support;

use application::{
    PrescriptionDeliveryStore as _, PrescriptionLifecycle as _, WorkoutPrescriber as _,
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::prescription::{DeliveryReference, DestinationName, PrescriptionState};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore,
};
use jiff::Timestamp;
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::corpus;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the store answers: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// Macros rather than functions: `panic` is forbidden in a free function even
/// in a test file, and the exemption reaches a `#[test]` body, which is where a
/// macro expands.
macro_rules! hevy {
    () => {
        match DestinationName::try_from("hevy".to_owned()) {
            Ok(name) => name,
            Err(error) => panic!("hevy is a destination name: {error}"),
        }
    };
}

macro_rules! reference {
    ($id:literal) => {
        match DeliveryReference::try_from($id.to_owned()) {
            Ok(reference) => reference,
            Err(error) => panic!("{} is a reference: {error}", $id),
        }
    };
}

/// A store with one issued prescription in it, and its id.
///
/// Built through the real prescribing path, because a hand-inserted row would
/// not prove the join works against what the tool actually writes.
async fn issued() -> Fallible<(tempfile::TempDir, SqlitePool, i64)> {
    let (directory, pool) = support::store::derived_and_authored().await?;

    // Issued through the real path: a hand-inserted row would not prove the
    // join works against what the tool actually writes.
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), "Europe/London".to_owned()),
    });
    // Week 6 of the fixture block, a light session.
    prescriber
        .prescribe(Date::constant(2026, 8, 10), application::Reissue::No)
        .await?;

    let id: i64 = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM prescribed_workout ORDER BY id LIMIT 1"#
    )
    .fetch_optional(&pool)
    .await?
    .ok_or("the fixture issues no prescription")?;

    Ok((directory, pool, id))
}

/// **Issued and nowhere else.** Nothing outside the store knows it exists.
#[test]
fn a_prescription_nobody_has_been_given_is_drafted() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool);

    let state = run!(deliveries.state_of(application::PrescribedWorkoutId::new(id)));

    assert_eq!(state, PrescriptionState::Drafted);
    assert!(state.is_disposable(), "a draft may be thrown away");
    assert_eq!(state.reference(), None);
}

/// **Delivered, and fixed by the reference the destination gave it** — but
/// still cheap, because nobody has performed it.
#[test]
fn a_delivered_prescription_nobody_has_performed_is_published() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool);
    let prescription = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        prescription,
        &hevy!(),
        &reference!("routine-1"),
        Timestamp::UNIX_EPOCH,
    ));

    let state = run!(deliveries.state_of(prescription));

    assert_eq!(
        state,
        PrescriptionState::Published {
            reference: reference!("routine-1")
        }
    );
    assert!(
        state.is_disposable(),
        "published is still cheap: withdrawing it removes the session at the \
         destination, and loses nothing else"
    );
}

/// **A workout names it, so it happened.**
///
/// And the date it happened on has nothing to do with it: this is what lets a
/// Friday session performed on Saturday morning still be that session.
#[test]
fn a_prescription_a_workout_names_is_performed() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let prescription = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        prescription,
        &hevy!(),
        &reference!("routine-2"),
        Timestamp::UNIX_EPOCH,
    ));

    // A workout in the store, performed against that routine. Which workout,
    // and on what date, is deliberately not asked: the join is the reference.
    run!(async {
        sqlx::query!(
            "UPDATE gym_workout SET performed_against = 'routine-2' \
             WHERE landing_record_id = (SELECT MIN(landing_record_id) FROM gym_workout)"
        )
        .execute(&pool)
        .await
    });

    let state = run!(deliveries.state_of(prescription));

    assert_eq!(
        state,
        PrescriptionState::Performed {
            reference: reference!("routine-2")
        }
    );
    assert!(
        !state.is_disposable(),
        "what it records happened, so it is not deletable"
    );
}

/// **A performed prescription is refused deletion by the schema.**
///
/// The rule lives in a trigger rather than in code, so it holds against every
/// writer that ever exists — including a hand-typed `DELETE` at a prompt. Raw
/// landing is append-only the same way, and for the same reason: what it
/// records happened.
///
/// **What this does not yet show is a drafted one being deleted**, because it
/// cannot be: `prescribed_item` and its children reference `prescribed_workout`
/// without `ON DELETE CASCADE`, so the row is pinned by a foreign key whatever
/// state it is in. § 12.1 says a draft is disposable and today nothing can
/// dispose of it — which is work for the withdrawal that has not been built
/// yet, and is recorded here rather than left to be discovered then.
#[test]
fn the_store_refuses_to_delete_a_performed_prescription() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let prescription = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        prescription,
        &hevy!(),
        &reference!("routine-3"),
        Timestamp::UNIX_EPOCH,
    ));
    run!(async {
        sqlx::query!(
            "UPDATE gym_workout SET performed_against = 'routine-3' \
             WHERE landing_record_id = (SELECT MIN(landing_record_id) FROM gym_workout)"
        )
        .execute(&pool)
        .await
    });

    // The trigger, not the foreign key: it names the rule it is enforcing.
    let refused = corpus::block_on(async {
        sqlx::query!("DELETE FROM prescribed_workout WHERE id = ?", id)
            .execute(&pool)
            .await
    });
    match refused {
        Ok(Err(error)) => assert!(
            error.to_string().contains("not deletable"),
            "refused by the trigger, and says why: {error}"
        ),
        Ok(Ok(_)) => panic!("a performed prescription was deleted"),
        Err(error) => panic!("a runtime is available: {error}"),
    }

    // And withdrawing its delivery is refused too: the receipt is what the
    // performance is joined through, so removing it would make a performed
    // prescription look drafted again.
    let withdrawn = corpus::block_on(async {
        sqlx::query!(
            "DELETE FROM prescription_delivery WHERE prescription = ?",
            id
        )
        .execute(&pool)
        .await
    });
    match withdrawn {
        Ok(Err(error)) => assert!(
            error.to_string().contains("not withdrawable"),
            "refused by the trigger, and says why: {error}"
        ),
        Ok(Ok(_)) => panic!("a performed session was withdrawn"),
        Err(error) => panic!("a runtime is available: {error}"),
    }

    assert!(matches!(
        run!(deliveries.state_of(prescription)),
        PrescriptionState::Performed { .. }
    ));
}
