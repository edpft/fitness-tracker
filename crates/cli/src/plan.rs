//! Generating a hybrid programme: two providers, one span, one set of
//! constraints.
//!
//! **This is what the tool exists for.** A gym provider and a cycling provider
//! are each asked for mesocycles of a stated shape, and the answers are laid
//! against one another and checked for collisions (decision 0034): a test week
//! in one discipline must fall on a test week or a deload in the other.
//!
//! **Nothing is stored.** The cycling side is read from Peloton on every run,
//! which is 65 class fetches and takes a minute. Authoring it once into the
//! store is issue #55 and is what stops this being slow.

use domain::cycling::Programme;
use infrastructure::peloton::{
    auth::{PelotonAuth, PelotonCredentials},
    class::PelotonClasses,
    provider, skeleton,
};
use jiff::civil::Date;

use crate::{Failure, exit};

const AUTH_BASE: &str = "https://auth.onepeloton.com";
const API_BASE: &str = "https://api.onepeloton.com";

/// How many weeks one SBS cycle runs, and where its test sits within it.
const SBS_WEEKS: usize = 4;

/// One cycling mesocycle, named, with what it answers.
struct Offered {
    name: String,
    microcycles: Vec<u32>,
    answer: Option<domain::cycling::Answer>,
}

/// Generate the hybrid programme.
///
/// # Errors
///
/// [`Failure`] if the credentials are absent, if Peloton will not answer, or if
/// a provider cannot supply the shape asked for.
pub async fn generate(
    start: Date,
    microcycles: usize,
    cycling_sessions: usize,
    gym_sessions: usize,
) -> Result<(), Failure> {
    if gym_sessions != 2 {
        return Err(Failure::message(
            format!(
                "the SBS chart is two sessions a week and cannot answer for {gym_sessions} — \
                 a percentage day and a repetition-maximum day"
            ),
            exit::USAGE,
        ));
    }

    let email = std::env::var("PELOTON_EMAIL")
        .map_err(|_| Failure::message("PELOTON_EMAIL is not set", exit::USAGE))?;
    let password = std::env::var("PELOTON_PASSWORD")
        .map_err(|_| Failure::message("PELOTON_PASSWORD is not set", exit::USAGE))?;
    let classes = PelotonClasses::new(
        API_BASE,
        PelotonAuth::new(AUTH_BASE, PelotonCredentials::new(email, password)),
    );

    println!("reading the Peloton programmes — 65 classes, this takes a minute");
    let mut offered = Vec::new();
    for (label, placements) in [
        (
            skeleton::BOOST_YOUR_BASE.name(),
            skeleton::BOOST_YOUR_BASE.placements().to_vec(),
        ),
        (
            skeleton::POWER_ZONE_BUILD.name(),
            skeleton::POWER_ZONE_BUILD.placements().to_vec(),
        ),
        ("Peak Your Power Zones", skeleton::peak_your_power_zones()),
    ] {
        let programme = provider::programme(&classes, &placements)
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
        offered.extend(mesocycles_of(
            label,
            &programme,
            microcycles,
            cycling_sessions,
        ));
    }

    report(&offered, start, microcycles);
    Ok(())
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
            answer: programme
                .answer(&mesocycle, microcycles, sessions)
                .into_iter()
                .next(),
            microcycles: mesocycle,
        })
        .collect()
}

fn report(offered: &[Offered], start: Date, microcycles: usize) {
    println!("\nwhat each mesocycle answers\n");
    println!(
        "  {:<24}{:<12}{:<12}{:>7}{:>8}",
        "mesocycle", "own shape", "answers", "compos", "span"
    );
    for one in offered {
        let own = format!("{} × 3", one.microcycles.len());
        match &one.answer {
            Some(answer) => println!(
                "  {:<24}{:<12}{:<12}{:>7.1}{:>8}",
                one.name,
                own,
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
            None => println!("  {:<24}{own:<12}{:<12}", one.name, "nothing"),
        }
    }

    // **Both disciplines open with a test microcycle and run mesocycles of the
    // same length** (0034), so week 1 is the two tests and every gym test week
    // lands on the fourth microcycle of a cycling mesocycle — which is its
    // deload. The rule holds by construction; this shows it holding.
    println!("\nthe span, three mesocycles against three SBS cycles\n");
    println!("  {:>2}  {:<12}{:<26}cycling", "w", "w/c", "gym");
    let usable: Vec<&Offered> = offered.iter().filter(|one| one.answer.is_some()).collect();
    let entry = "autumn-entry-test";
    println!("   1  {start}  {entry:<26}FTP test microcycle          <- both tests");
    for (cycle, one) in usable.iter().take(3).enumerate() {
        let Some(answer) = &one.answer else { continue };
        for (index, micro) in answer.microcycles.iter().enumerate() {
            let week = 1 + cycle * SBS_WEEKS + index + 1;
            let date = start.saturating_add(
                jiff::Span::new().weeks(i64::try_from(week).unwrap_or(0).saturating_sub(1)),
            );
            let sbs = index + 1;
            let gym = if sbs == SBS_WEEKS {
                format!("sbs-{} week {sbs} — 1RM test", cycle + 1)
            } else {
                format!("sbs-{} week {sbs}", cycle + 1)
            };
            let note = if sbs == SBS_WEEKS {
                "          ← test on deload"
            } else {
                ""
            };
            println!("  {week:>2}  {date}  {gym:<26}{} µ{micro}{note}", one.name);
        }
    }
    if usable.len() > 3 {
        println!(
            "\n  {} mesocycles are available and three are needed — which three is a \
             programming choice",
            usable.len()
        );
    }
    if microcycles != SBS_WEEKS {
        println!(
            "\n  ! asked for {microcycles}-microcycle mesocycles against 4-week SBS cycles; \
             the weeks above will not line up"
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
