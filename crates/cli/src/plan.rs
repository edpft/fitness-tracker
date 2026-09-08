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

use application::{Authored, DiaryStore as _, PlanAuthor as _, PlanStore as _};
use domain::{
    cycling::{
        Answer, CyclingMesocycle, CyclingMicrocycle, CyclingWeekdays, PlannedRide,
        PublishedProgramme, SessionPosition,
    },
    gym::exercise::RepsExercise,
    normalised::OperatorZone,
    plan::{Plan, PlanName, Programme},
    prescription::PrimaryPattern,
    provider::{ExternalProgramme, ProgrammeName, Provider},
    schedule::Discipline,
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteDiaryStore, SqliteGenerationParameterStore, SqliteGymMesocycleStore, SqlitePlanStore,
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

/// Who publishes the cycling programmes this build reads.
const PELOTON: &str = "Peloton";

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
    test_microcycle: SBS_MICROCYCLES,
}];

/// Progressions after the entry test. Three of four, against thirteen weeks of
/// cycling (decision 0034).
const PROGRESSIONS: usize = 3;

/// Microcycles in a mesocycle, and where the test sits in an SBS one.
///
/// Counted as a microcycle *number* because that is what the wizard asks for;
/// the report below widens it to index with.
pub const SBS_MICROCYCLES: u32 = 4;

/// One published programme, read once.
struct Read {
    name: &'static str,
    published: ProgrammeName,
    fetched: Fetched,
    programme: PublishedProgramme,
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
        label: "Boost Your Base, then Build Your Power Zones",
        programmes: ["Boost Your Base", "Build Your Power Zones"],
    },
    Pairing {
        label: "Build Your Power Zones, then Peak Your Power Zones",
        programmes: ["Build Your Power Zones", "Peak Your Power Zones"],
    },
];

/// Ask what the programme is made of, generate it, and offer to author it.
///
/// # Errors
///
/// [`Failure`] if there is nobody to ask, if the credentials are absent, if
/// Peloton will not answer, or if the store refuses what would be authored.
pub async fn generate(
    database: &Path,
    zone: &OperatorZone,
    microcycles: usize,
    sessions: usize,
) -> Result<(), Failure> {
    wizard::interactive()?;

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

    report(
        &offered,
        gym,
        lift.as_str(),
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
        &pool,
        zone,
        SqlitePlanStore::new(pool.clone(), zone.clone()),
        SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
        SqliteGenerationParameterStore::new(pool.clone()),
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

/// The Peloton adapters, from the environment.
///
/// **Public because `cycling stack` composes the same two.** Both need the same
/// credentials and the same cached token, and building them twice from two
/// places is how one of them ends up without the cache.
///
/// # Errors
///
/// [`Failure`] if either credential is absent from the environment.
pub fn peloton() -> Result<(PelotonClasses, infrastructure::peloton::PelotonStack), Failure> {
    let classes = credentials()?;
    let stack = infrastructure::peloton::PelotonStack::new(
        infrastructure::peloton::stack::GATEWAY.to_owned(),
        auth()?,
    );
    Ok((classes, stack))
}

/// The authenticator, with the token cache where there is one.
fn auth() -> Result<PelotonAuth, Failure> {
    let missing = |name: &str| Failure::message(format!("{name} is not set"), exit::USAGE);
    let email = std::env::var("PELOTON_EMAIL").map_err(|_| missing("PELOTON_EMAIL"))?;
    let password = std::env::var("PELOTON_PASSWORD").map_err(|_| missing("PELOTON_PASSWORD"))?;
    let mut auth = PelotonAuth::new(AUTH_BASE, PelotonCredentials::new(email, password));

    // **Where the token is kept, when there is anywhere to keep it.** Without
    // this every invocation walks the whole Auth0 flow to obtain a token the
    // last one already had (#54). A machine with neither `XDG_STATE_HOME` nor
    // `HOME` gets the old behaviour rather than an error: logging in again costs
    // a few seconds, and refusing to run costs the session.
    if let Ok(path) = crate::paths::token(&crate::paths::SystemEnvironment, "peloton") {
        auth = auth.caching_in(infrastructure::peloton::TokenFile::new(path));
    }
    Ok(auth)
}

fn credentials() -> Result<PelotonClasses, Failure> {
    Ok(PelotonClasses::new(API_BASE, auth()?))
}

fn placements(name: &str) -> Result<Vec<skeleton::Placement>, Failure> {
    match name {
        "Boost Your Base" => Ok(skeleton::BOOST_YOUR_BASE.placements().to_vec()),
        "Build Your Power Zones" => Ok(skeleton::BUILD_YOUR_POWER_ZONES.placements().to_vec()),
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
/// Whether any session ridden in this microcycle measures FTP.
///
/// **The class says so, not the programme.** Peloton marks the test rides
/// themselves, which is what makes this answerable for a microcycle chosen out
/// of the middle of a programme — Build's µ5 is a test week whether it is ridden
/// as Build's fifth or as our fourth.
fn measures(read: &Read, microcycle: u32, sessions: &[u32]) -> bool {
    sessions.iter().any(|session| {
        read.fetched
            .get(&(microcycle, *session))
            .is_some_and(|classes| classes.iter().any(|class| class.is_ftp_test))
    })
}

fn test_microcycle(read: &Read, sessions: usize) -> Option<Vec<u32>> {
    let number = read.programme.microcycles().first().copied()?;

    let mut taken: Vec<u32> = read
        .programme
        .sessions()
        .into_iter()
        .filter(|session| measures(read, number, &[*session]))
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

/// Write the plan: both programmes, and every mesocycle in them.
///
/// **Both halves or neither** (issue #73). It printed thirteen weeks and wrote
/// the cycling five of them until 2026-09-07, leaving the gym side to
/// `fitness programme add` afterwards — which meant the command that exists to
/// author a hybrid plan authored half of one. The gym questions are the wizard's
/// own, asked through [`wizard::gym_side`] rather than restated here.
#[expect(clippy::too_many_arguments, reason = "one plan's worth of answers")]
async fn author(
    pool: &infrastructure::SqlitePool,
    zone: &OperatorZone,
    plans: SqlitePlanStore,
    gym: SqliteGymMesocycleStore,
    parameter_store: SqliteGenerationParameterStore,
    start: Date,
    provider: &GymProvider,
    lift: RepsExercise,
    riding_days: &[Weekday],
    test: Option<(&Read, &[u32])>,
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
    let parameters = wizard::ready(&parameter_store).await?;

    let published = ExternalProgramme::new(
        Provider::try_from(provider.provider.to_owned()).map_err(|error| Failure::usage(&error))?,
        ProgrammeName::try_from(provider.programme.to_owned())
            .map_err(|error| Failure::usage(&error))?,
    );
    let gym_side = wizard::gym_side(
        pool,
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
                mesocycle.programme().name().to_string(),
                mesocycle.duration_weeks(),
                mesocycle.start(),
            )
        })
        .collect();

    let existing = plans
        .named(&name)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let plan = with_both(name, existing.as_ref(), gym_side.clone(), mesocycles)?;

    let (_, authored) = application::prescribe::Authoring::new(plans, gym, parameter_store)
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
    gym: Vec<domain::prescription::Mesocycle>,
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
    start: Date,
    read: &Read,
    microcycles: &[u32],
    sessions: &[u32],
    riding_days: &[Weekday],
) -> Result<CyclingMesocycle, Failure> {
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
        weeks.push(CyclingMicrocycle::new(rides, *number).map_err(|error| Failure::usage(&error))?);
    }

    let weekdays = CyclingWeekdays::new(
        riding_days
            .iter()
            .copied()
            .zip(positions.iter().copied())
            .collect(),
    )
    .map_err(|error| Failure::usage(&error))?;

    CyclingMesocycle::new(
        ExternalProgramme::new(
            Provider::try_from(PELOTON.to_owned()).map_err(|error| Failure::usage(&error))?,
            read.published.clone(),
        ),
        start,
        NonEmpty::new(weeks).map_err(|error| Failure::usage(&error))?,
        weekdays,
    )
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
        format!("{} µ1 — the FTP test", skeleton::POWER_ZONE_TEST)
    } else {
        "—".to_owned()
    };
    println!("   0  {start}  {:<32}{test}", "Entry test — the 1RM test");

    let usable: Vec<&Offered> = offered.iter().filter(|one| one.answer.is_some()).collect();
    for (cycle, one) in usable.iter().take(3).enumerate() {
        let Some(answer) = &one.answer else { continue };
        for index in 0..answer.microcycles.len() {
            let ordinal = cycle * SBS_MICROCYCLES as usize + index + 1;
            let date =
                start.saturating_add(jiff::Span::new().weeks(i64::try_from(ordinal).unwrap_or(0)));
            let within = index + 1;
            let named = format!("{} {} µ{within}", gym.programme, cycle + 1);
            let named = if within == SBS_MICROCYCLES as usize {
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
            let riding = if measures(one.from, ridden, &answer.sessions) {
                format!("{} µ{within} — the FTP test", one.name)
            } else {
                format!("{} µ{within}", one.name)
            };
            println!("  {ordinal:>2}  {date}  {named:<32}{riding}");
        }
    }

    if microcycles != SBS_MICROCYCLES as usize {
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
