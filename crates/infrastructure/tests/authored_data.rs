//! Authored data through its real store (§ III).
//!
//! What is asserted here is the half § II never has to think about: this is not
//! an observation and not a derivation, so nothing regenerates it if lost and
//! nothing replaces it wholesale. It is written once, kept, and superseded by
//! date.

mod support;

use application::{GenerationParameterStore as _, ProgrammeStore as _};
use infrastructure::{SqliteGenerationParameterStore, SqliteProgrammeStore, connect};
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

// --- The programme ---------------------------------------------------------

async fn programme_store()
-> Result<(SqliteProgrammeStore, SqlitePool, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        pool,
        directory,
    ))
}

macro_rules! programmes {
    () => {
        match corpus::block_on(programme_store()) {
            Ok(Ok(opened)) => opened,
            Ok(Err(error)) => panic!("a store opens: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

#[test]
fn an_unauthored_store_has_no_programme() {
    let (store, _pool, _directory) = programmes!();
    assert_eq!(run!(store.current()), None);
}

/// The eleven fills survive the round trip, in all four shapes.
///
/// This is the assertion the programme store exists for. A slot is single or a
/// superset, and either may alternate by role — four combinations across eleven
/// slots, flattened into one table and grouped back out. Comparing the whole
/// `SlotFills` covers every one of them at once.
#[test]
fn a_programme_round_trips_with_every_fill_shape() {
    let (store, _pool, _directory) = programmes!();
    let Ok(authored) = programme::programme() else {
        panic!("the fixture programme is consistent")
    };

    let id = run!(store.author(&authored));

    let Some((read_id, read_back)) = run!(store.current()) else {
        panic!("what was authored is in force")
    };
    assert_eq!(read_id, id);

    // The single, the alternating single, the same-both-ways superset and the
    // alternating superset, all in one comparison.
    assert_eq!(
        read_back.fills(),
        authored.fills(),
        "every slot fill round trips, in every shape"
    );

    assert_eq!(read_back.primary(), authored.primary());
    assert_eq!(read_back.primary_exercise(), authored.primary_exercise());
    assert_eq!(read_back.gating_role(), authored.gating_role());
    assert_eq!(
        read_back.anchor(),
        authored.anchor(),
        "the anchor round trips"
    );
    assert_eq!(
        read_back.calendar().start(),
        authored.calendar().start(),
        "the block's start round trips"
    );
    assert_eq!(
        read_back.calendar().duration_weeks(),
        authored.calendar().duration_weeks()
    );
}

/// The weeks the block does not run survive the round trip, and still place.
///
/// The store is where this can go wrong quietly: a programme read back without
/// its interruptions is a valid programme that prescribes the wrong week, and
/// nothing about it looks broken. So the assertion is on the placement and not
/// only on the rows.
#[test]
fn the_interrupted_weeks_round_trip() {
    let (store, _pool, _directory) = programmes!();
    let (Ok(away), Ok(after)) = (
        jiff::civil::Date::new(2026, 7, 20),
        jiff::civil::Date::new(2026, 7, 27),
    ) else {
        panic!("the dates are valid")
    };
    let Ok(authored) = programme::programme_skipping(&[away]) else {
        panic!("a week inside the block can be skipped")
    };

    let _id = run!(store.author(&authored));
    let Some((_, read_back)) = run!(store.current()) else {
        panic!("what was authored is in force")
    };

    assert_eq!(
        read_back
            .calendar()
            .interruptions()
            .iter()
            .collect::<Vec<_>>(),
        vec![away],
        "the week the operator named comes back as they named it"
    );
    assert!(
        read_back.calendar().place(away).is_err(),
        "a stored interruption still refuses its own week"
    );
    assert_eq!(
        read_back.calendar().place(after).ok(),
        authored.calendar().place(after).ok(),
        "and the week after it is the same rung it was authored to be"
    );
}

/// The weekday mapping round trips, including which role each day carries.
#[test]
fn the_weekday_mapping_round_trips() {
    let (store, _pool, _directory) = programmes!();
    let Ok(authored) = programme::programme() else {
        panic!("the fixture programme is consistent")
    };
    run!(store.author(&authored));

    let Some((_, read_back)) = run!(store.current()) else {
        panic!("what was authored is in force")
    };

    let mut authored_days: Vec<_> = authored.calendar().weekdays().iter().collect();
    let mut read_days: Vec<_> = read_back.calendar().weekdays().iter().collect();
    authored_days.sort_by_key(|(day, _)| format!("{day:?}"));
    read_days.sort_by_key(|(day, _)| format!("{day:?}"));
    assert_eq!(read_days, authored_days);
}

/// Authoring supersedes by date, and the earlier programme is kept.
#[test]
fn authoring_a_programme_supersedes_and_retains() {
    let (store, pool, _directory) = programmes!();
    let Ok(first) = programme::programme() else {
        panic!("the fixture programme is consistent")
    };

    let first_id = run!(store.author(&first));
    let second_id = run!(store.author(&first));
    assert_ne!(first_id, second_id, "each authoring gets its own identity");

    let count = run!(async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM programme")
            .fetch_one(&pool)
            .await
    });
    assert_eq!(count, 2, "the superseded programme is kept");
}

/// The three inconsistencies the types cannot catch are refused at authoring.
///
/// Refused before the store, so a programme that cannot work never reaches it —
/// which is why these assert on the constructor rather than on `author`.
#[test]
fn the_three_inconsistencies_are_refused() {
    let Ok(gating) = programme::gating_on_a_role_it_never_runs() else {
        panic!("the fixture literals are valid")
    };
    assert!(
        gating.is_err(),
        "a programme gating on a role it never runs would never advance"
    );

    let Ok(measure) = programme::primary_not_counted_in_reps() else {
        panic!("the fixture literals are valid")
    };
    assert!(
        measure.is_err(),
        "a top set is a number of repetitions, so the primary must be counted in them"
    );

    let Ok(slot) = programme::primary_does_not_fill_its_slot() else {
        panic!("the fixture literals are valid")
    };
    assert!(
        slot.is_err(),
        "a programme must not name one exercise as primary and prescribe another"
    );
}

// --- The authored document -------------------------------------------------

/// A document still carrying a `TODO` refuses to author.
///
/// **The fixture no longer has one**, so this puts one back. D8's two unsettled
/// values are gone — the ladder's endpoint stopped existing (D13) and its
/// opening is now derived from the entry test (D14) — but the mechanism has to
/// keep working, because a placeholder that authored successfully would produce
/// a workout indistinguishable from a decided one. Injecting the `TODO` is what
/// keeps this a test of the refusal rather than a test of the fixture.
#[test]
fn an_unsettled_document_refuses_to_author() {
    let Ok(settled) = settled_document() else {
        panic!("the fixture document is readable")
    };
    let unsettled = settled.replace(r#"climb_per_week = "2.5kg""#, r#"climb_per_week = "TODO""#);
    assert_ne!(unsettled, settled, "the fixture carries the key to replace");

    let Ok(document) = toml::from_str::<infrastructure::Document>(&unsettled) else {
        panic!("the amended document is valid TOML")
    };

    match document.parameters() {
        Err(infrastructure::DocumentError::Unsettled { field }) => {
            assert_eq!(field, "parameters.ladder.climb_per_week");
        }
        Ok(_) => panic!("a document with a TODO must not author"),
        Err(other) => panic!("the refusal names the unsettled field, got {other}"),
    }
}

/// The fixture document, which is now settled throughout.
///
/// It carried `TODO`s until 2026-08-19 and this returned it with them filled in.
/// Nothing is left to fill: see `docs/decisions/0008-the-linear-ladder-climbs-at-a-rate.md`
/// and `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`. A free
/// function returning `Result`, because the test exemptions do not reach one.
fn settled_document() -> Result<String, Box<dyn std::error::Error>> {
    let path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/programme.toml"
    ));
    Ok(std::fs::read_to_string(path)?)
}

/// The whole document converts — every fill shape, the anchor, the weekday
/// mapping and the parameters.
#[test]
fn a_settled_document_authors() {
    let Ok(settled) = settled_document() else {
        panic!("the fixture document is readable")
    };

    let Ok(document) = toml::from_str::<infrastructure::Document>(&settled) else {
        panic!("the settled document is valid TOML")
    };
    let Ok(parameters) = document.parameters() else {
        panic!("a settled document's parameters convert")
    };
    let Ok(zone) = jiff::tz::TimeZone::get("Europe/London") else {
        panic!("Europe/London is a zone")
    };
    let programme = match document.programme(&parameters, zone) {
        Ok(programme) => programme,
        Err(error) => panic!("the document describes a consistent programme: {error}"),
    };

    // The fills the document describes, in all four shapes.
    let Ok(expected) = programme::parameters() else {
        panic!("the fixture parameters are valid")
    };
    assert_eq!(
        parameters.back_off_of_top_set, expected.back_off_of_top_set,
        "the document and the Rust fixture agree"
    );
    assert_eq!(parameters.static_hold, expected.static_hold);
    let Ok(expected_fills) = programme::fills() else {
        panic!("the fixture fills are valid")
    };
    assert_eq!(programme.fills(), &expected_fills);
    assert_eq!(programme.calendar().duration_weeks(), 8);
    assert!(
        programme.calendar().interruptions().is_empty(),
        "the fixture block has nothing in its way"
    );
}

/// A document naming a week away authors a block that skips it.
///
/// The key is optional, so this is also the assertion that it is read at all: a
/// document whose `interruptions` went unparsed would author a programme that
/// looks exactly like the one above.
#[test]
fn a_document_can_name_the_weeks_the_block_does_not_run() {
    let Ok(settled) = settled_document() else {
        panic!("the fixture document is readable")
    };
    // Inside the block, which starts 2026-07-06 and runs eight training weeks.
    let named = settled.replace("interruptions = []", r#"interruptions = ["2026-07-20"]"#);
    assert_ne!(named, settled, "the fixture carries the key to replace");

    let Ok(document) = toml::from_str::<infrastructure::Document>(&named) else {
        panic!("the amended document is valid TOML")
    };
    let (Ok(parameters), Ok(zone)) = (
        document.parameters(),
        jiff::tz::TimeZone::get("Europe/London"),
    ) else {
        panic!("the parameters convert and Europe/London is a zone")
    };
    let programme = match document.programme(&parameters, zone) {
        Ok(programme) => programme,
        Err(error) => panic!("the document describes a consistent programme: {error}"),
    };

    let (Ok(away), Ok(after)) = (
        jiff::civil::Date::new(2026, 7, 20),
        jiff::civil::Date::new(2026, 7, 27),
    ) else {
        panic!("the dates are valid")
    };
    assert_eq!(
        programme
            .calendar()
            .interruptions()
            .iter()
            .collect::<Vec<_>>(),
        vec![away]
    );
    assert_eq!(
        programme.calendar().duration_weeks(),
        8,
        "the duration counts training weeks, so a holiday does not shorten it"
    );
    assert_eq!(programme.calendar().calendar_weeks(), 9);
    assert!(programme.calendar().place(away).is_err());
    match programme.calendar().place(after) {
        Ok((domain::prescription::WeekKind::Climbing(week), _)) => {
            assert_eq!(week.as_u32(), 3, "the week after the holiday is week three");
        }
        other => panic!("2026-07-27 is a climbing week, got {other:?}"),
    }
}

/// A week outside the block is refused rather than ignored.
#[test]
fn a_document_naming_a_week_outside_the_block_does_not_author() {
    let Ok(settled) = settled_document() else {
        panic!("the fixture document is readable")
    };
    // The block starts 2026-07-06; this is the week before it.
    let named = settled.replace("interruptions = []", r#"interruptions = ["2026-06-29"]"#);

    let Ok(document) = toml::from_str::<infrastructure::Document>(&named) else {
        panic!("the amended document is valid TOML")
    };
    let Ok(parameters) = document.parameters() else {
        panic!("the parameters convert")
    };
    let Ok(zone) = jiff::tz::TimeZone::get("Europe/London") else {
        panic!("Europe/London is a zone")
    };
    match document.programme(&parameters, zone) {
        Err(infrastructure::DocumentError::Uncalendarable(error)) => {
            assert!(
                matches!(
                    error,
                    domain::prescription::InvalidCalendar::InterruptionBeforeStart { .. }
                ),
                "the refusal says the week is before the block, got {error}"
            );
        }
        Ok(_) => panic!("a week outside the block must not author"),
        Err(other) => panic!("the refusal names the calendar, got {other}"),
    }
}
