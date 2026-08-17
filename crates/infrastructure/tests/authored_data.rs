//! Authored data through its real store (§ III).
//!
//! What is asserted here is the half § II never has to think about: this is not
//! an observation and not a derivation, so nothing regenerates it if lost and
//! nothing replaces it wholesale. It is written once, kept, and superseded by
//! date.

mod support;

use application::GenerationParameterStore as _;
use infrastructure::{SqliteGenerationParameterStore, connect};
use sqlx::SqlitePool;
use support::{corpus, programme};

async fn store() -> Result<
    (
        SqliteGenerationParameterStore,
        SqlitePool,
        tempfile::TempDir,
    ),
    Box<dyn std::error::Error>,
> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((
        SqliteGenerationParameterStore::new(pool.clone()),
        pool,
        directory,
    ))
}

macro_rules! opened {
    () => {
        match corpus::block_on(store()) {
            Ok(Ok(opened)) => opened,
            Ok(Err(error)) => panic!("a store opens: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the operation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// Nothing authored is the ordinary first-run state, and it is reported rather
/// than guessed at.
#[test]
fn an_unauthored_store_has_no_parameters() {
    let (store, _pool, _directory) = opened!();
    assert_eq!(run!(store.current()), None);
}

/// Every value survives the round trip exactly.
///
/// The point of holding percentages as basis points and loads as grams: a stored
/// prescription that cannot be reproduced is not a record of anything. A float
/// would pass a `==` on the same machine and fail across a rebuild.
#[test]
fn parameters_round_trip_exactly() {
    let (store, _pool, _directory) = opened!();
    let Ok(authored) = programme::parameters() else {
        panic!("the fixture parameters are valid")
    };
    let now = jiff::Timestamp::now();

    run!(store.author(now, &authored));

    let Some((read_at, read_back)) = run!(store.current()) else {
        panic!("what was authored is in force")
    };
    assert_eq!(read_at, now, "the authoring date round trips");
    assert_eq!(
        read_back, authored,
        "every parameter round trips, value for value"
    );
}

/// Authoring supersedes by date and keeps what came before (§ 12).
///
/// Two things are asserted, and the second is the one worth having: `current`
/// reads the later version, *and* the earlier row is still in the file. An
/// issued prescription names the version it used, so losing a superseded row
/// would make that reference dangle.
#[test]
fn authoring_supersedes_and_retains() {
    let (store, pool, _directory) = opened!();
    let Ok(first) = programme::parameters() else {
        panic!("the fixture parameters are valid")
    };

    // A second version differing in one value, so "which is in force" has an
    // observable answer.
    let Ok(changed) = domain::prescription::Percentage::try_from("80%".to_owned()) else {
        panic!("80% is a percentage")
    };
    let second = domain::prescription::GenerationParameters {
        back_off_of_top_set: changed,
        ..first.clone()
    };

    let earlier = jiff::Timestamp::now();
    let later = earlier
        .checked_add(jiff::Span::new().hours(1))
        .unwrap_or(earlier);

    run!(store.author(earlier, &first));
    run!(store.author(later, &second));

    let Some((in_force_at, in_force)) = run!(store.current()) else {
        panic!("something is in force")
    };
    assert_eq!(in_force_at, later, "the later version is in force");
    assert_eq!(in_force.back_off_of_top_set, changed);

    // And the earlier row survives. Read directly, because no port exposes a
    // superseded version — nothing should consult one, which is exactly why the
    // assertion has to reach past the port.
    let count = run!(async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM generation_parameters")
            .fetch_one(&pool)
            .await
    });
    assert_eq!(count, 2, "the superseded version is kept, not overwritten");
}

/// The store refuses parameters missing a session role.
///
/// `PerRole` is a struct, so a missing role is unrepresentable in Rust — which
/// makes this boundary the only place it can be asserted. A row deleted by hand
/// must be reported as corrupt rather than defaulted.
#[test]
fn parameters_missing_a_role_are_corrupt_not_defaulted() {
    let (store, pool, _directory) = opened!();
    let Ok(authored) = programme::parameters() else {
        panic!("the fixture parameters are valid")
    };
    run!(store.author(jiff::Timestamp::now(), &authored));

    let deleted = corpus::block_on(async {
        sqlx::query("DELETE FROM generation_role_reps WHERE role = 'light'")
            .execute(&pool)
            .await
    });
    assert!(deleted.is_ok(), "the row deletes");

    match corpus::block_on(store.current()) {
        Ok(Err(application::StoreError::Corrupt { .. })) => {}
        Ok(other) => panic!("a missing role must be corrupt, got {other:?}"),
        Err(error) => panic!("a runtime is available: {error}"),
    }
}
