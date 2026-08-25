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
    collections::{BTreeMap, BTreeSet},
    io::{IsTerminal, Write},
    num::NonZeroU8,
    path::Path,
};

use application::{DiaryAuthor as _, DiaryStore as _};
use domain::{
    gym::OperatorZone,
    schedule::{Alteration, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
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

fn parse_discipline(typed: &str) -> Result<Discipline, String> {
    match typed.to_lowercase().as_str() {
        "" | "g" | "gym" => Ok(Discipline::Gym),
        "c" | "cycling" => Ok(Discipline::Cycling),
        other => Err(format!("{other:?} is not gym or cycling")),
    }
}

/// Ask for a week's worth of slots, a day at a time, and whose each one is.
///
/// **Two passes rather than one prompt per slot-and-discipline.** Which parts of
/// a day are free is a different question from who gets them, and asking them
/// together produced a prompt nobody could read. The second pass asks only about
/// the slots the first one found, so a four-slot week is eleven short questions
/// and eight of them are a single keystroke.
///
/// A slot cannot be claimed twice, because it is asked about once.
fn ask_slots(
    preamble: &str,
    weekdays: &[(Weekday, &str)],
) -> Result<BTreeMap<TrainingSlot, Discipline>, Failure> {
    println!("{preamble}");
    println!("  m = morning, a = afternoon, e = evening; several is \"me\"; - is none");

    let mut found = Vec::new();
    for &(weekday, name) in weekdays {
        let parts = ask_until(&format!("  {name:<10}[-] "), parse_parts)?;
        for part in parts {
            found.push((name, TrainingSlot::new(weekday, part)));
        }
    }

    if found.is_empty() {
        return Ok(BTreeMap::new());
    }

    println!("And which of those are the gym's?");
    println!("  g = gym, c = cycling");

    let mut slots = BTreeMap::new();
    for (name, slot) in found {
        let asked = format!("  {:<20}[gym] ", format!("{name} {}", slot.part));
        slots.insert(slot, ask_until(&asked, parse_discipline)?);
    }
    Ok(slots)
}

/// The weekdays a run of days actually covers, in the order it meets them.
///
/// **Asking about all seven is asking about days that do not occur.** A trip
/// from Friday to Monday has no Wednesday in it, and a slot stated for one
/// would be a fact about a day the alteration never touches — silently
/// meaningless, and reasonable to expect otherwise. A run of a week or more
/// meets every weekday and is asked about every weekday.
///
/// Run order rather than Monday-first, because that is the order the operator
/// lives it: the Friday they leave, then the weekend, then the Monday back.
fn covered_weekdays(start: Date, days: NonZeroU8) -> Vec<(Weekday, &'static str)> {
    let mut covered: Vec<(Weekday, &'static str)> = Vec::new();
    let mut cursor = start;

    for _ in 0..days.get() {
        if let Some(&(weekday, name)) = WEEKDAYS
            .iter()
            .find(|(weekday, _)| *weekday == cursor.weekday())
            && !covered.iter().any(|(seen, _)| *seen == weekday)
        {
            covered.push((weekday, name));
        }
        if covered.len() == WEEKDAYS.len() {
            break;
        }
        let Ok(next) = cursor.tomorrow() else { break };
        cursor = next;
    }

    covered
}

fn yes(typed: &str) -> bool {
    matches!(typed.to_lowercase().as_str(), "y" | "yes")
}

/// Only an explicit no. An empty line takes the offered default, which for
/// "are you able to train" is yes — so a stray return cannot cancel a week.
fn no(typed: &str) -> bool {
    matches!(typed.to_lowercase().as_str(), "n" | "no")
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

    let slots = ask_slots(
        "Which parts of each day do you have room to train?",
        &WEEKDAYS,
    )?;
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

    // **The three cases, asked so the commonest is one keystroke.** Leaving the
    // slots alone, having none at all, and having different ones are three
    // different facts. Being unable to train is much the most common of them —
    // it is why most alterations get recorded — so it is asked first and
    // answered outright, and the walk through the days only happens for the
    // case that actually needs it.
    let slots = if no(&ask("Are you able to train during this period? [yes] ")?) {
        Some(BTreeMap::new())
    } else if yes(&ask("Does this change when you can train? [no] ")?) {
        Some(ask_slots(
            "Which parts of each day, while it lasts?",
            &covered_weekdays(start, days),
        )?)
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

#[cfg(test)]
mod tests {
    use super::{covered_weekdays, no, parse_parts, yes};
    use domain::schedule::PartOfDay;
    use jiff::civil::{Weekday, date};

    /// **A run asks about the days it contains, and no others.**
    ///
    /// Friday to Monday has no Wednesday in it. Asking anyway invites a slot
    /// stated for a day the alteration never touches, which would be silently
    /// meaningless — and reasonable to expect otherwise.
    #[test]
    fn a_short_run_covers_only_the_days_it_meets() {
        let Some(four) = std::num::NonZeroU8::new(4) else {
            panic!("four is not zero")
        };
        // Friday 11 September 2026 through the Monday.
        let covered: Vec<Weekday> = covered_weekdays(date(2026, 9, 11), four)
            .into_iter()
            .map(|(weekday, _)| weekday)
            .collect();

        assert_eq!(
            covered,
            vec![
                Weekday::Friday,
                Weekday::Saturday,
                Weekday::Sunday,
                Weekday::Monday
            ],
            "in the order the trip meets them, not Monday first"
        );
    }

    /// One day is one weekday.
    #[test]
    fn a_single_day_covers_one_weekday() {
        let Some(one) = std::num::NonZeroU8::new(1) else {
            panic!("one is not zero")
        };
        let covered = covered_weekdays(date(2026, 9, 14), one);
        assert_eq!(covered.len(), 1);
        assert_eq!(covered[0].0, Weekday::Monday);
    }

    /// A week or more meets every weekday, and each is asked about once.
    #[test]
    fn a_week_or_more_covers_every_weekday_once() {
        for length in [7_u8, 10, 30] {
            let Some(days) = std::num::NonZeroU8::new(length) else {
                panic!("{length} is not zero")
            };
            let covered = covered_weekdays(date(2026, 9, 11), days);
            assert_eq!(covered.len(), 7, "{length} days meets all seven");

            let mut seen: Vec<Weekday> = covered.iter().map(|(day, _)| *day).collect();
            seen.sort_unstable_by_key(|day| day.to_monday_zero_offset());
            seen.dedup();
            assert_eq!(seen.len(), 7, "{length} days asks about each one once");
        }
    }

    /// **An empty line takes the offered default, and the defaults differ.**
    ///
    /// "Are you able to train" defaults to yes, so a stray return cannot cancel
    /// a week's training; "does this change when you can train" defaults to no,
    /// so a stray return cannot rewrite it either.
    #[test]
    fn an_empty_answer_is_neither_a_yes_nor_a_no() {
        assert!(!yes(""), "an empty line does not agree");
        assert!(!no(""), "and does not refuse");

        assert!(yes("y") && yes("yes") && yes("YES"));
        assert!(no("n") && no("no") && no("No"));
    }

    /// `-` and an empty line both mean no part of this day.
    #[test]
    fn a_day_with_no_parts_is_written_two_ways() {
        let Ok(dash) = parse_parts("-") else {
            panic!("- parses")
        };
        let Ok(empty) = parse_parts("") else {
            panic!("an empty line parses")
        };
        assert!(dash.is_empty() && empty.is_empty());

        let Ok(several) = parse_parts("me") else {
            panic!("me parses")
        };
        assert_eq!(several.len(), 2);
        assert!(several.contains(&PartOfDay::Morning) && several.contains(&PartOfDay::Evening));
    }
}
