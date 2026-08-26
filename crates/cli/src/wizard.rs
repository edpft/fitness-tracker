//! `fitness programme add`, asked rather than typed.
//!
//! **Seventeen slots out of a hundred and thirty-five exercises.** A fresh
//! periodisation states every slot itself — there is no predecessor to inherit
//! from — and typing that by hand is the pain this exists to remove.
//!
//! ## It writes a document and then authors it
//!
//! Rather than authoring directly, which would be shorter and worse. A
//! programme is seventeen slots and a set of parameters, and it is worth
//! leaving behind something reviewable, diffable and re-authorable; the
//! schedule, which is four slots and a zone, is not and goes straight to the
//! store. It also keeps one authoring path, so the interruptions a document
//! derives from the diary are derived the same way whoever wrote it.
//!
//! ## Ordered by the record, limited by nothing
//!
//! Each slot offers what `candidates` holds for it, sorted by how much of it
//! the record actually contains. The operator asked to see options he has not
//! done before, so the list is an offer and not a menu: any key in the
//! vocabulary is accepted, and the numbers are a shortcut rather than a fence.

use std::{
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
};

use application::{ExerciseHistory as _, GenerationParameterStore as _};
use domain::{
    gym::{
        OperatorZone,
        exercise::{DistanceExercise, DurationExercise, Exercise, RepsExercise},
    },
    prescription::{Block, SlotId},
};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, connect,
    programme::draft::{Draft, FillLine, render},
};
use jiff::civil::Date;

use crate::{Failure, candidates, exit};

/// Our vocabulary, from a key. `None` if the key names nothing.
pub fn exercise_named(key: &str) -> Option<Exercise> {
    if let Ok(reps) = RepsExercise::try_from(key.to_owned()) {
        return Some(Exercise::Reps(reps));
    }
    if let Ok(duration) = DurationExercise::try_from(key.to_owned()) {
        return Some(Exercise::Duration(duration));
    }
    DistanceExercise::try_from(key.to_owned())
        .ok()
        .map(Exercise::Distance)
}

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure::message(message.to_string(), exit::USAGE)
}

fn interactive() -> Result<(), Failure> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(usage(
        "this asks questions and there is nobody to ask: pass a document, or run \
         it from a terminal. Nothing was written",
    ))
}

fn ask(question: &str) -> Result<String, Failure> {
    print!("{question}");
    std::io::stdout().flush().map_err(usage)?;
    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed).map_err(usage)?;
    Ok(typed.trim().to_owned())
}

/// Repeat a question until the answer parses.
///
/// Thirty answers in, unwinding on a typo would throw away the twenty-nine
/// before it. So a bad answer is re-asked and nothing else is lost.
fn ask_until<T>(question: &str, parse: impl Fn(&str) -> Result<T, String>) -> Result<T, Failure> {
    loop {
        let typed = ask(question)?;
        match parse(&typed) {
            Ok(value) => return Ok(value),
            Err(complaint) => println!("  {complaint}"),
        }
    }
}

fn parse_date(typed: &str) -> Result<Date, String> {
    typed
        .parse()
        .map_err(|_| format!("{typed:?} is not a date — try 2026-09-14"))
}

fn parse_count(what: &str, typed: &str) -> Result<u32, String> {
    typed
        .parse::<u32>()
        .ok()
        .filter(|count| *count > 0)
        .ok_or_else(|| format!("{typed:?} is not a number of {what}"))
}

/// A slot's candidates, most-performed first.
///
/// **The record decides the order and the operator decides the list.** Sorting
/// by what has been performed puts the answer he usually gives at the top; the
/// list itself is his, because which exercises belong in a slot is preference
/// rather than a fact about anybody.
async fn offered(
    history: &SqliteExerciseHistory,
    slot: SlotId,
) -> Result<Vec<(String, Option<usize>)>, Failure> {
    let mut offers = Vec::new();
    for key in candidates::for_slot(slot) {
        // **Only repetitions have a count to show.** `ExerciseHistory` answers
        // for exercises counted in reps, because that is what progression
        // needs — so a hold has no number here. Printing "never performed"
        // beside a couch stretch done every session would be worse than
        // printing nothing, which is what `None` renders as.
        let performed = match exercise_named(key) {
            Some(Exercise::Reps(exercise)) => Some(
                history
                    .performances(exercise)
                    .await
                    .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
                    .len(),
            ),
            _ => None,
        };
        offers.push(((*key).to_owned(), performed));
    }

    // Stable: equal counts keep the operator's own order, which is the
    // tie-break he stated in `docs/slot-candidates.md`.
    offers.sort_by_key(|(_, performed)| std::cmp::Reverse(performed.unwrap_or(0)));
    Ok(offers)
}

/// Ask which exercise fills a slot.
///
/// A number picks from the list; anything else is read as a vocabulary key, so
/// an exercise nobody thought to offer is one word rather than an impossibility.
fn ask_exercise(slot: SlotId, offers: &[(String, Option<usize>)]) -> Result<String, Failure> {
    println!("\n{slot}");
    for (at, (key, performed)) in offers.iter().enumerate() {
        let seen = match performed {
            None => String::new(),
            Some(0) => "never performed".to_owned(),
            Some(1) => "1 session".to_owned(),
            Some(many) => format!("{many} sessions"),
        };
        println!("  {:>2}. {key:<40} {seen}", at + 1);
    }

    let question = format!("  which? [{}] ", offers.first().map_or("", |(key, _)| key));
    ask_until(&question, |typed| {
        if typed.is_empty() {
            return offers
                .first()
                .map(|(key, _)| key.clone())
                .ok_or_else(|| "nothing is offered; name an exercise".to_owned());
        }
        if let Ok(number) = typed.parse::<usize>() {
            return offers
                .get(number.wrapping_sub(1))
                .map(|(key, _)| key.clone())
                .ok_or_else(|| format!("there is no {number} on the list"));
        }
        match exercise_named(typed) {
            Some(_) => Ok(typed.to_owned()),
            None => Err(format!(
                "{typed:?} is not an exercise — pick a number, or name one from \
                 the vocabulary"
            )),
        }
    })
}

/// Refuse before asking anything if the store cannot hold what the answers make.
async fn ready(parameters: &SqliteGenerationParameterStore) -> Result<(), Failure> {
    if parameters
        .current()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .is_some()
    {
        return Ok(());
    }
    Err(usage(
        "this store has no generation parameters, so nothing authored here could          prescribe anything. Run `fitness init` first — it stores them. Nothing          was asked and nothing was written",
    ))
}

async fn ask_fill(
    history: &SqliteExerciseHistory,
    slot: SlotId,
    block: &Draft,
) -> Result<(SlotId, FillLine), Failure> {
    // **The primary pattern's slot is the primary lift.** Authoring refuses a
    // block that names one exercise as its primary and fills that slot with
    // another, and rightly — the ladder and the slot would be climbing
    // different things. So it is stated rather than asked, and the operator
    // cannot answer his way into a document that will not author.
    if slot.as_str() == block.pattern {
        println!("\n{slot}");
        println!(
            "  {} — the primary, so it fills its own slot",
            block.primary
        );
        return Ok((slot, FillLine::Same(block.primary.clone())));
    }

    let offers = offered(history, slot).await?;
    let exercise = ask_exercise(slot, &offers)?;

    // The plyometric and power blocks are set at the start of a block and read
    // no history, so they carry their own sets and reps.
    if matches!(slot.block(), Block::Plyometric | Block::Power) {
        let sets = ask_until("  sets? [3] ", |typed| {
            if typed.is_empty() {
                return Ok(3);
            }
            parse_count("sets", typed)
        })?;
        let reps = ask_until("  reps? ", |typed| parse_count("reps", typed))?;
        return Ok((
            slot,
            FillLine::Static {
                exercise,
                sets,
                reps,
            },
        ));
    }

    // A hold is the authored duration on every session, so there is nothing to
    // alternate: asking would be offering a distinction that does not exist.
    if matches!(slot.block(), Block::Mobility) {
        return Ok((slot, FillLine::Same(exercise)));
    }

    let alternates = ask("  a different one on the other session? [no] ")?;
    if matches!(alternates.to_lowercase().as_str(), "y" | "yes") {
        let other = ask_exercise(slot, &offers)?;
        return Ok((
            slot,
            FillLine::Alternating {
                light: exercise,
                heavy: other,
            },
        ));
    }

    Ok((slot, FillLine::Same(exercise)))
}

const PATTERNS: [(&str, &str); 4] = [
    ("knee_dominant", "knee dominant"),
    ("hip_dominant", "hip dominant"),
    ("upper_push", "upper push"),
    ("upper_pull", "upper pull"),
];

const WEEKDAYS: [(&str, &str); 7] = [
    ("monday", "monday"),
    ("tuesday", "tuesday"),
    ("wednesday", "wednesday"),
    ("thursday", "thursday"),
    ("friday", "friday"),
    ("saturday", "saturday"),
    ("sunday", "sunday"),
];

fn ask_pattern() -> Result<&'static str, Failure> {
    println!("\nwhich pattern is the primary?");
    for (at, (_, name)) in PATTERNS.iter().enumerate() {
        println!("  {}. {name}", at + 1);
    }
    ask_until("  which? [1] ", |typed| {
        let number = if typed.is_empty() {
            1
        } else {
            typed
                .parse::<usize>()
                .map_err(|_| format!("{typed:?} is not one of the four"))?
        };
        PATTERNS
            .get(number.wrapping_sub(1))
            .map(|(key, _)| *key)
            .ok_or_else(|| format!("there is no {number} on the list"))
    })
}

/// The days a block runs, each with the session it is.
type Weekdays = Vec<(&'static str, &'static str)>;

/// Which weekdays the block runs, and which session each is.
///
/// **The light and the heavy are not interchangeable.** The heavy session is
/// the one the ladder gates on and the one an entry test is taken in, so a
/// block with no heavy day is a block that cannot advance.
fn ask_weekdays() -> Result<(Weekdays, &'static str), Failure> {
    println!("\nwhich days does it run, and which session is each?");
    println!("  l = light, h = heavy; - is not a training day");

    loop {
        let mut chosen = Vec::new();
        for (key, name) in WEEKDAYS {
            let role = ask_until(&format!("  {name:<10}[-] "), |typed| {
                match typed.to_lowercase().as_str() {
                    "" | "-" => Ok(None),
                    "l" | "light" => Ok(Some("light")),
                    "h" | "heavy" => Ok(Some("heavy")),
                    other => Err(format!("{other:?} is not l, h or -")),
                }
            })?;
            if let Some(role) = role {
                chosen.push((key, role));
            }
        }

        if chosen.is_empty() {
            println!("  a block has to run on some day — asking again");
            continue;
        }
        if !chosen.iter().any(|(_, role)| *role == "heavy") {
            println!("  a block needs a heavy session: it is what the ladder gates on");
            continue;
        }

        // Gating is on the heavy session wherever there is one, which there
        // now is.
        return Ok((chosen, "heavy"));
    }
}

fn ask_block(history_hint: Option<&str>) -> Result<Draft, Failure> {
    println!("A block: what it is, before what it contains.\n");

    let name = ask_until("name? ", |typed| {
        if typed.is_empty() {
            // The name is the identity a re-authoring supersedes on
            // (decision 0012), so an empty one would make every block the same
            // block.
            Err(
                "a block is identified by its name, and re-authoring under the \
                 same one is what corrects it"
                    .to_owned(),
            )
        } else {
            Ok(typed.to_owned())
        }
    })?;
    let start = ask_until("starts? ", parse_date)?;

    // Phase weeks. The entry test sits in front of them, which is said here
    // rather than left for the operator to work out from a total.
    let weeks = ask_until("how many weeks of phases? [9] ", |typed| {
        if typed.is_empty() {
            return Ok(9);
        }
        parse_count("weeks", typed)
    })?;

    let pattern = ask_pattern()?;
    let (weekdays, gating) = ask_weekdays()?;

    println!("\nthe lift this block is about");
    let primary = ask_until("  which exercise? ", |typed| match exercise_named(typed) {
        Some(Exercise::Reps(_)) => Ok(typed.to_owned()),
        Some(_) => Err(format!(
            "{typed:?} is not counted in repetitions, and a ladder needs one"
        )),
        None => Err(format!("{typed:?} is not an exercise in the vocabulary")),
    })?;

    println!("\nwhat you expect to lift. Week one measures it — this is the");
    println!("expectation the entry test confirms, not a number already proved.");
    if let Some(hint) = history_hint {
        println!("  {hint}");
    }
    let anchor = ask_until("  expected one-rep maximum? ", |typed| {
        if typed.is_empty() {
            Err("the ramp aims at this, so there is no sensible default".to_owned())
        } else {
            Ok(typed.trim_end_matches("kg").to_owned())
        }
    })?;
    let anchor_from = ask_until("  as of which date? ", parse_date)?;
    let entry_reps = ask_until(
        "  the entry test attempts it at how many reps? [3] ",
        |typed| {
            if typed.is_empty() {
                return Ok(3);
            }
            parse_count("reps", typed)
        },
    )?;
    let entry_light = {
        let typed = ask("  and the light session of that week runs at? [skip] ")?;
        if typed.is_empty() {
            None
        } else {
            Some(typed.trim_end_matches("kg").to_owned())
        }
    };

    Ok(Draft {
        name,
        start,
        weeks,
        pattern,
        primary,
        gating,
        weekdays,
        anchor,
        anchor_from,
        entry_reps,
        entry_light,
    })
}

/// Ask, write, and author.
pub async fn add(database: &Path, zone: &OperatorZone, into: Option<&Path>) -> Result<(), Failure> {
    interactive()?;

    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let history = SqliteExerciseHistory::new(pool.clone());

    // **Asked before the first question, not discovered after the last.** A
    // programme cannot be authored against nothing (§ 14), and finding that out
    // at the end costs the operator every answer he has just given. Setting the
    // machine up is what puts them there; see `setup::seed_parameters`.
    ready(&SqliteGenerationParameterStore::new(pool.clone())).await?;

    let block = ask_block(None)?;

    println!("\nAnd the slots. A number picks from the list; anything else is read");
    println!("as an exercise, so something you have never done is one word away.");

    let mut fills = Vec::with_capacity(SlotId::ALL.len());
    for slot in SlotId::ALL {
        fills.push(ask_fill(&history, *slot, &block).await?);
    }

    let document = render(&block, &fills);
    let path = into.map_or_else(
        || PathBuf::from(format!("{}.toml", block.name)),
        Path::to_path_buf,
    );
    std::fs::write(&path, &document).map_err(usage)?;
    println!("\nwritten to {}", path.display());

    // Authored through the same path a hand-written document takes, so the
    // interruptions come from the schedule exactly as they would have.
    crate::prescribing::add(database, zone, &path).await
}
