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
//! the one-repetition maximum. Peloton's is Power Zone Build's microcycle five —
//! the FTP warm-up and test pair with the endurance riding around it.
//!
//! **It asks rather than takes flags** (the operator, 2026-09-05: `plan` "needs
//! to be a wizard"), because choosing the providers, the primary lift and the
//! cycling pairing is not a thing to remember the spelling of.
//!
//! **It authors the cycling side, and only that.** Four programmes are written —
//! the FTP test microcycle, then the three mesocycles — after which
//! `cycling next` needs no flags and reads no network. The gym side is still
//! authored by `fitness programme add`; composing that wizard in here is issue
//! #73, and until it lands this command deliberately writes half of what it
//! prints.

use std::path::Path;

use application::{Authored, DiaryStore as _};
use domain::{
    cycling::{
        Answer, CyclingMicrocycle, CyclingProgramme, CyclingWeekdays, PlannedRide, Programme,
        PublishedMicrocycle, SessionPosition,
    },
    gym::sequence::NonEmpty,
    prescription::ProgrammeName,
    schedule::Discipline,
};
use infrastructure::{
    SqliteCyclingProgrammeStore, SqliteDiaryStore,
    peloton::{
        auth::{PelotonAuth, PelotonCredentials},
        class::PelotonClasses,
        provider::{self, Fetched},
        skeleton,
    },
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, wizard};

const AUTH_BASE: &str = "https://auth.onepeloton.com";
const API_BASE: &str = "https://api.onepeloton.com";

/// The gym providers this build holds.
const GYM_PROVIDERS: [&str; 1] = ["Stronger By Science, two-day intermediate"];

/// Microcycles in a mesocycle, and where the test sits in an SBS one.
const SBS_MICROCYCLES: usize = 4;

/// One published programme, read once.
struct Read {
    name: &'static str,
    published: ProgrammeName,
    fetched: Fetched,
    programme: Programme,
}

/// One cycling mesocycle, named, with what it answers.
struct Offered<'a> {
    name: String,
    own: usize,
    answer: Option<Answer>,
    from: &'a Read,
}

/// The pairings a cycling provider offers over three mesocycles.
#[derive(Clone, Copy)]
struct Pairing {
    label: &'static str,
    programmes: [&'static str; 2],
}

const PAIRINGS: [Pairing; 2] = [
    Pairing {
        label: "Boost Your Base, then Power Zone Build",
        programmes: ["Boost Your Base", "Power Zone Build"],
    },
    Pairing {
        label: "Power Zone Build, then Peak Your Power Zones",
        programmes: ["Power Zone Build", "Peak Your Power Zones"],
    },
];

/// Ask what the programme is made of, generate it, and offer to author it.
///
/// # Errors
///
/// [`Failure`] if there is nobody to ask, if the credentials are absent, if
/// Peloton will not answer, or if the store refuses what would be authored.
pub async fn generate(database: &Path, microcycles: usize, sessions: usize) -> Result<(), Failure> {
    wizard::interactive()?;

    println!("A hybrid programme: one gym provider, one cycling provider.\n");
    let start = wizard::ask_until("Monday the block begins (YYYY-MM-DD): ", |typed| {
        typed
            .parse::<Date>()
            .map_err(|_| format!("{typed:?} is not a date — try 2026-09-14"))
    })?;
    let gym = GYM_PROVIDERS
        .get(choose("Gym provider", &GYM_PROVIDERS)?)
        .copied()
        .unwrap_or("Stronger By Science, two-day intermediate");
    let lift = wizard::ask_until("Primary lift: ", |typed| {
        if typed.is_empty() {
            Err("a programme trains something — name the lift".to_owned())
        } else {
            Ok(typed.to_owned())
        }
    })?;
    let pairing = PAIRINGS
        .get(choose("Cycling pairing", &PAIRINGS.map(|one| one.label))?)
        .copied()
        .ok_or_else(|| Failure::message("no pairing chosen", exit::USAGE))?;

    // The days are read before anything is fetched: a schedule that says
    // nothing about the start date makes the whole run pointless, and finding
    // that out after a minute of requests is a minute wasted.
    let pool = infrastructure::connect(database).await?;
    let diary = SqliteDiaryStore::new(pool.clone());
    let riding_days = cycling_weekdays(&diary, start).await?;

    let classes = credentials()?;
    println!(
        "\nreading {} from Peloton",
        pairing.programmes.join(" and ")
    );

    let mut read = Vec::new();
    for name in pairing.programmes {
        read.push(fetch(&classes, name).await?);
    }
    // **The test programme is read whichever pairing is chosen**, because it is
    // a programme in its own right rather than a microcycle borrowed from one of
    // them. Every cycling block opens by measuring FTP.
    let testing = fetch(&classes, skeleton::POWER_ZONE_TEST).await?;

    let mut offered = Vec::new();
    for one in &read {
        offered.extend(mesocycles_of(one, microcycles, sessions));
    }
    let test = test_microcycle(&testing, sessions);

    report(&offered, gym, &lift, start, test.is_some(), microcycles);

    let taken: Vec<&Offered> = offered
        .iter()
        .filter(|one| one.answer.is_some())
        .take(3)
        .collect();
    let store = SqliteCyclingProgrammeStore::new(pool);
    author(
        &store,
        start,
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
async fn cycling_weekdays(diary: &SqliteDiaryStore, start: Date) -> Result<Vec<Weekday>, Failure> {
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

fn credentials() -> Result<PelotonClasses, Failure> {
    let missing = |name: &str| Failure::message(format!("{name} is not set"), exit::USAGE);
    let email = std::env::var("PELOTON_EMAIL").map_err(|_| missing("PELOTON_EMAIL"))?;
    let password = std::env::var("PELOTON_PASSWORD").map_err(|_| missing("PELOTON_PASSWORD"))?;
    Ok(PelotonClasses::new(
        API_BASE,
        PelotonAuth::new(AUTH_BASE, PelotonCredentials::new(email, password)),
    ))
}

fn placements(name: &str) -> Result<Vec<skeleton::Placement>, Failure> {
    match name {
        "Boost Your Base" => Ok(skeleton::BOOST_YOUR_BASE.placements().to_vec()),
        "Power Zone Build" => Ok(skeleton::POWER_ZONE_BUILD.placements().to_vec()),
        "Peak Your Power Zones" => Ok(skeleton::peak_your_power_zones()),
        skeleton::POWER_ZONE_TEST => Ok(skeleton::power_zone_test()),
        other => Err(Failure::message(
            format!("this build holds no skeleton for {other:?}"),
            exit::USAGE,
        )),
    }
}

/// Read one published programme: every class it places, and what it trains.
async fn fetch(classes: &PelotonClasses, name: &'static str) -> Result<Read, Failure> {
    let placements = placements(name)?;
    let fetched = provider::fetch(classes, &placements)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
    let programme = provider::programme(&fetched);
    Ok(Read {
        name,
        published: ProgrammeName::try_from(name).map_err(|error| Failure::usage(&error))?,
        fetched,
        programme,
    })
}

/// Split a programme and ask each mesocycle for the shape wanted.
fn mesocycles_of(read: &Read, microcycles: usize, sessions: usize) -> Vec<Offered<'_>> {
    read.programme
        .mesocycles()
        .into_iter()
        .enumerate()
        .map(|(at, mesocycle)| Offered {
            name: format!("{} {}", read.name, at + 1),
            own: mesocycle.len(),
            answer: read
                .programme
                .answer(&mesocycle, microcycles, sessions)
                .into_iter()
                .next(),
            from: read,
        })
        .collect()
}

/// Which of the test programme's sessions are ridden.
///
/// **The FTP test is taken, and the rest of the week fills up around it.** A
/// test week that dropped the session measuring FTP would be a week of ordinary
/// riding wearing the name.
///
/// The microcycle is the programme's own first and only one, so there is nothing
/// to choose there — which is the point of it being a programme rather than a
/// microcycle of Build.
fn test_microcycle(read: &Read, sessions: usize) -> Option<Vec<u32>> {
    let number = read.programme.microcycles().first().copied()?;

    let measures = |session: u32| {
        read.fetched
            .get(&(number, session))
            .is_some_and(|classes| classes.iter().any(|class| class.is_ftp_test))
    };
    let mut taken: Vec<u32> = read
        .programme
        .sessions()
        .into_iter()
        .filter(|session| measures(*session))
        .collect();
    for session in read.programme.sessions() {
        if taken.len() >= sessions {
            break;
        }
        if !taken.contains(&session) {
            taken.push(session);
        }
    }
    taken.sort_unstable();
    Some(taken)
}

/// Write the cycling side: the test microcycle, then the three mesocycles.
async fn author(
    store: &SqliteCyclingProgrammeStore,
    start: Date,
    riding_days: &[Weekday],
    test: Option<(&Read, &[u32])>,
    taken: &[&Offered<'_>],
) -> Result<(), Failure> {
    println!();
    let write = wizard::ask_until(
        "Author the cycling side of this? [y/N] ",
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

    let stem = wizard::ask_until(
        "Name these programmes (a stem, e.g. autumn-cycling): ",
        |typed| {
            if typed.is_empty() {
                Err("a programme is identified by its name — give a stem".to_owned())
            } else {
                Ok(typed.to_owned())
            }
        },
    )?;

    let authored_at = jiff::Timestamp::now();
    let mut at = start;
    let mut written = Vec::new();

    if let Some((read, sessions)) = test {
        let programme = build(
            &format!("{stem}-test"),
            authored_at,
            at,
            read,
            &[1],
            sessions,
            riding_days,
        )?;
        at = week_after(at, 1)?;
        written.push(record(store, &programme).await?);
    }

    for (cycle, one) in taken.iter().enumerate() {
        let Some(answer) = &one.answer else { continue };
        let programme = build(
            &format!("{stem}-{}", cycle + 1),
            authored_at,
            at,
            one.from,
            &answer.microcycles,
            &answer.sessions,
            riding_days,
        )?;
        at = week_after(at, answer.microcycles.len())?;
        written.push(record(store, &programme).await?);
    }

    println!();
    for (name, weeks, from, authored) in &written {
        let verb = match authored {
            Authored::Created => "authored",
            Authored::Modified => "re-authored",
        };
        println!("  {verb} {name}: {weeks} weeks from {from}");
    }
    println!("\n  fitness cycling next now answers without a start date.");
    Ok(())
}

async fn record(
    store: &SqliteCyclingProgrammeStore,
    programme: &CyclingProgramme,
) -> Result<(String, usize, Date, Authored), Failure> {
    let (_, authored) = application::cycling::author(store, programme)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;
    Ok((
        programme.name().to_string(),
        programme.duration_weeks(),
        programme.start(),
        authored,
    ))
}

fn week_after(date: Date, weeks: usize) -> Result<Date, Failure> {
    let weeks = i64::try_from(weeks)
        .map_err(|_| Failure::message("a mesocycle longer than the calendar", exit::USAGE))?;
    date.checked_add(jiff::Span::new().weeks(weeks))
        .map_err(|error| Failure::usage(&error))
}

/// Turn an answer into a programme the store can hold.
///
/// **Both axes are renumbered from one and both keep their provenance.** An
/// answer of µ1-2-4-5 by sessions 1+3 authors four microcycles of two rides, and
/// each says which published microcycle and which published session it came
/// from. The operator rides a first and a second session in the week; that they
/// are the published first and third is a fact about where they were taken from,
/// not what to call them.
///
/// The sessions are mapped onto the schedule's cycling days in the order both
/// are given: the week's earlier ride to the week's earlier day.
fn build(
    name: &str,
    authored_at: jiff::Timestamp,
    start: Date,
    read: &Read,
    microcycles: &[u32],
    sessions: &[u32],
    riding_days: &[Weekday],
) -> Result<CyclingProgramme, Failure> {
    if riding_days.len() < sessions.len() {
        return Err(Failure::message(
            format!(
                "the schedule gives cycling {} day(s) a week and this answer rides {} — \
                 record the week the block is trained in first: fitness schedule add",
                riding_days.len(),
                sessions.len(),
            ),
            exit::USAGE,
        ));
    }

    // This programme's own numbering: the first ride of the week is session one
    // whichever published session it was taken from.
    let positions = (1..=sessions.len())
        .map(|ordinal| {
            u8::try_from(ordinal)
                .ok()
                .and_then(|number| SessionPosition::new(number).ok())
                .ok_or_else(|| Failure::message("more rides than a week can hold", exit::USAGE))
        })
        .collect::<Result<Vec<_>, Failure>>()?;

    let mut weeks = Vec::with_capacity(microcycles.len());
    for number in microcycles {
        let mut rides = std::collections::BTreeMap::new();
        for (position, session) in positions.iter().zip(sessions) {
            let classes = read.fetched.get(&(*number, *session)).ok_or_else(|| {
                Failure::message(
                    format!(
                        "nothing was read for {} µ{number} session {session}",
                        read.name
                    ),
                    exit::SOURCE,
                )
            })?;
            let (ride, at) = provider::session((*number, *session), classes)
                .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
            rides.insert(*position, PlannedRide::new(ride, at, *session));
        }
        weeks.push(
            CyclingMicrocycle::new(
                rides,
                PublishedMicrocycle::new(read.published.clone(), *number),
            )
            .map_err(|error| Failure::usage(&error))?,
        );
    }

    let weekdays = CyclingWeekdays::new(
        riding_days
            .iter()
            .copied()
            .zip(positions.iter().copied())
            .collect(),
    )
    .map_err(|error| Failure::usage(&error))?;

    CyclingProgramme::new(
        ProgrammeName::try_from(name).map_err(|error| Failure::usage(&error))?,
        authored_at,
        start,
        NonEmpty::new(weeks).map_err(|error| Failure::usage(&error))?,
        weekdays,
    )
    .map_err(|error| Failure::usage(&error))
}

fn report(
    offered: &[Offered],
    gym: &str,
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

    println!("\n{gym}, {lift}\n");
    println!("  {:>2}  {:<12}{:<24}cycling", "µ", "w/c", "gym");
    // **A standalone programme on both sides**, and neither row names a
    // microcycle of the block that follows it: the gym's is a `test` programme
    // and cycling's is *Power Zone test*.
    let test = if tests {
        format!("{} µ1 — the FTP test", skeleton::POWER_ZONE_TEST)
    } else {
        "—".to_owned()
    };
    println!("   0  {start}  {:<24}{test}", "entry test — the 1RM test");

    let usable: Vec<&Offered> = offered.iter().filter(|one| one.answer.is_some()).collect();
    for (cycle, one) in usable.iter().take(3).enumerate() {
        let Some(answer) = &one.answer else { continue };
        for (index, micro) in answer.microcycles.iter().enumerate() {
            let ordinal = cycle * SBS_MICROCYCLES + index + 1;
            let date =
                start.saturating_add(jiff::Span::new().weeks(i64::try_from(ordinal).unwrap_or(0)));
            let within = index + 1;
            let gym = if within == SBS_MICROCYCLES {
                format!("sbs-{} µ{within} — 1RM test", cycle + 1)
            } else {
                format!("sbs-{} µ{within}", cycle + 1)
            };
            println!("  {ordinal:>2}  {date}  {gym:<24}{} µ{micro}", one.name);
        }
    }

    if microcycles != SBS_MICROCYCLES {
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
