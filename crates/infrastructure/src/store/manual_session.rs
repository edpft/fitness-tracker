//! Writing a gym session the operator logged himself, whichever log it came
//! from: the spreadsheets (#274) or Beyond The White Board (#285). Both go in
//! the source-independent gym tables beside Hevy's, told apart by `stream`.

use application::StoreError;
use domain::gym::{ManualExercise, ManualGymSession, ManualItem};
use sqlx::{Sqlite, Transaction};

use super::{
    count_for_storage,
    normalised::{Row, write_set},
    store_error,
};

/// One session: the session row, the one workout row that carries its day,
/// and its items.
///
/// **One workout per session.** A workout is a part of a session as one source
/// recorded it, and a log records a session in one place, so there is only
/// ever one part. It names the file the session is dated by; each set names the
/// file it was taken from.
pub(super) async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    session: &ManualGymSession,
) -> Result<(), StoreError> {
    let logged = session.logged();
    let landed_as = logged.landed_as.as_i64();
    let source_record_id = logged.source_record_id.as_str();
    let on_day = session.on().to_string();

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_session (stream, landing_record_id, run_id) VALUES (?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        stream,
        landed_as,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let session_id = row.id;

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_workout (
            session, stream, landing_record_id, source_record_id, on_day, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        session_id,
        stream,
        landed_as,
        source_record_id,
        on_day,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let workout = row.id;

    for (position, item) in session.items().iter().enumerate() {
        let position = count_for_storage(position)?;
        let is_superset = i64::from(matches!(item, ManualItem::Superset(_)));
        sqlx::query!(
            "INSERT INTO workout_item (workout, position, is_superset) VALUES (?, ?, ?)",
            workout,
            position,
            is_superset
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        for (member, exercise) in item.exercises().enumerate() {
            let member = count_for_storage(member)?;
            write_exercise(tx, workout, position, member, exercise).await?;
        }
    }
    Ok(())
}

async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    item_position: i64,
    position: i64,
    exercise: &ManualExercise,
) -> Result<(), StoreError> {
    let key = exercise.exercise_key();
    let measure = exercise.measure();

    sqlx::query!(
        r#"
        INSERT INTO performed_exercise (workout, item_position, position, exercise, measure)
        VALUES (?, ?, ?, ?, ?)
        "#,
        workout,
        item_position,
        position,
        key,
        measure
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    match exercise {
        ManualExercise::ForReps { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let reps = set
                    .outcome
                    .completed()
                    .and_then(Option::as_ref)
                    .map(|reps| i64::from(reps.as_u32()));
                write_set(
                    tx,
                    Row::manual(workout, item_position, position, ordinal, set),
                )
                .reps(reps)
                .execute()
                .await?;
            }
        }
        ManualExercise::ForDuration { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let seconds = set
                    .outcome
                    .completed()
                    .and_then(Option::as_ref)
                    .map(|duration| i64::try_from(duration.as_seconds()))
                    .transpose()
                    .map_err(|_| StoreError::Corrupt {
                        detail: "a duration larger than the store can hold".to_owned(),
                    })?;
                write_set(
                    tx,
                    Row::manual(workout, item_position, position, ordinal, set),
                )
                .duration(seconds)
                .execute()
                .await?;
            }
        }
        ManualExercise::ForDistance { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let millimetres = set
                    .outcome
                    .completed()
                    .and_then(Option::as_ref)
                    .map(|metres| i64::try_from(metres.as_millimetres()))
                    .transpose()
                    .map_err(|_| StoreError::Corrupt {
                        detail: "a distance larger than the store can hold".to_owned(),
                    })?;
                write_set(
                    tx,
                    Row::manual(workout, item_position, position, ordinal, set),
                )
                .distance(millimetres)
                .execute()
                .await?;
            }
        }
    }
    Ok(())
}
