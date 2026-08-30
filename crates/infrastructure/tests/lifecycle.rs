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
    Issuance, PrescriptionDeliveryStore as _, PrescriptionLifecycle as _, ProgrammeAuthor as _,
    WorkoutPrescriber as _,
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
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
        lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
    });
    // Week 6 of the fixture block, a light session.
    prescriber.prescribe(Date::constant(2026, 8, 10)).await?;

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

/// **A performed prescription is not derived again** — decision 0021, and the one
/// rule the shape does not get a say in.
///
/// The programme is re-authored underneath it so that the derivation *would*
/// produce a different session, which is what makes this a test rather than a
/// restatement: the session stands because it was performed, not because
/// nothing changed.
///
/// The reason is `compare`, which reads the prescription in force for a date.
/// Superseding this one would leave the performance measured against a session
/// that was never trained.
#[test]
fn a_performed_prescription_is_not_derived_again() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let prescription = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        prescription,
        &hevy!(),
        &reference!("routine-4"),
        Timestamp::UNIX_EPOCH,
    ));
    run!(async {
        sqlx::query!(
            "UPDATE gym_workout SET performed_against = 'routine-4' \
             WHERE landing_record_id = (SELECT MIN(landing_record_id) FROM gym_workout)"
        )
        .execute(&pool)
        .await
    });

    let issued = run!(async {
        // A fortnight later a start puts the same date on a different rung, so
        // a derivation that ran would not agree with what was performed.
        Authoring::new(
            SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
            SqliteGenerationParameterStore::new(pool.clone()),
        )
        .author(
            &support::programme::as_programme(support::programme::programme_from(Date::constant(
                2026, 7, 20,
            ))?),
            &support::programme::parameters()?,
        )
        .await?;

        let prescriber = Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
        });
        Ok::<_, Box<dyn std::error::Error>>(
            prescriber.prescribe(Date::constant(2026, 8, 10)).await?,
        )
    });

    assert_eq!(
        issued.issuance,
        Issuance::Performed {
            reference: reference!("routine-4")
        }
    );
    assert_eq!(
        issued.id, prescription,
        "the prescription in force is the one that was performed"
    );

    let count: i64 = run!(async {
        sqlx::query_scalar!(r#"SELECT COUNT(*) AS "n!: i64" FROM prescribed_workout"#)
            .fetch_one(&pool)
            .await
    });
    assert_eq!(count, 1, "and nothing was written beside it");
}

/// **A session trained after being superseded is still the one in force.**
///
/// The window this closes is narrow and entirely reachable: a session is
/// delivered, a re-derivation supersedes it while it is still merely published,
/// and then the operator trains the routine already on their phone. Without the
/// ordering the store applies, the newest row would be one nobody ever saw —
/// `compare` would measure the performance against it, and `prescribe` would go
/// on superseding a session that has been trained.
///
/// The rule lives in the query rather than in the three use cases that would
/// otherwise each have to remember it.
#[test]
fn a_superseded_session_that_was_trained_is_the_one_in_force() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let performed = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        performed,
        &hevy!(),
        &reference!("routine-5"),
        Timestamp::UNIX_EPOCH,
    ));

    // Superseded while still merely published: a second issue for the same
    // date, written later.
    let later: i64 = run!(async {
        sqlx::query_scalar!(
            r#"INSERT INTO prescribed_workout (
                   programme, issued_for, zone, session_role, week_kind, week_index,
                   anchor_grams, anchor_provenance, anchor_from,
                   parameters_authored_at, issued_at
               )
               SELECT programme, issued_for, zone, session_role, week_kind, week_index,
                      anchor_grams, anchor_provenance, anchor_from,
                      parameters_authored_at, '2099-01-01T00:00:00Z'
               FROM prescribed_workout WHERE id = ?
               RETURNING id AS "id!: i64""#,
            id
        )
        .fetch_one(&pool)
        .await
    });
    assert_ne!(later, id, "the supersession is a row of its own");

    // And only then trained, against the routine already delivered.
    run!(async {
        sqlx::query!(
            "UPDATE gym_workout SET performed_against = 'routine-5' \
             WHERE landing_record_id = (SELECT MIN(landing_record_id) FROM gym_workout)"
        )
        .execute(&pool)
        .await
    });

    let in_force = run!(async {
        application::PrescribedWorkoutStore::issued_for(
            &SqlitePrescribedWorkoutStore::new(pool.clone(), "Europe/London".to_owned()),
            Date::constant(2026, 8, 10),
        )
        .await
    });

    let Some((found, _)) = in_force else {
        panic!("the date holds two prescriptions")
    };
    assert_eq!(
        found, performed,
        "the session that was trained, not the newer one nobody saw"
    );
}

/// **A performed session's place is not handed over.**
///
/// Decision 0022 routes a replacement through a delete and an insert precisely
/// so that this holds: the delete is what
/// `prescription_delivery_performed_is_not_deletable` watches, and an
/// `UPDATE ... SET prescription = ?` would have slid straight past it.
///
/// In practice `prescribe` never offers the chance — a performed prescription
/// is the one in force, so nothing supersedes it and `deliver` finds the place
/// already held by the session it is delivering. This is the floor under that,
/// held by the schema rather than by the use case remembering.
#[test]
fn a_performed_sessions_place_is_not_handed_over() {
    let (_directory, pool, id) = run!(issued());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let performed = application::PrescribedWorkoutId::new(id);

    run!(deliveries.record(
        performed,
        &hevy!(),
        &reference!("routine-6"),
        Timestamp::UNIX_EPOCH,
    ));
    run!(async {
        sqlx::query!(
            "UPDATE gym_workout SET performed_against = 'routine-6' \
             WHERE landing_record_id = (SELECT MIN(landing_record_id) FROM gym_workout)"
        )
        .execute(&pool)
        .await
    });

    let successor = application::PrescribedWorkoutId::new(id + 1_000);
    let outcome = match corpus::block_on(deliveries.hand_over(
        performed,
        successor,
        &hevy!(),
        &reference!("routine-6"),
        Timestamp::UNIX_EPOCH,
    )) {
        Ok(outcome) => outcome,
        Err(error) => panic!("a runtime is available: {error}"),
    };

    match outcome {
        Err(error) => assert!(
            error.to_string().contains("not withdrawable"),
            "the schema refuses it, and says why: {error}"
        ),
        Ok(()) => panic!("a performed session's place was handed over"),
    }

    let still: i64 = run!(async {
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "n!: i64" FROM prescription_delivery
               WHERE prescription = ? AND reference = 'routine-6'"#,
            id
        )
        .fetch_one(&pool)
        .await
    });
    assert_eq!(still, 1, "and the delivery is still where it was");
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
