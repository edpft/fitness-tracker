//! `fitness programme add`, asked rather than typed.
//!
//! **Seventeen slots out of a hundred and thirty-five exercises.** A fresh
//! periodisation states every slot itself — there is no predecessor to inherit
//! from — and typing that by hand is the pain this exists to remove.
//!
//! ## It authors, and leaves no file behind
//!
//! Until 2026-09-06 it wrote a TOML document and then read it back, so that the
//! operator had something reviewable to keep. The document was also the only
//! other way in, and the two paths had to agree about everything: what a
//! template may state, where the interruptions come from, which fills a test
//! inherits. The answers now go to the store directly, and the questions are the
//! only authoring path there is.
//!
//! What is asked is typed as it is answered — a [`PrimaryPattern`] rather than
//! the word for one — so the assembly in [`domain::prescription::authored`] has
//! nothing left to validate that the questions did not already refuse.
//!
//! ## Ordered by the record, limited by nothing
//!
//! Each slot offers what `candidates` holds for it, sorted by how much of it
//! the record actually contains. The operator asked to see options he has not
//! done before, so the list is an offer and not a menu: any key in the
//! vocabulary is accepted, and the numbers are a shortcut rather than a fence.

use std::{
    io::{IsTerminal, Write},
    path::Path,
};

use application::{
    DiaryStore as _, ExerciseHistory as _, GenerationParameterStore as _, PlanAuthor as _,
    PlanStore as _, prescribe::Authoring,
};
use domain::{
    gym::{
        Kg, Load, OperatorZone, RepCount,
        exercise::{Exercise, RepsExercise},
    },
    plan::{Plan, PlanName, Programme},
    // `prescription::Block` is the *slot* block — plyometric, power, strength.
    // The periodised one is a different type with the same word on it, so it is
    // named for what it holds: the plan a duration divides into.
    prescription::{
        Anchor, AnchorProvenance, Anchoring, Authored, Block, Calendar, Entry, EntryTest, Fill,
        GenerationParameters, InvalidBlock, LoadSteps, Mesocycle, PerRole, PrimaryPattern,
        SessionRole, Skip, SlotFills, SlotId, StaticFill, TestTarget, Weekdays, authored::Shape,
        block::Block as BlockPlan, rep_max,
    },
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
    schedule::{Diary, Discipline},
};
use infrastructure::{
    SqliteDiaryStore, SqliteExerciseHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqlitePlanStore, connect,
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, output, plan::SBS_MICROCYCLES};

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure::message(message.to_string(), exit::USAGE)
}

pub fn interactive() -> Result<(), Failure> {
    if std::io::stdin().is_terminal() {
        return Ok(());
    }
    Err(usage(
        "this asks questions and there is nobody to ask: run it from a terminal. \
         Nothing was written",
    ))
}

pub fn ask(question: &str) -> Result<String, Failure> {
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
pub fn ask_until<T>(
    question: &str,
    parse: impl Fn(&str) -> Result<T, String>,
) -> Result<T, Failure> {
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
fn ask_exercise(slot: SlotId, offers: &[(String, Option<usize>)]) -> Result<Exercise, Failure> {
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
        // **The offer is a vocabulary key either way.** A number picks one off
        // the list and a word is looked up, and both end at the same
        // `Exercise` — so a key that no longer names an exercise is caught here
        // rather than several hundred lines later.
        let key = if typed.is_empty() {
            offers
                .first()
                .map(|(key, _)| key.clone())
                .ok_or_else(|| "nothing is offered; name an exercise".to_owned())?
        } else if let Ok(number) = typed.parse::<usize>() {
            offers
                .get(number.wrapping_sub(1))
                .map(|(key, _)| key.clone())
                .ok_or_else(|| format!("there is no {number} on the list"))?
        } else {
            typed.to_owned()
        };
        Exercise::named(&key).ok_or_else(|| {
            format!(
                "{key:?} is not an exercise — pick a number, or name one from \
                 the vocabulary"
            )
        })
    })
}

/// A count the operator typed, as the domain's.
///
/// The questions refuse zero already; this is the conversion, and it fails only
/// where the two disagree about what a count may be.
fn count(value: u32) -> Result<RepCount, Failure> {
    RepCount::new(value).map_err(usage)
}

/// Refuse before asking anything if the store cannot hold what the answers make.
///
/// **And hand back what it found.** The questions need the parameters as well as
/// the store: what one step up the bar is, and so what "beat it" comes to, is
/// the plate grid in `scales` rather than anything to ask about.
pub async fn ready(
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

/// Ask which exercise fills a slot that carries its own sets and reps.
///
/// The plyometric and power blocks are set at the start of a block and read no
/// history, so they state their whole prescription and never alternate.
async fn ask_static(
    history: &SqliteExerciseHistory,
    slot: SlotId,
) -> Result<Fill<StaticFill>, Failure> {
    let offers = offered(history, slot).await?;
    let exercise = ask_exercise(slot, &offers)?;
    let sets = ask_until("  sets? [3] ", |typed| {
        if typed.is_empty() {
            return Ok(3);
        }
        parse_count("sets", typed)
    })?;
    let reps = ask_until("  reps? ", |typed| parse_count("reps", typed))?;
    Ok(Fill::Same(StaticFill {
        exercise,
        sets: count(sets)?,
        reps: count(reps)?,
    }))
}

/// Ask which exercise fills a slot that takes its prescription from the
/// parameters.
async fn ask_single(
    history: &SqliteExerciseHistory,
    slot: SlotId,
    pattern: PrimaryPattern,
    primary: Exercise,
) -> Result<Fill<Exercise>, Failure> {
    // **The primary pattern's slot is the primary lift.** Authoring refuses a
    // programme that names one exercise as its primary and fills that slot with
    // another, and rightly — the ladder and the slot would be climbing different
    // things. So it is stated rather than asked, and the operator cannot answer
    // his way into a programme that will not author.
    if slot == pattern.slot() {
        println!("\n{slot}");
        println!(
            "  {} — the primary, so it fills its own slot",
            primary.as_str()
        );
        return Ok(Fill::Same(primary));
    }

    let offers = offered(history, slot).await?;
    let exercise = ask_exercise(slot, &offers)?;

    // A hold is the authored duration on every session, so there is nothing to
    // alternate: asking would be offering a distinction that does not exist.
    if matches!(slot.block(), Block::Mobility) {
        return Ok(Fill::Same(exercise));
    }

    let alternates = ask("  a different one on the other session? [no] ")?;
    if matches!(alternates.to_lowercase().as_str(), "y" | "yes") {
        let other = ask_exercise(slot, &offers)?;
        return Ok(Fill::Alternating(PerRole {
            light: exercise,
            heavy: other,
        }));
    }

    Ok(Fill::Same(exercise))
}

/// Every slot, asked in [`SlotId::ALL`] order.
///
/// **A field per slot rather than a list of answers**, which is what
/// [`SlotFills`] is for: there is no assembly step that could leave one out, and
/// no lookup that could fail. Adding a slot to the template is a compile error
/// here until it is asked.
async fn ask_fills(
    history: &SqliteExerciseHistory,
    pattern: PrimaryPattern,
    primary: Exercise,
) -> Result<SlotFills, Failure> {
    Ok(SlotFills {
        plyometric: ask_static(history, SlotId::Plyometric).await?,
        power: ask_static(history, SlotId::Power).await?,
        knee_dominant: ask_single(history, SlotId::KneeDominant, pattern, primary).await?,
        upper_push: ask_single(history, SlotId::UpperPush, pattern, primary).await?,
        upper_pull: ask_single(history, SlotId::UpperPull, pattern, primary).await?,
        hip_dominant: ask_single(history, SlotId::HipDominant, pattern, primary).await?,
        biceps: ask_single(history, SlotId::Biceps, pattern, primary).await?,
        triceps: ask_single(history, SlotId::Triceps, pattern, primary).await?,
        wrist_flexion: ask_single(history, SlotId::WristFlexion, pattern, primary).await?,
        wrist_extension: ask_single(history, SlotId::WristExtension, pattern, primary).await?,
        core: ask_single(history, SlotId::Core, pattern, primary).await?,
        handstand_hold: ask_single(history, SlotId::HandstandHold, pattern, primary).await?,
        dead_hang: ask_single(history, SlotId::DeadHang, pattern, primary).await?,
        hip_flexor_stretch: ask_single(history, SlotId::HipFlexorStretch, pattern, primary).await?,
        hip_external_rotator_stretch: ask_single(
            history,
            SlotId::HipExternalRotatorStretch,
            pattern,
            primary,
        )
        .await?,
        hamstring_stretch: ask_single(history, SlotId::HamstringStretch, pattern, primary).await?,
        groin_stretch: ask_single(history, SlotId::GroinStretch, pattern, primary).await?,
    })
}

/// The patterns a block can be built around.
///
/// **Two, not four.** The ladder, the anchor and the entry test are all about a
/// lower-body maximum, and an upper push or pull is an accessory slot whichever
/// block it sits in. Stated by the operator, twice — the four-item list came
/// from treating `SlotId`'s patterns as interchangeable, which they are not.
const PATTERNS: [(PrimaryPattern, &str); 2] = [
    (PrimaryPattern::KneeDominant, "knee dominant"),
    (PrimaryPattern::HipDominant, "hip dominant"),
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

fn ask_pattern() -> Result<PrimaryPattern, Failure> {
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
            .map(|(pattern, _)| *pattern)
            .ok_or_else(|| format!("there is no {number} on the list"))
    })
}

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
fn ask_weekdays(diary: &Diary, start: Date) -> Result<(Weekdays, SessionRole), Failure> {
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

        // The calendar is needed here, before the programme exists, to work out
        // how many training weeks the operator's dates actually hold.
        let scheduled = Weekdays::new(
            chosen
                .iter()
                .map(|(_, weekday, role)| (*weekday, *role))
                .collect(),
        )
        .map_err(|error| usage(error.to_string()))?;

        // Gating is on the heavy session wherever there is one, which there
        // now is.
        return Ok((scheduled, SessionRole::Heavy));
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
            // Unreachable: an SBS cycle's length is the chart's, so `ask_climb`
            // never gets here. Answered rather than left to a wildcard, so that
            // adding a template forces a decision here.
            Climbing::Linear | Climbing::Sbs => available,
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
            // Unreachable for `Sbs`, as above: the chart's four weeks are not
            // negotiated with the calendar.
            Climbing::Linear | Climbing::Sbs => {
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
) -> Result<Anchor, Failure> {
    println!("\n{asks}");

    let Some(best) = best else {
        // Nothing to match and nothing to beat. The one remaining answer is
        // asked directly rather than offered as the only item on a list.
        println!("  nothing in the record for {lift}, so there is nothing to match");
        let declared = ask_until("  what should it aim at? ", declared_load)?;
        return anchor(declared, start, AnchorProvenance::Asserted);
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
        1 => anchor(best.maximum, best.on, AnchorProvenance::Estimated),
        // Asserted: nobody has lifted it. The date is still the performance's,
        // because that performance is what the assertion is reasoning from.
        2 => anchor(beaten, best.on, AnchorProvenance::Asserted),
        _ => {
            let declared = ask_until("  what should it aim at? ", declared_load)?;
            anchor(declared, start, AnchorProvenance::Asserted)
        }
    }
}

/// The anchor, once the load and where it came from are settled.
///
/// **No ceiling is ever asked for here.** `Anchor::failed` is what an entry test
/// *found*, and this is authored before the test is taken.
fn anchor(load: Kg, from: Date, provenance: AnchorProvenance) -> Result<Anchor, Failure> {
    Anchor::new(load, None, provenance, from).map_err(usage)
}

fn declared_load(typed: &str) -> Result<Kg, String> {
    if typed.is_empty() {
        return Err("the ramp aims at this, so there is no sensible default".to_owned());
    }
    Kg::try_from(typed.trim_end_matches("kg").to_owned()).map_err(|error| error.to_string())
}

/// Which of the three programmes this build can author.
///
/// **The wizard reached one of them until 2026-09-02.** It authored a block
/// whatever the operator wanted, and the only way to a test or a ladder was a
/// hand-written document — which is the input format it has now replaced
/// outright.
const TEMPLATES: [(&str, &str); 4] = [
    ("sbs", "Stronger By Science's four-week chart, twice a week"),
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
            .ok_or_else(|| format!("{typed:?} is not one of the four"))
    })
}

/// The questions every programme answers, whatever its template.
struct Common {
    plan: PlanName,
    start: Date,
    pattern: PrimaryPattern,
    scheduled: Weekdays,
    gating: SessionRole,
}

fn ask_common(diary: &Diary, template: &str) -> Result<Common, Failure> {
    // **The plan is what is named, not the mesocycle** (issue #86). A mesocycle
    // is the *n*th gym mesocycle of a plan; the name is the plan's, and
    // re-authoring under it is what corrects the plan as a whole.
    let plan = ask_until("which plan? ", |typed| {
        PlanName::try_from(typed.to_owned()).map_err(|error| {
            format!(
                "{error} — a plan is identified by its name, and this {template} \
                 goes into the plan you name here"
            )
        })
    })?;
    let start = ask_until("starts? ", parse_date)?;

    let pattern = ask_pattern()?;
    let (scheduled, gating) = ask_weekdays(diary, start)?;

    Ok(Common {
        plan,
        start,
        pattern,
        scheduled,
        gating,
    })
}

/// Who published this mesocycle, and which of its microcycles this is.
///
/// **Answered with defaults rather than looked up.** This build holds no
/// catalogue of published gym programmes — the chart is a table in `domain` and
/// the title is the operator's to state — so the questions carry the answers
/// settled on 2026-09-06 and take Return for each.
fn ask_provided(microcycles: Vec<u32>) -> Result<ProvidedFrom, Failure> {
    println!("\nwho published this, and which of its microcycles this is");
    let provider = ask_until("  provider? [Stronger By Science] ", |typed| {
        let typed = if typed.is_empty() {
            "Stronger By Science"
        } else {
            typed
        };
        Provider::try_from(typed.to_owned()).map_err(|error| error.to_string())
    })?;
    let programme = ask_until("  programme? [Squat 2x Int] ", |typed| {
        let typed = if typed.is_empty() {
            "Squat 2x Int"
        } else {
            typed
        };
        ProgrammeName::try_from(typed.to_owned()).map_err(|error| error.to_string())
    })?;
    ProvidedFrom::new(ExternalProgramme::new(provider, programme), microcycles)
        .map_err(|error| Failure::usage(&error))
}

/// **Asked after the duration, not before it.** The dates are what a programme
/// *is*; the lift is the first thing it is about, and the seventeen slots
/// follow it. Moving it in front of `ends?` puts an exercise between two dates.
///
/// **Offered rather than typed, since 2026-09-07.** The pattern narrows the
/// primary to three exercises knee-dominant or two hip-dominant, and until now
/// this took free text — so answering it needed the operator to already know
/// which lifts were acceptable, and a typo came back as "not in the
/// vocabulary" rather than as a list. A word is still read as a key, so a lift
/// off the list is one word away; what changed is that nothing has to be
/// remembered to answer.
pub fn ask_lift(template: &str, pattern: PrimaryPattern) -> Result<RepsExercise, Failure> {
    let offers = domain::prescription::candidates::for_primary(pattern);
    println!("\nthe lift this {template} is about");
    for (at, key) in offers.iter().enumerate() {
        println!("  {:>2}. {key}", at.saturating_add(1));
    }

    let question = format!(
        "  which? [{}] ",
        offers.first().copied().unwrap_or_default()
    );
    ask_until(&question, move |typed| {
        let key = if typed.is_empty() {
            (*offers
                .first()
                .ok_or_else(|| "nothing is offered; name an exercise".to_owned())?)
            .to_owned()
        } else if let Ok(number) = typed.parse::<usize>() {
            (*offers
                .get(number.wrapping_sub(1))
                .ok_or_else(|| format!("there is no {number} on the list"))?)
            .to_owned()
        } else {
            typed.to_owned()
        };
        match Exercise::named(&key) {
            Some(Exercise::Reps(exercise)) => Ok(exercise),
            Some(_) => Err(format!(
                "{key:?} is not counted in repetitions, and a primary needs one"
            )),
            None => Err(format!(
                "{key:?} is not an exercise — pick a number, or name one from \
                 the vocabulary"
            )),
        }
    })
}

async fn ask_programme(
    template: &'static str,
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<(PlanName, Authored), Failure> {
    match template {
        "test" => ask_test(diary, history, parameters).await,
        "sbs" => ask_climb(Climbing::Sbs, diary, history, parameters).await,
        "linear" => ask_climb(Climbing::Linear, diary, history, parameters).await,
        _ => ask_climb(Climbing::Block, diary, history, parameters).await,
    }
}

/// **A test is a week, so it is never asked how long it is.** `Shape::Test` has
/// nowhere to put a duration, and `Test::week` derives the calendar from the
/// start alone.
async fn ask_test(
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<(PlanName, Authored), Failure> {
    println!("A test: one week, measuring.\n");
    let common = ask_common(diary, "test")?;
    let lift = ask_lift("test", common.pattern)?;
    let primary = lift.as_str();

    let scale = parameters.scales.for_exercise(Exercise::Reps(lift));
    let best = best_of(history, lift, scale).await?;

    println!("\nwhat should the test aim at?");
    // **Inheriting is the ordinary case** (decision 0013): a test between two
    // programmes is for the load the progression stands at, and the programme
    // before it is what knows that. A declared target is for the case
    // inheritance cannot answer.
    let target = match best.as_ref() {
        Some(best) => {
            println!("  {}", best.describe(primary));
            println!("   1. the load the programme before this one stands at");
            println!("   2. a number of my own");
            let choice = ask_until("  which? [1] ", |typed| match typed {
                "" | "1" => Ok(1),
                "2" => Ok(2),
                other => Err(format!("{other:?} is not one of the two")),
            })?;
            if choice == 1 {
                TestTarget::Inherited
            } else {
                TestTarget::Declared(ask_until("  what should it aim at? ", declared_load)?)
            }
        }
        // Nothing performed, so there is nothing to describe and nothing a
        // predecessor could hand over either.
        None => TestTarget::Declared(ask_until("  what should it aim at? ", declared_load)?),
    };
    let reps = ask_until("  attempted at how many reps? [1] ", |typed| {
        if typed.is_empty() {
            return Ok(1);
        }
        parse_count("reps", typed)
    })?;

    // **A test may be published or may be nobody's.** *Squat 2x Int Entry Test*
    // is a week its publisher wrote; a week that exists only to measure a lift
    // before something else begins was written by nobody, and may not claim
    // otherwise.
    let published = ask_until("\n  was this week published by somebody? [y/N] ", yes_or_no)?;
    let provided = if published {
        let microcycle = ask_until("  which of its microcycles? [1] ", |typed| {
            if typed.is_empty() {
                return Ok(1);
            }
            typed
                .parse::<u32>()
                .ok()
                .filter(|number| *number > 0)
                .ok_or_else(|| format!("{typed:?} is not a microcycle number"))
        })?;
        Some(ask_provided(vec![microcycle])?)
    } else {
        None
    };

    Ok((
        common.plan,
        Authored {
            start: common.start,
            pattern: common.pattern,
            primary_exercise: Exercise::Reps(lift),
            weekdays: common.scheduled,
            shape: Shape::Test {
                reps: count(reps)?,
                target,
                provided,
            },
        },
    ))
}

fn yes_or_no(typed: &str) -> Result<bool, String> {
    match typed.to_lowercase().as_str() {
        "" | "n" | "no" => Ok(false),
        "y" | "yes" => Ok(true),
        other => Err(format!("{other:?} is not y or n")),
    }
}

/// Which climbing template, and so which week the span's first one is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Climbing {
    /// Every week it holds is a climbing week.
    Linear,
    /// The first week measures the anchor and is not a phase.
    Block,
    /// A published chart: four weeks, stated, with its test as the last session.
    Sbs,
}

async fn ask_climb(
    climbing: Climbing,
    diary: &Diary,
    history: &SqliteExerciseHistory,
    parameters: &GenerationParameters,
) -> Result<(PlanName, Authored), Failure> {
    let (template, asks_anchor) = match climbing {
        // **A ladder has no test week, so nothing aims at its anchor.** The
        // anchor is the maximum its percentages are shares of, and week one
        // climbs from it rather than measuring it.
        Climbing::Linear => ("ladder", "what does the ladder climb from?"),
        Climbing::Block => ("block", "what should the entry test aim at?"),
        // **What the chart's percentages are shares of, opening.** It does not
        // stay fixed: each week's rep-max day resets it.
        Climbing::Sbs => ("cycle", "what does week one programme from?"),
    };
    println!("A {template}: what it is, before what it contains.\n");
    let common = ask_common(diary, template)?;
    // **Never asked how long it is.** The chart is four weeks; offering the
    // question would invite an answer this build cannot prescribe, exactly as a
    // test is never asked its duration.
    let weeks = if matches!(climbing, Climbing::Sbs) {
        domain::prescription::sbs::WEEKS
    } else {
        ask_weeks(climbing, diary, common.start, &common.scheduled)?
    };
    let lift = ask_lift(template, common.pattern)?;

    let scale = parameters.scales.for_exercise(Exercise::Reps(lift));
    let best = best_of(history, lift, scale).await?;
    let anchor = ask_anchor(
        asks_anchor,
        lift.as_str(),
        best.as_ref(),
        scale,
        common.start,
    )?;

    let shape = match climbing {
        Climbing::Linear => {
            // **The anchor is where it opens unless the operator says
            // otherwise**, which is the `None` the assembly takes as "derive
            // it".
            let typed = ask("  and it opens at? [the anchor] ")?;
            let opening = if typed.is_empty() {
                None
            } else {
                Some(declared_load(&typed).map_err(usage)?)
            };
            Shape::Linear {
                gating: common.gating,
                weeks,
                anchor,
                opening,
            }
        }
        // The chart states every set it runs, so there is nothing to ask about
        // the shape — only about who published it.
        //
        // **Stated, because this command adds one cycle at a time.** The
        // operator running it has the predecessor's test behind him and a number
        // to give. `fitness plan` authors the whole chain months ahead, where
        // only the first cycle has one — the rest inherit.
        Climbing::Sbs => Shape::Provided {
            from: ask_provided((1..=SBS_MICROCYCLES).collect())?,
            anchor: Anchoring::Stated(Entry::derived(anchor)),
        },
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
            let light = if typed.is_empty() {
                None
            } else {
                Some(declared_load(&typed).map_err(usage)?)
            };
            Shape::Block {
                gating: common.gating,
                weeks,
                anchor,
                entry_test: Some(EntryTest::new(count(entry_reps)?, light).map_err(usage)?),
            }
        }
    };

    Ok((
        common.plan,
        Authored {
            start: common.start,
            pattern: common.pattern,
            primary_exercise: Exercise::Reps(lift),
            weekdays: common.scheduled,
            shape,
        },
    ))
}

/// Ask, and author.
///
/// **Nothing is written to disk.** The questions produced a TOML document until
/// 2026-09-06 and that document was then read back; what it carried is now
/// carried by [`Authored`], and the store is the only record.
///
/// **It writes a whole plan, not a mesocycle** (issue #86). The plan in force
/// under the name given is read, this mesocycle is put into its gym programme,
/// and the lot is re-authored — which is what lets `fitness plan` write the
/// cycling side and this command write the gym side of the same plan without
/// either superseding the other.
///
/// # Errors
///
/// [`Failure`] if there is nobody to ask, if the store holds no generation
/// parameters or no schedule, or if what the answers make is refused.
pub async fn add(database: &Path, zone: &OperatorZone) -> Result<(), Failure> {
    interactive()?;

    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let history = SqliteExerciseHistory::new(pool.clone());

    // **Asked before the first question, not discovered after the last.** A
    // programme cannot be authored against nothing (§ 14), and finding that out
    // at the end costs the operator every answer he has just given. Setting the
    // machine up is what puts them there; see `setup::seed_parameters`.
    let parameter_store = SqliteGenerationParameterStore::new(pool.clone());
    let parameters = ready(&parameter_store).await?;

    let diary = SqliteDiaryStore::new(pool.clone())
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let template = ask_template()?;
    let (name, answers) = ask_programme(template, &diary, &history, &parameters).await?;

    println!("\nAnd the slots. A number picks from the list; anything else is read");
    println!("as an exercise, so something you have never done is one word away.");

    let fills = ask_fills(&history, answers.pattern, answers.primary_exercise).await?;

    // **The days the gym loses, worked out here and recorded.** The schedule
    // knows when there is room to train and which slots are the gym's; the
    // programme is told its window and reads back what it loses. Resolved at
    // authoring so the stored programme is complete on its own — a holiday
    // coming off the calendar afterwards cannot retroactively move what it
    // prescribed.
    let interruptions = interruptions(&diary, &answers);

    let mesocycle = domain::prescription::authored::programme(
        answers,
        fills,
        &interruptions,
        zone.as_time_zone(),
        &parameters,
    )
    .map_err(|error| Failure::usage(&error))?;

    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());
    let existing = plans
        .named(&name)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let plan = with_gym(name, existing.as_ref(), mesocycle.clone())?;

    let (id, authored) = Authoring::new(
        plans,
        SqliteGymMesocycleStore::new(pool, zone.clone()),
        parameter_store,
    )
    .author(&plan, &parameters)
    .await
    .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::programme_authored(id, authored, &plan, &mesocycle, &parameters);
    Ok(())
}

/// The plan with this mesocycle in its gym programme.
///
/// **A mesocycle is placed by the day it starts.** Adding one is ordinarily
/// appending — the autumn's four go in one at a time — and re-running with a
/// start date already in the plan replaces that mesocycle, which is how a
/// mesocycle is corrected now that it has no name of its own to re-author under.
///
/// The cycling side is carried through untouched, so `fitness plan` and this
/// command can write the two halves of one plan in either order.
fn with_gym(
    name: PlanName,
    existing: Option<&Plan>,
    mesocycle: Mesocycle,
) -> Result<Plan, Failure> {
    let start = mesocycle.calendar().start();
    let mut gym: Vec<Mesocycle> = existing
        .and_then(Plan::gym)
        .map(|programme| {
            programme
                .mesocycles()
                .filter(|held| held.calendar().start() != start)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    gym.push(mesocycle);
    gym.sort_by_key(|held| held.calendar().start());

    Plan::new(
        name,
        jiff::Timestamp::now(),
        Some(Programme::new(gym).map_err(|error| Failure::usage(&error))?),
        existing.and_then(Plan::cycling).cloned(),
    )
    .map_err(|error| Failure::usage(&error))
}

/// What `plan` already knows before the gym questions begin.
///
/// A struct rather than five arguments because they arrive together and mean one
/// thing: the gym programme the operator has already chosen by picking a
/// provider, a lift and a start date.
pub struct GymOutline<'a> {
    pub start: Date,
    pub pattern: PrimaryPattern,
    pub lift: RepsExercise,
    /// Who published it, and under what name.
    pub published: &'a ExternalProgramme,
    /// Progressions after the entry test.
    pub progressions: usize,
    /// Which of the published programme's microcycles the entry test is. The
    /// programme's to say (issue #59), not the operator's to remember.
    pub test_microcycle: u32,
}

/// The week the plan opens on, measuring the lift the rest of it is about.
///
/// **The published programme's own**, not a generic week measuring a lift:
/// *Squat 2x Int* µ4 is a taper and a one-repetition maximum, which is what the
/// block opens on (issue #59).
///
/// Nothing precedes it in the plan, so there is nothing for its target to
/// inherit from: it is declared, from the record where the record speaks.
///
/// **Which microcycle it is, is the programme's to say and not a question.** It
/// asked until 2026-09-07, and there was only ever one answer: a test week is
/// the only thing an entry test can be derived from, so offering the choice
/// invited a wrong one. The operator: *"it doesn't make sense to ask which
/// microcycle to derive the entry test from because it can only be derived from
/// a test week."*
fn ask_entry_test(
    lift: RepsExercise,
    published: &ExternalProgramme,
    microcycle: u32,
    best: Option<&Best>,
) -> Result<Shape, Failure> {
    println!("\nthe entry test");
    let reps = ask_until("  attempted at how many reps? [1] ", |typed| {
        if typed.is_empty() {
            return Ok(1);
        }
        parse_count("reps", typed)
    })?;
    let target = match best {
        Some(best) => {
            println!("  {}", best.describe(lift.as_str()));
            println!(
                "   1. what the record stands at{:>13}",
                format!("{}kg", best.maximum)
            );
            println!("   2. a number of my own");
            let choice = ask_until("  which? [1] ", |typed| match typed {
                "" | "1" => Ok(1),
                "2" => Ok(2),
                other => Err(format!("{other:?} is not one of the two")),
            })?;
            if choice == 1 {
                TestTarget::Declared(best.maximum)
            } else {
                TestTarget::Declared(ask_until("  what should it aim at? ", declared_load)?)
            }
        }
        None => TestTarget::Declared(ask_until("  what should it aim at? ", declared_load)?),
    };

    Ok(Shape::Test {
        reps: count(reps)?,
        target,
        provided: Some(
            ProvidedFrom::new(published.clone(), vec![microcycle])
                .map_err(|error| Failure::usage(&error))?,
        ),
    })
}

/// The gym half of a plan: an entry test and the progressions after it.
///
/// **This is `plan` composing the wizard rather than restating it** (issue #73).
/// Every question here is one `programme add` already asks; what differs is that
/// they are asked once for the whole plan instead of once per mesocycle, and the
/// four mesocycles are laid out from the answers rather than typed in over three
/// months.
///
/// **Only the first progression states an anchor.** The rest open from the cycle
/// before them — week 4 day 2 is a one-repetition maximum, so each leaves the
/// next one's opening behind it — and those tests have not happened when the
/// plan is authored. Asking for them would be asking the operator to invent
/// three numbers; see [`Anchoring::Inherited`].
///
/// # Errors
///
/// [`Failure`] if the schedule gives the gym no days as of `start`, if there is
/// nobody to ask, or if the answers do not assemble into a mesocycle.
pub async fn gym_side(
    pool: &infrastructure::SqlitePool,
    zone: &OperatorZone,
    parameters: &GenerationParameters,
    outline: &GymOutline<'_>,
) -> Result<Vec<Mesocycle>, Failure> {
    let GymOutline {
        start,
        pattern,
        lift,
        published,
        progressions,
        test_microcycle,
    } = *outline;
    let history = SqliteExerciseHistory::new(pool.clone());
    let diary = SqliteDiaryStore::new(pool.clone())
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    println!("\nthe gym side: asked once, for the whole plan\n");
    let (weekdays, _) = ask_weekdays(&diary, start)?;

    println!("\nAnd the slots. A number picks from the list; anything else is read");
    println!("as an exercise, so something you have never done is one word away.");
    let fills = ask_fills(&history, pattern, Exercise::Reps(lift)).await?;

    let scale = parameters.scales.for_exercise(Exercise::Reps(lift));
    let best = best_of(&history, lift, scale).await?;

    let entry = ask_entry_test(lift, published, test_microcycle, best.as_ref())?;

    let mut shapes: Vec<(Date, Shape)> = vec![(start, entry)];
    for cycle in 0..progressions {
        let weeks = 1 + cycle * (SBS_MICROCYCLES as usize);
        let at = week_after(start, weeks)?;
        let numbers: Vec<u32> = (1..=SBS_MICROCYCLES).collect();
        shapes.push((
            at,
            Shape::Provided {
                from: ProvidedFrom::new(published.clone(), numbers)
                    .map_err(|error| Failure::usage(&error))?,
                // **Every one of them inherits, the first included.** It
                // stated an anchor until 2026-09-07, and the operator asked why
                // the entry test is given a target and then the mesocycle after
                // it is given the same number again. It is the same number: the
                // entry test is the week that measures it, and it runs before
                // this begins. Stating it here would be fixing what that test is
                // about to find out.
                anchor: Anchoring::Inherited,
            },
        ));
    }

    let mut mesocycles = Vec::new();
    for (at, shape) in shapes {
        let answers = Authored {
            start: at,
            pattern,
            primary_exercise: Exercise::Reps(lift),
            weekdays: weekdays.clone(),
            shape,
        };
        let skips = interruptions(&diary, &answers);
        mesocycles.push(
            domain::prescription::authored::programme(
                answers,
                fills.clone(),
                &skips,
                zone.as_time_zone(),
                parameters,
            )
            .map_err(|error| Failure::usage(&error))?,
        );
    }
    Ok(mesocycles)
}

/// The Monday `weeks` after this one.
fn week_after(date: Date, weeks: usize) -> Result<Date, Failure> {
    let weeks = i64::try_from(weeks).map_err(|_| usage("a plan longer than the calendar"))?;
    date.checked_add(jiff::Span::new().weeks(weeks))
        .map_err(|_| usage("a plan running past the end of the calendar"))
}

/// The days this programme's window loses, from the schedule.
///
/// Empty where nothing has been recorded about the operator's week, which is a
/// machine that has not run `fitness schedule add` yet — not a claim that the
/// block runs through everything. The window itself is
/// [`Authored::window`](domain::prescription::Authored::window), which is where
/// the reasoning about its span lives.
fn interruptions(diary: &Diary, answers: &Authored) -> Vec<Skip> {
    let Some((from, until)) = answers.window() else {
        return Vec::new();
    };
    diary
        .unavailable(from, until, Discipline::Gym)
        .into_iter()
        .map(Skip::day)
        .collect()
}
