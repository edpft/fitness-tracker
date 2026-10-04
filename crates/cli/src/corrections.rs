//! `fitness gym corrections` — what the operator says a source got wrong.
//!
//! The edit overlay's porcelain (§ II.2). Two things can be wrong, and the flag
//! he reaches for names which.
//!
//! **`--was` corrects the exercise a source named.** A source with no entry for
//! what was performed gets the nearest thing it offers, and nothing in the
//! payload separates that stand-in from the exercise it names: Hevy had no
//! neutral grip pull up, so `Pull Up` carried them for eighteen months, and
//! `Stretching` stood in for a squatting groin stretch for two. Only the
//! operator can say which is which.
//!
//! **He names both exercises in our vocabulary, not the source's.** `--was
//! pull-up` reaches all three of Hevy's pull-up templates, because which
//! templates read as a pull-up is a fact about the source and not something he
//! should have to know. What gets *stored* is the source's own identity for each
//! entry, which is what § II.2 requires and what survives a rebuild.
//!
//! **`--recorded` corrects the figures a source recorded for a set.** 2018-06-11
//! holds a deadlift set the watch states as 58 repetitions at 6 kg. The
//! operator, 2026-10-03: *"I think the 58 x 6kg is a transposition error, I
//! think it's actually 6x 57.5kg, entered incorrectly in the watch."* No merge
//! rule reaches that — the canonical layer is reporting the source faithfully
//! and the source is wrong — so he says the number himself, against the day he
//! trained and the figures that are wrong.
//!
//! **A day, not a span.** A habit spans months and is one assertion; a number
//! typed wrongly is one session, and letting it reach a range would quietly
//! correct every set in the record that happened to share those figures.
//!
//! **It reports before it records.** An assertion over 223 sets is worth seeing
//! the shape of first, and an assertion over none is a mistake the operator
//! wants told rather than filed.
//!
//! **Retraction is by assertion, not by set.** He asserted it in one action and
//! he undoes it in one; `remove` takes the number `show` prints, and both kinds
//! of correction share one numbering for that reason.

use std::path::Path;

use domain::{
    gym::exercise::Exercise,
    measure::{InvalidMass, InvalidQuantity, Kg, RepCount},
    normalised::{Corrected, CorrectionId, CorrectionReason, OperatorZone, SetFigures},
    sequence::NonEmpty,
};
use infrastructure::{
    GarminExerciseSetLandingStore, GarminRecordedSets, HevyStandIns, HevyWorkoutLandingStore,
    SqliteEditOverlayStore, SqlitePool, connect, garmin::FoundSet, hevy::StandIn,
};
use jiff::{Timestamp, civil::Date};

use crate::{Failure, exit};

/// The streams a correction can be asserted against.
///
/// Not [`crate::catalogue::KNOWN`]: `garmin.exercise_sets` is not a stream the
/// operator collects — it is fetched per activity behind `garmin.activities`
/// (#172) — and it is the one whose records a figures correction anchors in. A
/// stream nobody has corrected answers with nothing, so reading all of them
/// costs one query each and keeps the numbering one space.
const CORRECTED: [&str; 2] = [
    HevyWorkoutLandingStore::STREAM,
    GarminExerciseSetLandingStore::STREAM,
];

/// What `add` was asked to assert about an exercise a source named.
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

/// What `add` was asked to assert about a set's figures.
pub struct FigureAssertion<'a> {
    /// The day the set was performed.
    pub on: &'a str,
    /// The figures the source recorded, as `<reps>x<kg>`.
    pub recorded: &'a str,
    /// The figures the operator says it was, in the same form.
    pub actually: &'a str,
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
    let reason = correction_reason(assertion.because)?;
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

/// Assert that what a source recorded for a set was something else.
///
/// # Errors
///
/// [`Failure`] if either set of figures or the day will not parse, no set
/// matches, or the store cannot be reached.
pub async fn add_figures(
    database: &Path,
    zone: &OperatorZone,
    assertion: &FigureAssertion<'_>,
) -> Result<(), Failure> {
    let recorded = figures(assertion.recorded, "--recorded")?;
    let actually = figures(assertion.actually, "--actually")?;
    if recorded == actually {
        return Err(Failure::message(
            format!("--recorded and --actually are both {recorded}, which asserts nothing"),
            exit::USAGE,
        ));
    }
    let reason = correction_reason(assertion.because)?;
    let on = day(Some(assertion.on), "--on")?.ok_or_else(|| {
        Failure::message(
            "no --on given, and a figure is corrected on one day",
            exit::USAGE,
        )
    })?;

    let pool = pool(database).await?;
    let found = GarminRecordedSets::new(pool.clone())
        .recording(recorded, zone, on)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let Ok(sets) = NonEmpty::new(found.iter().map(|found| found.set.clone()).collect()) else {
        return Err(Failure::message(
            format!("no set on {on} was recorded as {recorded}"),
            exit::USAGE,
        ));
    };

    report_figures(&found, on, recorded, actually);

    let id = SqliteEditOverlayStore::new(pool, GarminExerciseSetLandingStore::STREAM)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .assert_figures(recorded, actually, Timestamp::now(), &reason, &sets)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    println!();
    println!(
        "Recorded as correction {id}, over {}. It applies at the next derivation.",
        counted(sets.count())
    );
    Ok(())
}

/// Read back every correction in force.
///
/// Both streams, because the operator asked what he has corrected and not which
/// source he corrected it on.
///
/// # Errors
///
/// [`Failure`] if the store cannot be reached or holds a key the vocabulary no
/// longer has.
pub async fn show(database: &Path) -> Result<(), Failure> {
    let pool = pool(database).await?;
    let mut corrections = Vec::new();
    for stream in CORRECTED {
        corrections.extend(
            SqliteEditOverlayStore::new(pool.clone(), stream)
                .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
                .all()
                .await
                .map_err(|error| Failure::message(error.to_string(), exit::STORE))?,
        );
    }
    corrections.sort_by_key(|correction| (correction.asserted_at(), correction.id()));

    if corrections.is_empty() {
        println!("No corrections. The normalised layer is exactly what the sources said.");
        return Ok(());
    }

    for correction in &corrections {
        let reaches = correction.reaches();
        let what = match correction.corrected() {
            Corrected::Exercise { exercise, .. } => {
                format!("{reaches} entries read as {}", exercise.as_str())
            }
            Corrected::Figures {
                recorded,
                figures: reads,
                ..
            } => format!(
                "{} recorded as {recorded} reads as {reads}",
                counted(reaches)
            ),
        };
        println!(
            "{} — {what}, asserted {}",
            correction.id(),
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

    let pool = pool(database).await?;
    // One numbering across every stream, so the number `show` printed is the
    // one that is retracted whichever source it was asserted against.
    for stream in CORRECTED {
        let retracted = SqliteEditOverlayStore::new(pool.clone(), stream)
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
            .retract(CorrectionId::from(id))
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
        if retracted {
            println!("Retracted correction {id}. The next derivation reads what the source said.");
            return Ok(());
        }
    }

    Err(Failure::message(
        format!("there is no correction {id}"),
        exit::USAGE,
    ))
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

/// What a figures assertion reaches, before it is recorded.
///
/// The source's own term for each set, so correcting a set he did not mean to
/// shows up before the assertion is filed: a set recorded as 58 × 6 kg under
/// `DEADLIFT` is the one he is after, and one under `BENCH_PRESS` is not.
fn report_figures(found: &[FoundSet], on: Date, recorded: SetFigures, actually: SetFigures) {
    println!("Recorded as {recorded} on {on}:");
    for set in found {
        let movement = set.movement.as_deref().unwrap_or("unclassified");
        println!(
            "  {:02}:{:02}:{:02}  {movement}",
            set.began.hour(),
            set.began.minute(),
            set.began.second()
        );
    }
    println!("Reading as {actually} instead.");
}

/// A count of sets, said once so neither message reads "1 sets".
///
/// A figures correction reaches one set about as often as an exercise
/// correction reaches two hundred, so the singular is the ordinary case here
/// rather than an edge of it.
fn counted(sets: usize) -> String {
    if sets == 1 {
        "1 set".to_owned()
    } else {
        format!("{sets} sets")
    }
}

fn exercise(key: &str) -> Result<Exercise, Failure> {
    Exercise::named(key).ok_or_else(|| {
        Failure::message(
            format!("{key} is not an exercise in this vocabulary"),
            exit::USAGE,
        )
    })
}

fn correction_reason(because: &str) -> Result<CorrectionReason, Failure> {
    CorrectionReason::try_from(because.to_owned())
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))
}

/// A set's figures, as the operator writes them: `58x6`, or `10` for a set at
/// body weight.
///
/// `x` because that is how the record reads them out loud and how the sheets are
/// laid out. The mass goes through [`Kg`]'s own parser, so `57.5` is exact.
fn figures(given: &str, flag: &str) -> Result<SetFigures, Failure> {
    let unreadable = |detail: String| {
        Failure::message(
            format!("{flag} {given} is not a set's figures: {detail}"),
            exit::USAGE,
        )
    };
    let (reps, load) = match given.split_once(['x', 'X', '×']) {
        Some((reps, load)) => (reps, Some(load.trim_end_matches("kg").trim())),
        None => (given, None),
    };
    let reps = RepCount::try_from(reps.trim().to_owned())
        .map_err(|error: InvalidQuantity| unreadable(error.to_string()))?;
    let load = load
        .map(|load| {
            Kg::try_from(load.to_owned())
                .map_err(|error: InvalidMass| unreadable(error.to_string()))
        })
        .transpose()?;
    Ok(SetFigures::new(reps, load))
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
