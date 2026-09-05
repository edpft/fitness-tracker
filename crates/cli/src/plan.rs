//! Generating a hybrid programme: two providers, one span, one set of
//! constraints.
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
//! **Nothing is stored.** The cycling side is read from Peloton on every run.
//! Authoring it once into the store is issue #55.

use domain::cycling::{Answer, Programme};
use infrastructure::peloton::{
    auth::{PelotonAuth, PelotonCredentials},
    class::PelotonClasses,
    provider, skeleton,
};
use jiff::civil::Date;

use crate::{Failure, exit, wizard};

const AUTH_BASE: &str = "https://auth.onepeloton.com";
const API_BASE: &str = "https://api.onepeloton.com";

/// The gym providers this build holds.
const GYM_PROVIDERS: [&str; 1] = ["Stronger By Science, two-day intermediate"];

/// Microcycles in a mesocycle, and where the test sits in an SBS one.
const SBS_MICROCYCLES: usize = 4;

/// One cycling mesocycle, named, with what it answers.
struct Offered {
    name: String,
    own: usize,
    answer: Option<Answer>,
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

/// Ask what the programme is made of, then generate it.
///
/// # Errors
///
/// [`Failure`] if there is nobody to ask, if the credentials are absent, or if
/// Peloton will not answer.
pub async fn generate(microcycles: usize, sessions: usize) -> Result<(), Failure> {
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

    let classes = credentials()?;
    println!(
        "\nreading {} from Peloton",
        pairing.programmes.join(" and ")
    );

    let mut offered = Vec::new();
    let mut test_microcycle = None;
    for name in pairing.programmes {
        let placements = placements(name)?;
        let programme = provider::programme(&classes, &placements)
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
        // **Build supplies the cycling test microcycle**, whether or not the
        // pairing rides Build's mesocycle later: it is the only Power Zone
        // programme holding an FTP test.
        if name == "Power Zone Build" {
            test_microcycle = programme
                .mesocycles()
                .last()
                .and_then(|meso| meso.last().copied());
        }
        offered.extend(mesocycles_of(name, &programme, microcycles, sessions));
    }

    report(&offered, gym, &lift, start, test_microcycle, microcycles);
    Ok(())
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
        other => Err(Failure::message(
            format!("this build holds no skeleton for {other:?}"),
            exit::USAGE,
        )),
    }
}

/// Split a programme and ask each mesocycle for the shape wanted.
fn mesocycles_of(
    label: &str,
    programme: &Programme,
    microcycles: usize,
    sessions: usize,
) -> Vec<Offered> {
    programme
        .mesocycles()
        .into_iter()
        .enumerate()
        .map(|(at, mesocycle)| Offered {
            name: format!("{label} {}", at + 1),
            own: mesocycle.len(),
            answer: programme
                .answer(&mesocycle, microcycles, sessions)
                .into_iter()
                .next(),
        })
        .collect()
}

fn report(
    offered: &[Offered],
    gym: &str,
    lift: &str,
    start: Date,
    test_microcycle: Option<u32>,
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
    let test = test_microcycle.map_or_else(|| "—".to_owned(), |micro| format!("Build µ{micro}"));
    println!("   0  {start}  {:<24}{test}", "SBS µ4 — the 1RM test");

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
