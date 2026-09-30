//! `fitness gym corrections` — what the operator says a source got wrong.
//!
//! The edit overlay's porcelain (§ II.2). A source with no entry for what was
//! performed gets the nearest thing it offers, and nothing in the payload
//! separates that stand-in from the exercise it names: Hevy had no neutral grip
//! pull up, so `Pull Up` carried them for eighteen months, and `Stretching`
//! stood in for a squatting groin stretch for two. Only the operator can say
//! which is which, and this is where he says it.
//!
//! **He names both exercises in our vocabulary, not the source's.** `--was
//! pull-up` reaches all three of Hevy's pull-up templates, because which
//! templates read as a pull-up is a fact about the source and not something he
//! should have to know. What gets *stored* is the source's own identity for each
//! entry, which is what § II.2 requires and what survives a rebuild.
//!
//! **It reports before it records.** An assertion over 223 sets is worth seeing
//! the shape of first, and an assertion over none is a mistake the operator
//! wants told rather than filed.
//!
//! **Retraction is by assertion, not by set.** He asserted it in one action and
//! he undoes it in one; `remove` takes the number `show` prints.

use std::path::Path;

use domain::{
    gym::exercise::Exercise,
    normalised::{CorrectionId, CorrectionReason, OperatorZone},
    sequence::NonEmpty,
};
use infrastructure::{
    HevyStandIns, HevyWorkoutLandingStore, SqliteEditOverlayStore, SqlitePool, connect,
    hevy::StandIn,
};
use jiff::{Timestamp, civil::Date};

use crate::{Failure, exit};

/// What `add` was asked to assert.
pub struct Assertion<'a> {
    /// The exercise the records currently read as.
    pub was: &'a str,
    /// The exercise the operator says they were.
    pub actually: &'a str,
    /// The first day it reaches, inclusive. Absent means from the beginning.
    pub from: Option<&'a str>,
    /// The last day it reaches, inclusive. Absent means up to now.
    pub until: Option<&'a str>,
    /// Why, which § II.2 requires.
    pub because: &'a str,
}

/// Assert that what a source named was something else.
///
/// # Errors
///
/// [`Failure`] if either exercise is not in the vocabulary, a date will not
/// parse, no entry matches, or the store cannot be reached.
pub async fn add(
    database: &Path,
    zone: &OperatorZone,
    assertion: &Assertion<'_>,
) -> Result<(), Failure> {
    let was = exercise(assertion.was)?;
    let actually = exercise(assertion.actually)?;
    if was == actually {
        return Err(Failure::message(
            format!(
                "--was and --actually are both {}, which asserts nothing",
                assertion.actually
            ),
            exit::USAGE,
        ));
    }
    let reason = CorrectionReason::try_from(assertion.because.to_owned())
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;
    let from = day(assertion.from, "--from")?;
    let until = day(assertion.until, "--until")?;
    if let (Some(first), Some(last)) = (from, until)
        && first > last
    {
        return Err(Failure::message(
            format!("--from {first} is after --until {last}"),
            exit::USAGE,
        ));
    }

    let pool = pool(database).await?;
    let found = HevyStandIns::new(pool.clone())
        .reading_as(was, zone, from, until)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let Ok(terms) = NonEmpty::new(found.iter().map(|found| found.term.clone()).collect()) else {
        // Nothing matched. Told rather than filed: a correction reaching no
        // observation is a mistake in the dates or the exercise, and storing it
        // would leave the operator believing his record had been corrected.
        return Err(Failure::message(
            format!(
                "nothing {} reads as {} {}",
                assertion.was,
                assertion.was,
                within(from, until)
            ),
            exit::USAGE,
        ));
    };

    report(&found, assertion.was, assertion.actually);

    let id = SqliteEditOverlayStore::new(pool, HevyWorkoutLandingStore::STREAM)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .assert(actually, Timestamp::now(), &reason, &terms)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    println!();
    println!(
        "Recorded as correction {id}, over {} entries. It applies at the next derivation.",
        terms.count()
    );
    Ok(())
}

/// Read back every correction in force.
///
/// # Errors
///
/// [`Failure`] if the store cannot be reached or holds a key the vocabulary no
/// longer has.
pub async fn show(database: &Path) -> Result<(), Failure> {
    let corrections =
        SqliteEditOverlayStore::new(pool(database).await?, HevyWorkoutLandingStore::STREAM)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
            .all()
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    if corrections.is_empty() {
        println!("No corrections. The normalised layer is exactly what the sources said.");
        return Ok(());
    }

    for correction in &corrections {
        println!(
            "{} — {} entries read as {}, asserted {}",
            correction.id(),
            correction.terms().count(),
            correction.exercise().as_str(),
            correction.asserted_at().strftime("%Y-%m-%d"),
        );
        println!("     {}", correction.reason());
    }
    Ok(())
}

/// Retract one assertion, by the number `show` prints.
///
/// # Errors
///
/// [`Failure`] if there is no such correction, or the store cannot be written.
pub async fn remove(database: &Path, id: &str) -> Result<(), Failure> {
    let id: i64 = id
        .parse()
        .map_err(|_| Failure::message(format!("{id} is not a correction number"), exit::USAGE))?;

    let retracted =
        SqliteEditOverlayStore::new(pool(database).await?, HevyWorkoutLandingStore::STREAM)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
            .retract(CorrectionId::from(id))
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    if retracted {
        println!("Retracted correction {id}. The next derivation reads what the source said.");
        Ok(())
    } else {
        Err(Failure::message(
            format!("there is no correction {id}"),
            exit::USAGE,
        ))
    }
}

/// What an assertion reaches, before it is recorded.
///
/// Days and sets, because those are the two figures that let the operator
/// recognise his own training: 223 sets over 69 sessions from 2025-02-28 is a
/// sentence he can check against what he remembers, where a list of 223
/// identifiers is not.
fn report(found: &[StandIn], was: &str, actually: &str) {
    let sets: usize = found.iter().map(|found| found.sets).sum();
    let days: std::collections::BTreeSet<Date> = found.iter().map(|found| found.day).collect();

    println!("{sets} sets over {} sessions read as {was}.", days.len());
    if let (Some(first), Some(last)) = (days.iter().next(), days.iter().next_back()) {
        println!("From {first} to {last}.");
    }

    // The source's own titles, so an unexpected one shows up before the
    // assertion is filed rather than after: `pull-up` reaching `Pull Up (Band)`
    // is right, and it reaching something he does not recognise is not.
    let mut titles: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for entry in found {
        *titles.entry(entry.title.as_str()).or_default() += entry.sets;
    }
    for (title, sets) in titles {
        let title = if title.is_empty() {
            "(untitled)"
        } else {
            title
        };
        println!("  {sets:>4} sets logged as {title}");
    }
    println!("All of them will read as {actually}.");
}

fn exercise(key: &str) -> Result<Exercise, Failure> {
    Exercise::named(key).ok_or_else(|| {
        Failure::message(
            format!("{key} is not an exercise in this vocabulary"),
            exit::USAGE,
        )
    })
}

fn day(given: Option<&str>, flag: &str) -> Result<Option<Date>, Failure> {
    given
        .map(|text| {
            text.parse::<Date>().map_err(|error| {
                Failure::message(format!("{flag} {text} is not a date: {error}"), exit::USAGE)
            })
        })
        .transpose()
}

fn within(from: Option<Date>, until: Option<Date>) -> String {
    match (from, until) {
        (Some(first), Some(last)) => format!("between {first} and {last}"),
        (Some(first), None) => format!("on or after {first}"),
        (None, Some(last)) => format!("on or before {last}"),
        (None, None) => "in the record".to_owned(),
    }
}

async fn pool(database: &Path) -> Result<SqlitePool, Failure> {
    connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))
}
