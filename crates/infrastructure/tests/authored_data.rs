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
        light_of_heavy: changed,
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
    assert_eq!(in_force.light_of_heavy, changed);

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

/// A day the fixture block covers.
///
/// The store is read by date now that programmes succeed one another, so every
/// round-trip test has to name one. The fixture opens on 2026-07-06 and runs
/// eight weeks, so its first Monday does.
const fn inside_the_block() -> jiff::civil::Date {
    jiff::civil::Date::constant(2026, 7, 6)
}

#[test]
fn an_unauthored_store_has_no_programme() {
    let (store, _pool, _directory) = programmes!();
    assert_eq!(run!(store.on(inside_the_block())), None);
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

    let id = run!(store.author(&programme::as_programme(authored.clone())));

    let Some((read_id, read_back)) = run!(store.on(inside_the_block())) else {
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
    assert_eq!(read_back.gating_role(), Some(authored.gating_role()));
    assert_eq!(
        read_back.anchor(),
        Some(authored.anchor()),
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
    let Some(seven) = std::num::NonZeroU8::new(7) else {
        panic!("seven is not zero")
    };
    let week = domain::prescription::Skip::new(away, seven);
    let Ok(authored) = programme::programme_skipping(&[week]) else {
        panic!("a week inside the block can be skipped")
    };

    let _id = run!(store.author(&programme::as_programme(authored.clone())));
    let Some((_, read_back)) = run!(store.on(inside_the_block())) else {
        panic!("what was authored is in force")
    };

    assert_eq!(
        read_back
            .calendar()
            .interruptions()
            .iter()
            .collect::<Vec<_>>(),
        vec![week],
        "the skip the operator named comes back as they named it, days and all"
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
    run!(store.author(&programme::as_programme(authored.clone())));

    let Some((_, read_back)) = run!(store.on(inside_the_block())) else {
        panic!("what was authored is in force")
    };

    let mut authored_days: Vec<_> = authored.calendar().weekdays().iter().collect();
    let mut read_days: Vec<_> = read_back.calendar().weekdays().iter().collect();
    authored_days.sort_by_key(|(day, _)| format!("{day:?}"));
    read_days.sort_by_key(|(day, _)| format!("{day:?}"));
    assert_eq!(read_days, authored_days);
}

/// Authoring supersedes by date, and the earlier programme is kept.
///
/// **Two constructions rather than one authored twice.** A `Linear` stamps
/// its own `authored_at`, so re-authoring the same value would be one version
/// claiming two rows — which `UNIQUE (name, authored_at)` refuses. Building the
/// fixture again is what re-authoring a document actually does.
#[test]
fn authoring_a_programme_supersedes_and_retains() {
    let (store, pool, _directory) = programmes!();
    let (Ok(first), Ok(again)) = (programme::programme(), programme::programme()) else {
        panic!("the fixture programme is consistent")
    };

    let first_id = run!(store.author(&programme::as_programme(first)));
    let second_id = run!(store.author(&programme::as_programme(again)));
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

/// A block authored with days away skips them, and reaches a week further.
///
/// **The duration counts training weeks**, so a holiday does not shorten a
/// block — it pushes its last week later. A week that loses both its sessions
/// survives as no training week at all, which is the case this asserts: the
/// Monday is named outright and the Friday falls inside a three-day run.
#[test]
fn a_block_authored_with_days_away_skips_them() {
    let (Ok(monday), Ok(friday), Ok(after)) = (
        jiff::civil::Date::new(2026, 7, 20),
        jiff::civil::Date::new(2026, 7, 24),
        jiff::civil::Date::new(2026, 7, 27),
    ) else {
        panic!("the dates are valid")
    };
    let Some(three) = std::num::NonZeroU8::new(3) else {
        panic!("three is not zero")
    };
    let skips = [
        domain::prescription::Skip::day(monday),
        domain::prescription::Skip::new(friday, three),
    ];

    let programme = match fixture_block().and_then(|answers| programme::authoring(answers, &skips))
    {
        Ok(Ok(programme)) => programme,
        Ok(Err(error)) => panic!("the block is consistent: {error}"),
        Err(error) => panic!("the fixture builds: {error}"),
    };

    assert_eq!(
        programme
            .calendar()
            .interruptions()
            .iter()
            .collect::<Vec<_>>(),
        skips.to_vec(),
        "both skips are the block's"
    );
    assert_eq!(
        programme.calendar().duration_weeks(),
        8,
        "the duration counts training weeks, so a holiday does not shorten it"
    );
    // The week of the 20th loses both its sessions, so nothing survives in it
    // and the block reaches one calendar week further.
    assert_eq!(programme.calendar().calendar_weeks(), 9);
    assert!(programme.calendar().place(monday).is_err());
    assert!(programme.calendar().place(friday).is_err());
    match programme.calendar().place(after) {
        Ok((domain::prescription::WeekKind::Climbing(week), _)) => {
            assert_eq!(week.as_u32(), 3, "the week after the holiday is week three");
        }
        other => panic!("2026-07-27 is a climbing week, got {other:?}"),
    }
}

/// The fixture block, as a set of answers: eight weeks from 2026-07-06.
///
/// A free function returning `Result`, because the test exemptions do not reach
/// one.
fn fixture_block() -> Result<domain::prescription::Authored, programme::ProgrammeFixtureError> {
    programme::authored(
        "summer-2026-front-squat",
        jiff::civil::Date::constant(2026, 7, 6),
        domain::prescription::authored::Shape::Linear {
            gating: domain::prescription::SessionRole::Heavy,
            weeks: 8,
            anchor: programme::anchor()?,
            // Derived from the anchor, as the fixtures do it.
            opening: None,
        },
    )
}

/// A week outside the block is refused rather than ignored.
///
/// An interruption before the start means the operator and the programme
/// disagree about when the block runs, and that disagreement is worth more than
/// the holiday.
#[test]
fn a_week_outside_the_block_does_not_author() {
    // The block starts 2026-07-06; this is the week before it.
    let before = [domain::prescription::Skip::day(
        jiff::civil::Date::constant(2026, 6, 29),
    )];

    match fixture_block().and_then(|answers| programme::authoring(answers, &before)) {
        Ok(Err(domain::prescription::AuthoringError::Calendar(error))) => {
            assert!(
                matches!(
                    error,
                    domain::prescription::InvalidCalendar::InterruptionBeforeStart { .. }
                ),
                "the refusal says the week is before the block, got {error}"
            );
        }
        Ok(Ok(_)) => panic!("a week outside the block must not author"),
        Ok(Err(other)) => panic!("the refusal names the calendar, got {other}"),
        Err(error) => panic!("the fixture builds: {error}"),
    }
}

/// Two programmes over different weeks are both real, and the date decides.
///
/// **What succession is for** (decision 0012). Before this, the store answered
/// with whatever was authored last, so the second programme would have taken
/// over the first one's dates as well as its own.
#[test]
fn two_programmes_succeed_one_another_and_the_date_chooses() {
    let (store, _pool, _directory) = programmes!();
    let (Ok(summer), Ok(autumn)) = (
        programme::programme_named_from("summer", jiff::civil::Date::constant(2026, 7, 6)),
        // Eight weeks from 6 July ends on 31 August, so the autumn block opens
        // the Monday after and the two are adjacent rather than overlapping.
        programme::programme_named_from("autumn", jiff::civil::Date::constant(2026, 8, 31)),
    ) else {
        panic!("the fixtures are consistent")
    };

    let summer_id = run!(store.author(&programme::as_programme(summer)));
    let autumn_id = run!(store.author(&programme::as_programme(autumn)));

    let in_summer = run!(store.on(jiff::civil::Date::constant(2026, 7, 20)));
    let in_autumn = run!(store.on(jiff::civil::Date::constant(2026, 9, 14)));
    let (Some((first, _)), Some((second, _))) = (in_summer, in_autumn) else {
        panic!("both programmes answer for their own weeks")
    };

    assert_eq!(first, summer_id, "July belongs to the summer block");
    assert_eq!(
        second, autumn_id,
        "September belongs to the autumn block, which was authored last"
    );
}

/// A day no programme covers is nothing, not the nearest programme.
#[test]
fn a_day_between_programmes_belongs_to_neither() {
    let (store, _pool, _directory) = programmes!();
    let Ok(summer) =
        programme::programme_named_from("summer", jiff::civil::Date::constant(2026, 7, 6))
    else {
        panic!("the fixture is consistent")
    };
    run!(store.author(&programme::as_programme(summer)));

    // Eight weeks from 6 July is over on 31 August.
    assert_eq!(
        run!(store.on(jiff::civil::Date::constant(2026, 9, 7))),
        None,
        "a date past the block belongs to no programme"
    );
    assert_eq!(
        run!(store.on(jiff::civil::Date::constant(2026, 6, 29))),
        None,
        "and so does one before it"
    );
}

/// Every programme's window comes back, and versions of one collapse to one.
#[test]
fn windows_report_one_entry_per_programme() {
    let (store, _pool, _directory) = programmes!();
    let (Ok(summer), Ok(again), Ok(autumn)) = (
        programme::programme_named_from("summer", jiff::civil::Date::constant(2026, 7, 6)),
        programme::programme_named_from("summer", jiff::civil::Date::constant(2026, 7, 6)),
        programme::programme_named_from("autumn", jiff::civil::Date::constant(2026, 8, 31)),
    ) else {
        panic!("the fixtures are consistent")
    };
    run!(store.author(&programme::as_programme(summer)));
    run!(store.author(&programme::as_programme(again)));
    run!(store.author(&programme::as_programme(autumn)));

    let windows = run!(store.windows());
    let names: Vec<String> = windows
        .iter()
        .map(|window| window.name().to_string())
        .collect();
    assert_eq!(
        names,
        vec!["summer".to_owned(), "autumn".to_owned()],
        "three authorings of two programmes are two windows, oldest block first"
    );
}
