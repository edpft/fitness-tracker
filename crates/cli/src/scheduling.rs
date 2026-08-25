//! `fitness schedule` — when there is room to train, and what departs from it.
//!
//! **Operator-level, so no programme is consulted here.** The diary records the
//! times there is room to train; which of those the gym may use is allocation,
//! and allocation is planning rather than fact.
//!
//! ## Asked, not authored
//!
//! There is no document. A pattern is four or five slots and an alteration is a
//! date, a length and a reason — small enough that a file to hold them would be
//! a file to lose, and the store is where the object lives. So both are typed at
//! a prompt, and `show` reads them back.
//!
//! **Every prompt refuses rather than assumes when nobody is there.** The CLI is
//! meant to run under a scheduler, and a wizard that blocks on stdin in cron is
//! a hang rather than an error. `is_terminal` is checked once, up front, so the
//! refusal arrives before any question instead of halfway through.

use std::{
    collections::BTreeSet,
    io::{IsTerminal, Write},
    num::NonZeroU8,
    path::Path,
};

use application::{DiaryAuthor as _, DiaryStore as _};
use domain::{
    gym::OperatorZone,
    schedule::{Alteration, PartOfDay, TrainingPattern, TrainingSlot},
};
use infrastructure::{SqliteDiaryStore, connect};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, output};

const WEEKDAYS: [(Weekday, &str); 7] = [
    (Weekday::Monday, "monday"),
    (Weekday::Tuesday, "tuesday"),
    (Weekday::Wednesday, "wednesday"),
    (Weekday::Thursday, "thursday"),
    (Weekday::Friday, "friday"),
    (Weekday::Saturday, "saturday"),
    (Weekday::Sunday, "sunday"),
];

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure::message(message.to_string(), exit::USAGE)
}

/// Somebody has to be there to answer.
fn interactive() -> Result<(), Failure> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(usage(
        "this asks questions and there is nobody to ask: run it from a terminal. \
         Nothing was recorded",
    ))
}

/// One line typed at a prompt, trimmed. Empty means "take the default".
fn ask(question: &str) -> Result<String, Failure> {
    print!("{question}");
    std::io::stdout().flush().map_err(usage)?;

    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed).map_err(usage)?;
    Ok(typed.trim().to_owned())
}

/// Repeat a question until the answer parses.
///
/// A mistyped zone at question two should not throw away the four answers
/// already given, which is what returning an error from the middle of a wizard
/// would do.
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

fn parse_zone(typed: &str) -> Result<OperatorZone, String> {
    OperatorZone::try_from(typed.to_owned()).map_err(|error| error.to_string())
}

/// The parts of one day, as `m`, `a`, `e` — or nothing at all.
///
/// Letters rather than words because this is asked seven times, and `-` for
/// none because an empty line is easy to produce by accident and this is the
/// answer that removes a day.
fn parse_parts(typed: &str) -> Result<BTreeSet<PartOfDay>, String> {
    let lowered = typed.to_lowercase();
    if lowered.is_empty() || lowered == "-" {
        return Ok(BTreeSet::new());
    }

    let mut parts = BTreeSet::new();
    for letter in lowered.chars().filter(|c| !c.is_whitespace() && *c != ',') {
        let part = match letter {
            'm' => PartOfDay::Morning,
            'a' => PartOfDay::Afternoon,
            'e' => PartOfDay::Evening,
            other => {
                return Err(format!(
                    "{other:?} is not m, a or e — type the parts you can train, \
                     or - for none"
                ));
            }
        };
        parts.insert(part);
    }
    Ok(parts)
}

/// Ask for a week's worth of slots, a day at a time.
fn ask_slots(preamble: &str) -> Result<BTreeSet<TrainingSlot>, Failure> {
    println!("{preamble}");
    println!("  m = morning, a = afternoon, e = evening; several is \"me\"; - is none");

    let mut slots = BTreeSet::new();
    for (weekday, name) in WEEKDAYS {
        let parts = ask_until(&format!("  {name:<10}[-] "), parse_parts)?;
        for part in parts {
            slots.insert(TrainingSlot::new(weekday, part));
        }
    }
    Ok(slots)
}

fn yes(typed: &str) -> bool {
    matches!(typed.to_lowercase().as_str(), "y" | "yes")
}

async fn store(database: &Path) -> Result<SqliteDiaryStore, Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    Ok(SqliteDiaryStore::new(pool))
}

/// Ask for the ordinary pattern and record it.
pub async fn add(database: &Path) -> Result<(), Failure> {
    interactive()?;

    println!("When do you ordinarily have room to train?");
    let from = ask_until("From which date? [today] ", |typed| {
        if typed.is_empty() {
            // Not a compiled-in default: it is what somebody at a prompt means
            // by pressing enter, and they can see what they are agreeing to.
            return Ok(jiff::Zoned::now().date());
        }
        parse_date(typed)
    })?;
    let zone = ask_until("Which IANA time zone? [Europe/London] ", |typed| {
        parse_zone(if typed.is_empty() {
            "Europe/London"
        } else {
            typed
        })
    })?;

    let slots = ask_slots("Which parts of each day?")?;
    let pattern = TrainingPattern::new(from, zone, slots);

    store(database)
        .await?
        .record_pattern(&pattern)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::pattern_recorded(&pattern);
    Ok(())
}

/// Ask for an alteration and record it.
pub async fn alter(database: &Path) -> Result<(), Failure> {
    interactive()?;

    println!("What departs from the ordinary pattern?");
    println!("  Not only holidays — a course, a visitor or a late finish all count.");

    let start = ask_until("From which date? ", parse_date)?;
    let days = ask_until("How many days? [1] ", |typed| {
        if typed.is_empty() {
            return NonZeroU8::new(1).ok_or_else(|| "one is not zero".to_owned());
        }
        typed
            .parse::<u8>()
            .ok()
            .and_then(NonZeroU8::new)
            .ok_or_else(|| format!("{typed:?} is not a number of days from 1 to 255"))
    })?;

    let zone = {
        let typed = ask("In a different time zone? [no] ")?;
        if typed.is_empty() || typed.eq_ignore_ascii_case("no") {
            None
        } else {
            Some(ask_until("Which IANA time zone? ", parse_zone)?)
        }
    };

    // **The three cases, asked as two questions.** Leaving the slots alone is
    // not the same as having none, and it is not the same as having different
    // ones — so "does this change when you can train" comes first, and only
    // then what it changes them to.
    let slots = if yes(&ask("Does this change when you can train? [no] ")?) {
        Some(ask_slots("Which parts of each day, while it lasts?")?)
    } else {
        None
    };

    let reason = ask_until("Why? ", |typed| {
        if typed.is_empty() {
            Err("an alteration nobody explained is unreadable six months later".to_owned())
        } else {
            Ok(typed.to_owned())
        }
    })?;

    let alteration = Alteration::new(start, days, zone, slots, reason);

    store(database)
        .await?
        .record_alteration(&alteration)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::alteration_recorded(&alteration);
    Ok(())
}

/// Report the ordinary pattern and everything that departs from it.
pub async fn show(database: &Path) -> Result<(), Failure> {
    let diary = store(database)
        .await?
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::schedule(&diary);
    Ok(())
}
