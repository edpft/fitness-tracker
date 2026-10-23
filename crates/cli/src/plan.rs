//! Generating a hybrid programme: two providers, one span, one set of
//! constraints — and authoring the cycling side of it.
//!
//! **This is what the tool exists for.** A gym provider and a cycling provider
//! are each asked for mesocycles of a stated shape, and the answers are laid
//! against one another. A test week in one discipline falls on a test week or a
//! deload in the other (decision 0034).
//!
//! **Each provider supplies its own test microcycle from its own material**, and
//! neither borrows a generic one. SBS's is its microcycle four — the taper and
//! the one-repetition maximum. Peloton's is Build Your Power Zones's microcycle five —
//! the FTP warm-up and test pair with the endurance riding around it.
//!
//! **It asks rather than takes flags** (the operator, 2026-09-05: `plan` "needs
//! to be a wizard"), because choosing the providers, the primary lift and the
//! cycling pairing is not a thing to remember the spelling of.
//!
//! **It authors the cycling side of a plan, and only that.** Four mesocycles are
//! written — the FTP test microcycle, then the three of four — after which
//! `cycling next` needs no flags and reads no network.
//!
//! **The plan is written whole even so** (issue #86). What is already authored
//! under the name given is read, its cycling programme is replaced by these four
//! and its gym programme carried through untouched, and the lot is re-authored.
//! So this command and `fitness programme add` write the two halves of one plan
//! in either order, without either superseding the other — which is what closes
//! the half-written intermediate state #73 named, without merging the two
//! wizards.

use std::path::Path;

use application::{
    Authored, DiaryStore, PlanAuthor as _, PlanStore as _, PublishedCycling, PublishedRead,
    published::{self, BuildError},
};
use configurator::{access::Peloton, store::Store};
use domain::{
    cycling::{Answer, CyclingMesocycle},
    gym::exercise::RepsExercise,
    macrocycle::PROGRESSIONS,
    normalised::OperatorZone,
    plan::{Plan, PlanName, Programme},
    prescription::{PrimaryPattern, sbs},
    provider::{ExternalProgramme, ProgrammeName, Provider},
    schedule::Discipline,
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, wizard};

/// A gym provider this build holds, and what choosing it settles.
///
/// **Choosing the programme chooses the pattern.** Stronger By Science publish a
/// two-day intermediate per pattern — *Squat 2x Int* and a deadlift programme
/// this build does not hold — so the pattern is not a question of its own here,
/// and asking it would invite an answer contradicting the programme just chosen.
/// The operator, 2026-09-07: *"The SBS programme is, in theory, squat pattern
/// specific, they have a different intermediate 2-day deadlift programme, so, by
/// choosing it, I'm effectively choosing a squat pattern."*
struct GymProvider {
    label: &'static str,
    provider: &'static str,
    /// The publisher's own name for it, which is what the layout says. Not
    /// `sbs-n`: a name below the plan is the publisher's (issue #86).
    programme: &'static str,
    pattern: PrimaryPattern,
    /// Which of its microcycles the entry test is taken from. *Squat 2x Int* µ4
    /// is a taper and a one-repetition maximum, which is what a block opens on.
    test_microcycle: u32,
}

/// The gym providers this build holds.
const GYM_PROVIDERS: [GymProvider; 1] = [GymProvider {
    label: "Stronger By Science, Squat 2x Int",
    provider: "Stronger By Science",
    programme: "Squat 2x Int",
    pattern: PrimaryPattern::KneeDominant,
    test_microcycle: sbs::WEEKS,
}];

/// One cycling mesocycle, named, with what it answers.
struct Offered<'a> {
    name: String,
    own: usize,
    answer: Option<Answer>,
    from: &'a PublishedRead,
}

/// Each mesocycle of a programme, named as the wizard lists it.
fn offered(read: &PublishedRead, microcycles: usize, sessions: usize) -> Vec<Offered<'_>> {
    published::mesocycles_of(read, microcycles, sessions)
        .into_iter()
        .map(|one| Offered {
            name: format!("{} {}", read.from.name(), one.index + 1),
            own: one.own,
            answer: one.answer,
            from: read,
        })
        .collect()
}

/// The pairings a cycling provider offers over three mesocycles.
#[derive(Clone, Copy)]
struct Pairing {
    label: &'static str,
    /// The two programmes, as the wizard says them.
    names: &'static str,
    programmes: [PublishedCycling; 2],
}

const PAIRINGS: [Pairing; 2] = [
    Pairing {
        label: "Boost Your Base, then Build Your Power Zones",
        names: "Boost Your Base and Build Your Power Zones",
        programmes: [PublishedCycling::Base, PublishedCycling::Build],
    },
    Pairing {
        label: "Build Your Power Zones, then Peak Your Power Zones",
        names: "Build Your Power Zones and Peak Your Power Zones",
        programmes: [PublishedCycling::Build, PublishedCycling::Peak],
    },
];

/// Ask what the programme is made of, generate it, and offer to author it.
///
/// # Errors
///
/// [`Failure`] if there is nobody to ask, if a class the catalogue has not read
/// cannot be fetched, or if the store refuses what would be authored.
///
/// **The credentials are no longer required** (#246). Every class the skeletons
/// place is in the local catalogue, and the refresh inside `fitness next` reads
/// those before any other — so authoring reads the store and contacts Peloton
/// only for a class it has not reached yet.
pub async fn generate(
    database: &Path,
    zone: &OperatorZone,
    microcycles: usize,
    sessions: usize,
    credentials: &configurator::Credentials,
) -> Result<(), Failure> {
    wizard::interactive()?;

    // **Resolved before a single question is asked, and no longer fatal.**
    // Every answer below used to be spent reading Peloton, so a missing login
    // found afterwards wasted the whole interrogation. Since #246 the classes
    // come from the catalogue, so an absent credential costs only a class the
    // catalogue has not read — but it is still resolved up front, because that
    // is when it would be cheapest to say so.
    let peloton = configurator::access::peloton(credentials).ok();

    println!("A hybrid programme: one gym provider, one cycling provider.\n");
    let start = wizard::ask_until("Monday the block begins (YYYY-MM-DD): ", |typed| {
        typed
            .parse::<Date>()
            .map_err(|_| format!("{typed:?} is not a date — try 2026-09-14"))
    })?;
    let labels = GYM_PROVIDERS.map(|one| one.label);
    let gym = GYM_PROVIDERS
        .get(choose("Gym provider", &labels)?)
        .ok_or_else(|| Failure::message("no gym provider chosen", exit::USAGE))?;
    let lift = wizard::ask_lift(gym.programme, gym.pattern)?;
    let pairing = PAIRINGS
        .get(choose("Cycling pairing", &PAIRINGS.map(|one| one.label))?)
        .copied()
        .ok_or_else(|| Failure::message("no pairing chosen", exit::USAGE))?;

    // The days are read before anything is fetched: a schedule that says
    // nothing about the start date makes the whole run pointless, and finding
    // that out after a minute of requests is a minute wasted.
    let store = Store::open(database).await?;
    let riding_days = cycling_weekdays(&store.diary(), start).await?;

    // **The count, not the read count.** How many classes have had their
    // detail read is not what matters here: the refresh reads the classes the
    // skeletons place before any other, so a catalogue with rows in it answers
    // this authoring whatever its overall progress. An *empty* catalogue is the
    // one case worth saying out loud, because then every class is a request.
    let listed = configurator::cycling::listed(&store)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let programmes = pairing.names;
    if listed == 0 {
        println!(
            "\nthe class catalogue is empty, so {programmes} are read from Peloton. \
             Running fitness next fills it"
        );
    } else {
        println!("\nreading {programmes} from the local catalogue of {listed} classes");
    }

    let peloton = peloton.as_ref();
    let mut read = Vec::new();
    for which in pairing.programmes {
        read.push(fetch(&store, peloton, which).await?);
    }
    // **The test programme is read whichever pairing is chosen**, because it is
    // a programme in its own right rather than a microcycle borrowed from one of
    // them. Every cycling block opens by measuring FTP.
    let testing = fetch(&store, peloton, PublishedCycling::Test).await?;

    let offered: Vec<Offered> = read
        .iter()
        .flat_map(|one| offered(one, microcycles, sessions))
        .collect();
    let test = published::test_microcycle(&testing, sessions);

    report(
        &offered,
        gym,
        lift.name(),
        start,
        test.is_some(),
        microcycles,
    );

    let taken: Vec<&Offered> = offered
        .iter()
        .filter(|one| one.answer.is_some())
        .take(3)
        .collect();
    author(
        &store,
        zone,
        start,
        gym,
        lift,
        &riding_days,
        test.as_ref()
            .map(|sessions| (&testing, sessions.as_slice())),
        &taken,
    )
    .await
}

/// Which weekdays the schedule gives cycling, as of the start date.
///
/// **The schedule says which days are cycling's, and the programme says which
/// session each rides.** That is the same split the gym wizard uses: the diary
/// offers the discipline's ordinary days, and what runs on them is the
/// programme's to fix.
pub async fn cycling_weekdays(
    diary: &(impl DiaryStore + Sync),
    start: Date,
) -> Result<Vec<Weekday>, Failure> {
    let diary = diary
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;

    let Some(days) = diary.ordinarily(start, Discipline::Cycling) else {
        return Err(Failure::message(
            format!(
                "the schedule says nothing about {start}, so there is no way to know \
                 which days are cycling's. Record the week first: fitness schedule add"
            ),
            exit::USAGE,
        ));
    };
    if days.is_empty() {
        return Err(Failure::message(
            format!(
                "the schedule gives cycling no day of the week as of {start}, so \
                 there is nothing for a programme to run on"
            ),
            exit::USAGE,
        ));
    }
    Ok(days)
}

/// Offer a numbered list and take one.
fn choose(question: &str, options: &[&str]) -> Result<usize, Failure> {
    println!("{question}:");
    for (at, option) in options.iter().enumerate() {
        println!("  {}. {option}", at + 1);
    }
    let count = options.len();
    wizard::ask_until("  choose: ", move |typed| {
        typed
            .parse::<usize>()
            .ok()
            .filter(|chosen| (1..=count).contains(chosen))
            .map(|chosen| chosen - 1)
            .ok_or_else(|| format!("choose a number from 1 to {count}"))
    })
}

/// Read one published programme: every class it places, and what it trains.
async fn fetch(
    store: &Store,
    peloton: Option<&Peloton>,
    which: PublishedCycling,
) -> Result<PublishedRead, Failure> {
    configurator::cycling::published(store, peloton, which)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))
}

/// A mesocycle of the programme read, or why it would not build.
fn build(
    start: Date,
    read: &PublishedRead,
    microcycles: &[u32],
    sessions: &[u32],
    riding_days: &[Weekday],
) -> Result<CyclingMesocycle, Failure> {
    published::build(start, read, microcycles, sessions, riding_days).map_err(|error| match error {
        BuildError::TooFewDays { .. } => {
            Failure::message(format!("{error}: fitness schedule add"), exit::USAGE)
        }
        BuildError::Source(_) => Failure::message(error.to_string(), exit::SOURCE),
        BuildError::Unbuildable { .. } => Failure::usage(&error),
    })
}

/// Write the plan: both programmes, and every mesocycle in them.
///
/// **Both halves or neither** (issue #73). It printed thirteen weeks and wrote
/// the cycling five of them until 2026-09-07, leaving the gym side to
/// `fitness programme add` afterwards — which meant the command that exists to
/// author a hybrid plan authored half of one. The gym questions are the wizard's
/// own, asked through [`wizard::gym_side`] rather than restated here.
#[expect(clippy::too_many_arguments, reason = "one plan's worth of answers")]
async fn author(
    store: &Store,
    zone: &OperatorZone,
    start: Date,
    provider: &GymProvider,
    lift: RepsExercise,
    riding_days: &[Weekday],
    test: Option<(&PublishedRead, &[u32])>,
    taken: &[&Offered<'_>],
) -> Result<(), Failure> {
    println!();
    let write =
        wizard::ask_until(
            "Author this plan — both programmes? [y/N] ",
            |typed| match typed.to_lowercase().as_str() {
                "" | "n" | "no" => Ok(false),
                "y" | "yes" => Ok(true),
                other => Err(format!("{other:?} is not y or n")),
            },
        )?;
    if !write {
        println!("  nothing written.");
        return Ok(());
    }

    // **The plan is named, not the mesocycles** (issue #86). It was a stem here
    // until 2026-09-06 — `autumn-cycling-1`, `-2`, `-3` — because four rows
    // needed four names and nothing held them together.
    let name = wizard::ask_until("Which plan do these belong to? ", |typed| {
        PlanName::try_from(typed.to_owned())
            .map_err(|error| format!("{error} — a plan is identified by its name"))
    })?;

    // **Before the first question, not after the last.** A plan cannot be
    // authored against no parameters (§ 14), and finding that out at the end
    // costs every answer just given.
    let parameter_store = store.parameters();
    let parameters = wizard::ready(&parameter_store).await?;

    let published = ExternalProgramme::new(
        Provider::try_from(provider.provider.to_owned()).map_err(|error| Failure::usage(&error))?,
        ProgrammeName::try_from(provider.programme.to_owned())
            .map_err(|error| Failure::usage(&error))?,
    );
    let gym_side = wizard::gym_side(
        store,
        zone,
        &parameters,
        &wizard::GymOutline {
            start,
            pattern: provider.pattern,
            lift,
            published: &published,
            progressions: PROGRESSIONS,
            test_microcycle: provider.test_microcycle,
        },
    )
    .await?;

    let mut at = start;
    let mut mesocycles = Vec::new();

    if let Some((read, sessions)) = test {
        mesocycles.push(build(at, read, &[1], sessions, riding_days)?);
        at = week_after(at, 1)?;
    }
    for one in taken {
        let Some(answer) = &one.answer else { continue };
        mesocycles.push(build(
            at,
            one.from,
            &answer.microcycles,
            &answer.sessions,
            riding_days,
        )?);
        at = week_after(at, answer.microcycles.len())?;
    }

    let reported: Vec<(String, usize, Date)> = mesocycles
        .iter()
        .map(|mesocycle| {
            (
                mesocycle.provenance().to_string(),
                mesocycle.duration_weeks(),
                mesocycle.start(),
            )
        })
        .collect();

    let plans = store.plans(zone);
    let existing = plans
        .named(&name)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let plan = with_both(name, existing.as_ref(), gym_side.clone(), mesocycles)?;

    let (_, authored) = application::prescribe::Authoring::new(plans, parameter_store)
        .author(&plan, &parameters)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    println!();
    let verb = match authored {
        Authored::Created => "authored",
        Authored::Modified => "re-authored",
    };
    println!("  {verb} {}:", plan.name());
    println!("    gym");
    for mesocycle in &gym_side {
        println!(
            "      {}: {} weeks from {}",
            mesocycle.template(),
            mesocycle.calendar().duration_weeks(),
            mesocycle.calendar().start()
        );
    }
    println!("    cycling");
    for (from, weeks, on) in &reported {
        println!("      {from}: {weeks} weeks from {on}");
    }
    println!("\n  fitness prescribe and fitness cycling next both answer now.");
    Ok(())
}

/// The plan with these mesocycles as its two programmes.
///
/// **The cycling side is replaced whole, and the gym side carried through.**
/// This command generates the four together — a test week and three mesocycles
/// laid against one another — so there is no sense in which one of them could be
/// amended alone. What must survive is the gym programme, which
/// `fitness programme add` wrote and this command knows nothing about.
fn with_both(
    name: PlanName,
    existing: Option<&Plan>,
    gym: Vec<domain::prescription::GymMesocycle>,
    mesocycles: Vec<CyclingMesocycle>,
) -> Result<Plan, Failure> {
    // **Both sides replaced, since this command now authors both.** It carried
    // the gym programme through until 2026-09-07, when it wrote only cycling and
    // `programme add` wrote the other half; re-running it then would leave a gym
    // programme from an older answer beside a cycling one from this run. What
    // `programme add` adds afterwards still survives — it reads the plan back
    // and appends to whichever half it is adding to.
    let _ = existing;
    Plan::new(
        name,
        jiff::Timestamp::now(),
        Some(Programme::new(gym).map_err(|error| Failure::usage(&error))?),
        Some(Programme::new(mesocycles).map_err(|error| Failure::usage(&error))?),
    )
    .map_err(|error| Failure::usage(&error))
}

fn week_after(date: Date, weeks: usize) -> Result<Date, Failure> {
    let weeks = i64::try_from(weeks)
        .map_err(|_| Failure::message("a mesocycle longer than the calendar", exit::USAGE))?;
    date.checked_add(jiff::Span::new().weeks(weeks))
        .map_err(|error| Failure::usage(&error))
}

fn report(
    offered: &[Offered],
    gym: &GymProvider,
    lift: &str,
    start: Date,
    tests: bool,
    microcycles: usize,
) {
    println!("\nwhat each cycling mesocycle answers\n");
    println!(
        "  {:<26}{:<10}{:<18}{:>7}{:>8}",
        "mesocycle", "own", "answers", "compos", "span"
    );
    for one in offered {
        let own = format!("{} × 3", one.own);
        match &one.answer {
            Some(answer) => println!(
                "  {:<26}{own:<10}{:<18}{:>7.1}{:>8}",
                one.name,
                format!(
                    "µ{} by {}",
                    join(&answer.microcycles, "-"),
                    join(&answer.sessions, "+")
                ),
                answer.composition,
                answer
                    .span
                    .map_or_else(|| "—".to_owned(), |ratio| format!("{ratio:.2}×")),
            ),
            None => println!("  {:<26}{own:<10}{:<18}", one.name, "nothing"),
        }
    }

    println!("\n{}, {} — {lift}\n", gym.provider, gym.programme);
    println!("  {:>2}  {:<12}{:<32}cycling", "µ", "w/c", "gym");
    // **A standalone programme on both sides**, and neither row names a
    // microcycle of the block that follows it: the gym's is a `test` programme
    // and cycling's is *Power Zone test*.
    let test = if tests {
        format!("{} µ1 — the FTP test", configurator::rides::TEST_PROGRAMME)
    } else {
        "—".to_owned()
    };
    println!("   0  {start}  {:<32}{test}", "Entry test — the 1RM test");

    let usable: Vec<&Offered> = offered.iter().filter(|one| one.answer.is_some()).collect();
    for (cycle, one) in usable.iter().take(3).enumerate() {
        let Some(answer) = &one.answer else { continue };
        for index in 0..answer.microcycles.len() {
            let ordinal = cycle * sbs::WEEKS as usize + index + 1;
            let date =
                start.saturating_add(jiff::Span::new().weeks(i64::try_from(ordinal).unwrap_or(0)));
            let within = index + 1;
            let named = format!("{} {} µ{within}", gym.programme, cycle + 1);
            let named = if within == sbs::WEEKS as usize {
                format!("{named} — 1RM test")
            } else {
                named
            };
            // **The cycling column marks its tests too, since 2026-09-07.** The
            // gym's 1RM lands on every µ4 and was called out; an FTP test falls
            // wherever the chosen microcycles happen to include one — Build's µ5
            // is one, and a pairing taking µ1-2-4-5 rides it in its fourth week
            // — and went unmarked. The flag is the class's own, the same one the
            // standalone test week is assembled from.
            // **Our microcycle number, not the publisher's.** An answer of
            // Peak's µ5-6-7-8 is our mesocycle's µ1-4: the external programme is
            // one eight-microcycle block and splitting it into two mesocycles is
            // ours, so numbering the second from five would say the mesocycle
            // begins four weeks into itself. `cycling_microcycle` has carried
            // both numbers since 0024 — `ordinal` ours, `published_ordinal`
            // theirs — and this printed the wrong one until 2026-09-07. What
            // they answer is the table above, which still names them.
            let ridden = answer.microcycles.get(index).copied().unwrap_or_default();
            let riding = if published::measures(one.from, ridden, &answer.sessions) {
                format!("{} µ{within} — the FTP test", one.name)
            } else {
                format!("{} µ{within}", one.name)
            };
            println!("  {ordinal:>2}  {date}  {named:<32}{riding}");
        }
    }

    if microcycles != sbs::WEEKS as usize {
        println!(
            "\n  ! {microcycles}-microcycle mesocycles against 4-microcycle SBS cycles \
             will not line up"
        );
    }
}

fn join(items: &[u32], between: &str) -> String {
    items
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(between)
}
