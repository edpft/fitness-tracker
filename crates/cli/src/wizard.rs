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

use application::{DiaryStore as _, ExerciseHistory as _, GenerationParameterStore as _};
use domain::{
    gym::{
        Kg, Load, OperatorZone, RepCount,
        exercise::{Exercise, RepsExercise},
    },
    // `prescription::Block` is the *slot* block — plyometric, power, strength.
    // The periodised one is a different type with the same word on it, so it is
    // named for what it holds: the plan a duration divides into.
    prescription::{
        Block, Calendar, GenerationParameters, InvalidBlock, LoadSteps, SessionRole, Skip, SlotId,
        Weekdays, block::Block as BlockPlan, rep_max,
    },
    schedule::{Diary, Discipline},
};
use infrastructure::{
    SqliteDiaryStore, SqliteExerciseHistory, SqliteGenerationParameterStore, connect,
    programme::draft::{Draft, FillLine, Ladder, Shape, render},
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit};

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
    for key in domain::prescription::candidates::for_slot(slot) {
        // **Only repetitions have a count to show.** `ExerciseHistory` answers
        // for exercises counted in reps, because that is what progression
        // needs — so a hold has no number here. Printing "never performed"
        // beside a couch stretch done every session would be worse than
        // printing nothing, which is what `None` renders as.
        let performed = match Exercise::named(key) {
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
        match Exercise::named(typed) {
            Some(_) => Ok(typed.to_owned()),
            None => Err(format!(
                "{typed:?} is not an exercise — pick a number, or name one from \
                 the vocabulary"
            )),
        }
    })
}

/// Refuse before asking anything if the store cannot hold what the answers make.
///
/// **And hand back what it found.** The questions need the parameters as well as
/// the store: what one step up the bar is, and so what "beat it" comes to, is
/// the plate grid in `scales` rather than anything to ask about.
async fn ready(
    parameters: &SqliteGenerationParameterStore,
) -> Result<GenerationParameters, Failure> {
    parameters
        .current()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .map(|(_, parameters)| parameters)
        .ok_or_else(|| {
            usage(
                "this store has no generation parameters, so nothing authored here could \
                 prescribe anything. Run `fitness init` first — it stores them. Nothing \
                 was asked and nothing was written",
            )
        })
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

/// The patterns a block can be built around.
///
/// **Two, not four.** The ladder, the anchor and the entry test are all about a
/// lower-body maximum, and an upper push or pull is an accessory slot whichever
/// block it sits in. Stated by the operator, twice — the four-item list came
/// from treating `SlotId`'s patterns as interchangeable, which they are not.
const PATTERNS: [(&str, &str); 2] = [
    ("knee_dominant", "knee dominant"),
    ("hip_dominant", "hip dominant"),
];

const WEEKDAYS: [(&str, Weekday); 7] = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
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
                .map_err(|_| format!("{typed:?} is neither of the two"))?
        };
        PATTERNS
            .get(number.wrapping_sub(1))
            .map(|(key, _)| *key)
            .ok_or_else(|| format!("there is no {number} on the list"))
    })
}

/// The days a block runs, each with the session it is, as the document names
/// them.
type WeekdayRoles = Vec<(&'static str, &'static str)>;

/// Which session each of the gym's days is.
///
/// **The schedule says which days are the gym's; the programme says what it
/// does with them.** This asked all seven and consulted nothing, so a block
/// could name days the schedule had given to cycling and nothing would object
/// — the autumn block agreed with the schedule by the operator's hand rather
/// than by construction. The days come from the diary now, and the only
/// question left is the one the schedule cannot answer.
///
/// **Ordinary days, not the days around the start.** A block starting inside a
/// holiday would otherwise be offered whatever that holiday left, which is the
/// alteration deciding the shape of the block rather than interrupting it. The
/// calendar takes the alteration out separately, as skips.
///
/// **The light and the heavy are not interchangeable.** The heavy session is
/// the one the ladder gates on and the one an entry test is taken in, so a
/// block with no heavy day is a block that cannot advance.
///
/// A day may still be declined with `-`: which days are the gym's is the
/// schedule's to say, and how many of them a given block uses is not.
fn ask_weekdays(
    diary: &Diary,
    start: Date,
) -> Result<(WeekdayRoles, Weekdays, &'static str), Failure> {
    let Some(available) = diary.ordinarily(start, Discipline::Gym) else {
        return Err(usage(format!(
            "the schedule says nothing about {start}, so there is no way to know \
             which days are the gym's. Record the week first: fitness schedule add"
        )));
    };
    if available.is_empty() {
        return Err(usage(format!(
            "the schedule gives the gym no day of the week as of {start}, so \
             there is nothing for a programme to run on"
        )));
    }

    let offered: Vec<(&'static str, Weekday)> = WEEKDAYS
        .into_iter()
        .filter(|(_, weekday)| available.contains(weekday))
        .collect();

    println!("\nwhich session is each of the gym's days?");
    println!(
        "  the schedule gives the gym {} as of {start}.",
        list(&offered)
    );
    println!("  l = light, h = heavy; - is a day this programme does not use");

    loop {
        let mut chosen = Vec::new();
        for (key, weekday) in &offered {
            let role = ask_until(&format!("  {key:<10}[-] "), |typed| {
                match typed.to_lowercase().as_str() {
                    "" | "-" => Ok(None),
                    "l" | "light" => Ok(Some(SessionRole::Light)),
                    "h" | "heavy" => Ok(Some(SessionRole::Heavy)),
                    other => Err(format!("{other:?} is not l, h or -")),
                }
            })?;
            if let Some(role) = role {
                chosen.push((*key, *weekday, role));
            }
        }

        if chosen.is_empty() {
            println!("  a programme has to run on some day — asking again");
            continue;
        }
        if !chosen
            .iter()
            .any(|(_, _, role)| *role == SessionRole::Heavy)
        {
            println!(
                "  a heavy session is needed: a ladder gates on it, and a test is taken in it"
            );
            continue;
        }

        // **The same answer in two shapes.** The document names its days in
        // words and the calendar needs them as weekdays and roles — and the
        // calendar is needed here, before the document exists, to work out how
        // many training weeks the operator's dates actually hold.
        let named = chosen
            .iter()
            .map(|(key, _, role)| (*key, role_word(*role)))
            .collect();
        let scheduled = Weekdays::new(
            chosen
                .iter()
                .map(|(_, weekday, role)| (*weekday, *role))
                .collect(),
        )
        .map_err(|error| usage(error.to_string()))?;

        // Gating is on the heavy session wherever there is one, which there
        // now is.
        return Ok((named, scheduled, "heavy"));
    }
}

/// Days in a sentence, so the line reads as one.
fn list(days: &[(&'static str, Weekday)]) -> String {
    let names: Vec<&str> = days.iter().map(|(key, _)| *key).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

const fn role_word(role: SessionRole) -> &'static str {
    match role {
        SessionRole::Light => "light",
        SessionRole::Heavy => "heavy",
    }
}

/// How many weeks the block gets, from the dates it has to run between.
///
/// **The operator states dates and the tool derives the plan.** "From the week
/// commencing 14 September through the week commencing 14 December" is how a
/// block gets decided; how that divides into an entry test and three phases is
/// the tool's job, and asking for a count of phase weeks was asking him to do
/// the arithmetic and the holidays in his head.
///
/// **So the dates are the only question.** Decision 0019 removed the arithmetic
/// and left the question standing, defaulted to the answer it had just worked
/// out — which invited an answer contradicting the end date the operator had
/// given one line earlier. A block that should end sooner is a block with an
/// earlier `ends?`, and a span too long for one block is refused rather than
/// quietly filled: fifteen phase weeks is where the top-set ladder stops being
/// liftable, and there is nothing to put in the remainder.
///
/// **What the schedule takes is taken here**, not discovered later. A week the
/// diary leaves nothing in is not a training week, so the span holds one fewer
/// — and because the calendar counts the same way in the other direction, a
/// duration derived here spans back to exactly these dates.
fn ask_weeks(
    climbing: Climbing,
    diary: &Diary,
    start: Date,
    weekdays: &Weekdays,
) -> Result<u32, Failure> {
    loop {
        let last = ask_until("ends? ", |typed| {
            let ends = parse_date(typed)?;
            if ends <= start {
                return Err(format!("a block cannot end on {ends}, before it starts"));
            }
            Ok(ends)
        })?;

        // The gym's own losses. A day the other discipline keeps is a day this
        // programme cannot run, which is why the question is asked of the
        // diary rather than of a list of holidays.
        let skips: Vec<Skip> = diary
            .unavailable(start, last, Discipline::Gym)
            .into_iter()
            .map(Skip::day)
            .collect();
        let available = Calendar::training_weeks_within(start, last, weekdays, &skips);

        // **A block's first week measures the maximum the rest is a share of,
        // and it is not a phase**, so `duration_weeks` is one fewer than the
        // span. A ladder has no such week — every week it holds is a climbing
        // week — so its duration is the span itself.
        let weeks = match climbing {
            Climbing::Linear => available,
            Climbing::Block => available.saturating_sub(1),
        };
        if weeks == 0 {
            println!("  {start} to {last} leaves no room to train — try a later end");
            continue;
        }

        report_span(start, last, available, &skips);

        match climbing {
            Climbing::Block => match BlockPlan::new(weeks) {
                Ok(plan) => {
                    describe(plan);
                    return Ok(weeks);
                }
                Err(error) => {
                    println!("  {error}");
                    println!("  {}", remedy(error));
                }
            },
            // **A ladder has no table to describe and no ceiling to refuse
            // against.** What it does need is somewhere to climb: one week is a
            // single session's load, not a progression. The climb itself lives
            // in the generation parameters rather than in the programme, so
            // whether it makes a ladder over this duration is `Linear::new`'s
            // to answer at authoring.
            Climbing::Linear => {
                if weeks < 2 {
                    println!("  a ladder needs somewhere to climb, and {weeks} week is one load");
                    println!("  try a later end");
                    continue;
                }
                println!("  {weeks} weeks of climbing, at the rate the parameters hold.");
                return Ok(weeks);
            }
        }
    }
}

/// Which end of the span to move, for a duration no block can hold.
///
/// **The direction is the whole of the advice.** One line told the operator to
/// try a later end whichever way the block failed, and for a block already too
/// long that is the wrong way — following it makes the next attempt worse than
/// the one it was correcting.
const fn remedy(error: InvalidBlock) -> &'static str {
    match error {
        InvalidBlock::TooShort { .. } => "try a later end",
        // Fifteen phase weeks, plus the week that measures the anchor.
        InvalidBlock::TooLong { .. } => {
            "a block runs at most 15 weeks of phases, so 16 with its entry test \
             — try an earlier end"
        }
        // Not reachable from a duration, and a wrong word here would be worse
        // than a vague one.
        InvalidBlock::EntryTestTooLong { .. } => "try different dates",
    }
}

/// What the dates came to, and what the diary took out of them.
fn report_span(start: Date, last: Date, available: u32, skips: &[Skip]) {
    let taken: Vec<String> = skips.iter().map(ToString::to_string).collect();
    if taken.is_empty() {
        println!("  {available} weeks, {start} to {last}. Nothing lost to the schedule.");
        return;
    }
    // **Printed rather than silently absorbed.** Silence here looks identical
    // to the bug where a week away quietly cost a rung.
    println!(
        "  {available} weeks, {start} to {last}, after the schedule takes {}.",
        taken.join(", ")
    );
}

/// The split, said back in the words the operator's own table uses.
fn describe(plan: BlockPlan) {
    println!(
        "  the test, then {} accumulation, {} intensification, {} realisation — {} weeks in all.",
        plan.accumulation_weeks(),
        plan.intensification_weeks(),
        plan.realisation_weeks(),
        plan.duration_weeks().saturating_add(1),
    );
    println!("  The last realisation week is the exit test.");
}

/// What the record says the primary is worth, as a one-rep maximum.
struct Best {
    /// The set's own load and repetitions, so the operator can see what the
    /// number was read off rather than being handed a figure.
    load: Kg,
    reps: u32,
    on: Date,
    /// Converted through the repetition-maximum table, and quantised onto the
    /// grid the exercise is loaded on.
    maximum: Kg,
}

/// What a completed set is worth as a one-rep maximum.
///
/// The same published table the block's own percentages run on
/// ([`rep_max`]), applied in the other direction: a set of `n` at zero in
/// reserve *is* an `n`-rep maximum, so dividing by its share gives the one-rep
/// maximum it implies.
impl Best {
    /// The set this was read off, as the operator would say it.
    ///
    /// **The set, not just the number.** A maximum handed over bare is a figure
    /// to be trusted or not; the set behind it is something he can recognise.
    fn describe(&self, lift: &str) -> String {
        if self.reps == 1 {
            format!("your best {lift} is {}kg, on {}", self.load, self.on)
        } else {
            format!(
                "your best {lift} is {}kg × {}, on {} — a maximum of {}kg",
                self.load, self.reps, self.on, self.maximum,
            )
        }
    }
}

fn as_one_rep_max(load: Kg, reps: RepCount) -> Option<Kg> {
    let points = i64::from(rep_max(reps)?.as_basis_points());
    let grams = i64::try_from(load.as_grams())
        .ok()?
        .checked_mul(10_000)?
        .checked_div(points)?;
    Some(Kg::from_grams(u64::try_from(grams).ok()?))
}

/// The best one-rep maximum the record implies for a lift.
///
/// **Every completed working set is a candidate, not just the heaviest.** A
/// triple at 85 implies more than a single at 88, and the block's own
/// percentages already agree — so the comparison is made in the unit the anchor
/// is stated in rather than in bare load.
///
/// `None` where nothing has been performed, which is a real state: an exercise
/// exists before it is prescribed and is prescribed before it has been done.
async fn best_of(
    history: &SqliteExerciseHistory,
    lift: RepsExercise,
    scale: Option<&LoadSteps>,
) -> Result<Option<Best>, Failure> {
    let performances = history
        .performances(lift)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let mut best: Option<Best> = None;
    for performance in performances {
        for set in performance.sets {
            let Load::Absolute(load) = set.load else {
                continue;
            };
            let Some(reps) = set.outcome.completed() else {
                continue;
            };
            let Some(maximum) = as_one_rep_max(load, *reps) else {
                continue;
            };
            let maximum = scale.map_or(maximum, |steps| steps.quantise(maximum));
            if best
                .as_ref()
                .is_none_or(|held| maximum.as_grams() > held.maximum.as_grams())
            {
                best = Some(Best {
                    load,
                    reps: reps.as_u32(),
                    on: performance.on,
                    maximum,
                });
            }
        }
    }
    Ok(best)
}

/// What the entry test is an attempt at.
///
/// **Three intents, and the operator picks between them** — match a recent
/// maximum, exceed one, or declare a number. Stated by him on 2026-08-26, and
/// it settles what the date on an anchor means: where the number points at a
/// performance the date is that performance's, and where it is plucked out of
/// the air there is nothing for a date to mean, so none is asked.
///
/// **Never `tested`.** The record shows a set, not a test — a completed single
/// may have been a top set rather than an attempt at a ceiling — so reading a
/// maximum off it is an estimate however few repetitions it took. Only a test
/// this tool issued may claim to have tested anything.
fn ask_anchor(
    asks: &str,
    lift: &str,
    best: Option<&Best>,
    scale: Option<&LoadSteps>,
    start: Date,
) -> Result<(String, Date, &'static str), Failure> {
    println!("\n{asks}");

    let Some(best) = best else {
        // Nothing to match and nothing to beat. The one remaining answer is
        // asked directly rather than offered as the only item on a list.
        println!("  nothing in the record for {lift}, so there is nothing to match");
        let declared = ask_until("  what should it aim at? ", declared_load)?;
        return Ok((declared, start, "asserted"));
    };

    let beaten = scale.map_or(best.maximum, |steps| steps.next_above(best.maximum));
    println!("  {}", best.describe(lift));
    // The whole figure is padded, not the number in front of the unit: "95kg"
    // and "97.5kg" have to end in the same column to be comparable at a glance.
    println!("   1. match it{:>30}", format!("{}kg", best.maximum));
    println!("   2. beat it{:>31}", format!("{beaten}kg"));
    println!("   3. a number of my own");

    let choice = ask_until("  which? [2] ", |typed| match typed {
        "" | "2" => Ok(2),
        "1" => Ok(1),
        "3" => Ok(3),
        other => Err(format!("{other:?} is not one of the three")),
    })?;

    match choice {
        1 => Ok((best.maximum.to_string(), best.on, "estimated")),
        // Asserted: nobody has lifted it. The date is still the performance's,
        // because that performance is what the assertion is reasoning from.
        2 => Ok((beaten.to_string(), best.on, "asserted")),
        _ => {
            let declared = ask_until("  what should it aim at? ", declared_load)?;
            Ok((declared, start, "asserted"))
        }
    }
}

fn declared_load(typed: &str) -> Result<String, String> {
    if typed.is_empty() {
        return Err("the ramp aims at this, so there is no sensible default".to_owned());
    }
    Ok(typed.trim_end_matches("kg").to_owned())
}

/// Which of the three programmes this build can author.
///
/// **The wizard reached one of them.** `document.rs` has read `test`, `linear`
/// and `block` since the templates existed, and the wizard authored a block
/// whatever the operator wanted — so the only way to a test or a ladder was a
/// hand-written document, which is the input format this exists to replace.
const TEMPLATES: [(&str, &str); 3] = [
    (
        "block",
        "accumulate, intensify, realise — and test at each end",
    ),
    ("linear", "one ladder, climbing every week it runs"),
    ("test", "a single week, measuring"),
];

fn ask_template() -> Result<&'static str, Failure> {
    println!("which kind of programme?");
    for (at, (name, gloss)) in TEMPLATES.iter().enumerate() {
        println!("  {:>2}. {name:<8} {gloss}", at.saturating_add(1));
    }
    ask_until("  which? [1] ", |typed| {
        if typed.is_empty() {
            return Ok(TEMPLATES[0].0);
        }
        TEMPLATES
            .iter()
            .enumerate()
            .find(|(at, (name, _))| {
                typed == at.saturating_add(1).to_string() || typed.eq_ignore_ascii_case(name)
            })
            .map(|(_, (name, _))| *name)
            .ok_or_else(|| format!("{typed:?} is not one of the three"))
    })
}

/// The questions every programme answers, whatever its template.
struct Common {
    name: String,
    start: Date,
    pattern: &'static str,
    named: WeekdayRoles,
    scheduled: Weekdays,
    gating: &'static str,
}

fn ask_common(diary: &Diary, template: &str) -> Result<Common, Failure> {
    let name = ask_until("name? ", |typed| {
        if typed.is_empty() {
            // The name is the identity a re-authoring supersedes on
            // (decision 0012), so an empty one would make every programme the
            // same programme.
            Err(format!(
                "a {template} is identified by its name, and re-authoring under \
                 the same one is what corrects it"
            ))
        } else {
            Ok(typed.to_owned())
        }
    })?;
    let start = ask_until("starts? ", parse_date)?;

    let pattern = ask_pattern()?;
    let (named, scheduled, gating) = ask_weekdays(diary, start)?;

    Ok(Common {
        name,
        start,
        pattern,
        named,
        scheduled,
        gating,
    })
}

/// **Asked after the duration, not before it.** The dates are what a programme
/// *is*; the lift is the first thing it is about, and the seventeen slots
/// follow it. Moving it in front of `ends?` puts an exercise between two dates.
fn ask_lift(template: &str) -> Result<RepsExercise, Failure> {
    println!("\nthe lift this {template} is about");
    ask_until("  which exercise? ", |typed| match Exercise::named(typed) {
        Some(Exercise::Reps(exercise)) => Ok(exercise),
        Some(_) => Err(format!(
            "{typed:?} is not counted in repetitions, and a primary needs one"
        )),
        None => Err(format!("{typed:?} is not an exercise in the vocabulary")),
    })
}

async fn ask_programme(
    template: &'static str,
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<Draft, Failure> {
    match template {
        "test" => ask_test(diary, history, parameters).await,
        "linear" => ask_climb(Climbing::Linear, diary, history, parameters).await,
        _ => ask_climb(Climbing::Block, diary, history, parameters).await,
    }
}

/// **A test is a week, so it is never asked how long it is.** `document.rs`
/// refuses a duration on one, and `Test::week` derives the calendar from the
/// start alone.
async fn ask_test(
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<Draft, Failure> {
    println!("A test: one week, measuring.\n");
    let common = ask_common(diary, "test")?;
    let lift = ask_lift("test")?;
    let primary = lift.as_str().to_owned();

    let scale = parameters.scales.for_exercise(Exercise::Reps(lift));
    let best = best_of(history, lift, scale).await?;

    println!("\nwhat should the test aim at?");
    // **Inheriting is the ordinary case** (decision 0013): a test between two
    // programmes is for the load the progression stands at, and the programme
    // before it is what knows that. A declared target is for the case
    // inheritance cannot answer.
    let target = match best.as_ref() {
        Some(best) => {
            println!("  {}", best.describe(&primary));
            println!("   1. the load the programme before this one stands at");
            println!("   2. a number of my own");
            let choice = ask_until("  which? [1] ", |typed| match typed {
                "" | "1" => Ok(1),
                "2" => Ok(2),
                other => Err(format!("{other:?} is not one of the two")),
            })?;
            if choice == 1 {
                None
            } else {
                Some(ask_until("  what should it aim at? ", declared_load)?)
            }
        }
        // Nothing performed, so there is nothing to describe and nothing a
        // predecessor could hand over either.
        None => Some(ask_until("  what should it aim at? ", declared_load)?),
    };
    let reps = ask_until("  attempted at how many reps? [1] ", |typed| {
        if typed.is_empty() {
            return Ok(1);
        }
        parse_count("reps", typed)
    })?;

    Ok(Draft {
        name: common.name,
        start: common.start,
        pattern: common.pattern,
        primary,
        weekdays: common.named,
        shape: Shape::Test { reps, target },
    })
}

/// Which climbing template, and so which week the span's first one is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Climbing {
    /// Every week it holds is a climbing week.
    Linear,
    /// The first week measures the anchor and is not a phase.
    Block,
}

async fn ask_climb(
    climbing: Climbing,
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<Draft, Failure> {
    let (template, asks_anchor) = match climbing {
        // **A ladder has no test week, so nothing aims at its anchor.** The
        // anchor is the maximum its percentages are shares of, and week one
        // climbs from it rather than measuring it.
        Climbing::Linear => ("ladder", "what does the ladder climb from?"),
        Climbing::Block => ("block", "what should the entry test aim at?"),
    };
    println!("A {template}: what it is, before what it contains.\n");
    let common = ask_common(diary, template)?;
    let weeks = ask_weeks(climbing, diary, common.start, &common.scheduled)?;
    let lift = ask_lift(template)?;
    let primary = lift.as_str().to_owned();

    let scale = parameters.scales.for_exercise(Exercise::Reps(lift));
    let best = best_of(history, lift, scale).await?;
    let (anchor, anchor_from, provenance) =
        ask_anchor(asks_anchor, &primary, best.as_ref(), scale, common.start)?;

    let ladder = match climbing {
        Climbing::Linear => {
            // **The anchor is where it opens unless the operator says
            // otherwise**, which is the `None` the document reader takes as
            // "derive it".
            let typed = ask("  and it opens at? [the anchor] ")?;
            Ladder::Linear {
                opening: (!typed.is_empty()).then(|| typed.trim_end_matches("kg").to_owned()),
            }
        }
        Climbing::Block => {
            let entry_reps = ask_until(
                "  the entry test attempts it at how many reps? [3] ",
                |typed| {
                    if typed.is_empty() {
                        return Ok(3);
                    }
                    parse_count("reps", typed)
                },
            )?;
            let typed = ask("  and the light session of that week runs at? [skip] ")?;
            Ladder::Block {
                entry_reps,
                entry_light: (!typed.is_empty()).then(|| typed.trim_end_matches("kg").to_owned()),
            }
        }
    };

    Ok(Draft {
        name: common.name,
        start: common.start,
        pattern: common.pattern,
        primary,
        weekdays: common.named,
        shape: Shape::Climb {
            weeks,
            gating: common.gating,
            anchor,
            anchor_from,
            provenance,
            ladder,
        },
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
    let parameters = ready(&SqliteGenerationParameterStore::new(pool.clone())).await?;

    let diary = SqliteDiaryStore::new(pool.clone())
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let template = ask_template()?;
    let block = ask_programme(template, &diary, &history, &parameters).await?;

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
