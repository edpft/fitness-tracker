//! Transcribe a Peloton programme, and derive what it answers when asked for a
//! smaller shape.
//!
//! **What this replaces.** Peak was transcribed from screenshots and got a
//! cool-down wrong by five minutes; Build and Boost Your Base were then read
//! through a scratchpad Python script. This is the same work in the codebase,
//! against the same API, so the answer can be checked rather than trusted.
//!
//! ```text
//! set -a; . ./.env; set +a
//! cargo run -p infrastructure --example transcribe -- skeleton.txt
//! ```
//!
//! The skeleton is `class_id microcycle session` per line, because **Peloton
//! serves classes and not programmes** (decision 0033): which class sits where is
//! the operator's to say and is never fetched.

use std::{collections::BTreeMap, fmt::Write as _};

use domain::cycling::{PowerZone, Ride, ZoneProfile, diverges, is_three_to_one};
use infrastructure::peloton::{
    auth::{PelotonAuth, PelotonCredentials},
    class::{ClassSession, PelotonClasses},
};

const AUTH_BASE: &str = "https://auth.onepeloton.com";
const API_BASE: &str = "https://api.onepeloton.com";

struct Placed {
    microcycle: u32,
    session: u32,
    class: ClassSession,
}

fn clock(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: transcribe <skeleton file>")?;
    let skeleton = std::fs::read_to_string(&path)?;
    let email = std::env::var("PELOTON_EMAIL")?;
    let password = std::env::var("PELOTON_PASSWORD")?;

    let mut wanted = Vec::new();
    for line in skeleton.lines() {
        let mut parts = line.split_whitespace();
        let (Some(id), Some(micro), Some(session)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        wanted.push((
            id.to_owned(),
            micro.parse::<u32>()?,
            session.parse::<u32>()?,
        ));
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let classes = PelotonClasses::new(
        API_BASE,
        PelotonAuth::new(AUTH_BASE, PelotonCredentials::new(email, password)),
    );

    let mut placed = Vec::new();
    for (id, microcycle, session) in wanted {
        let class = runtime.block_on(classes.class(&id))?;
        placed.push(Placed {
            microcycle,
            session,
            class,
        });
    }

    report(&placed);
    Ok(())
}

fn report(placed: &[Placed]) {
    transcription(placed);
    derivation(placed);
}

fn transcription(placed: &[Placed]) {
    println!(
        "{:<4}{:<4}{:<44}{:>8}{:>8}{:>7}  zones",
        "µ", "s", "class", "warm", "ride", "cool"
    );
    for entry in placed {
        let class = &entry.class;
        let zones = if class.is_ftp_test {
            "no zones — this is the test".to_owned()
        } else {
            class
                .time_in_zone()
                .into_iter()
                .fold(String::new(), |mut acc, (zone, seconds)| {
                    let _ = write!(acc, "{zone} {} ", clock(seconds));
                    acc
                })
        };
        println!(
            "{:<4}{:<4}{:<44}{:>8}{:>8}{:>7}  {zones}",
            entry.microcycle,
            entry.session,
            class.title.chars().take(43).collect::<String>(),
            clock(class.warm_up_seconds),
            clock(class.ride_seconds),
            clock(class.cool_down_seconds),
        );
    }

    // **Every riding class should tile exactly.** One that does not is one this
    // reader has misunderstood — except the test, which has no zone plan at all.
    let ragged: Vec<&str> = placed
        .iter()
        .filter(|entry| !entry.class.tiles() && !entry.class.is_ftp_test)
        .map(|entry| entry.class.title.as_str())
        .collect();
    println!(
        "\ntiling: {} of {} riding classes account for their whole ride{}",
        placed.iter().filter(|e| e.class.tiles()).count(),
        placed.len(),
        if ragged.is_empty() {
            String::new()
        } else {
            format!("  — ragged: {ragged:?}")
        }
    );
}

fn derivation(placed: &[Placed]) {
    let microcycles: Vec<u32> = {
        let mut seen: Vec<u32> = placed.iter().map(|entry| entry.microcycle).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };

    println!("\nhard work (Z4+) as a share of each microcycle's riding\n");
    print!("  {:<10}", "sessions");
    for micro in &microcycles {
        print!("{:>7}", format!("µ{micro}"));
    }
    println!("   3:1?");

    let sessions: Vec<u32> = {
        let mut seen: Vec<u32> = placed.iter().map(|entry| entry.session).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    };
    let mut candidates: Vec<Vec<u32>> = vec![sessions.clone()];
    for (index, first) in sessions.iter().enumerate() {
        for second in sessions.iter().skip(index + 1) {
            candidates.push(vec![*first, *second]);
        }
    }

    let whole = ZoneProfile::of(rides(placed, &sessions));
    for candidate in &candidates {
        let label = if *candidate == sessions {
            "all".to_owned()
        } else {
            candidate
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("+")
        };
        print!("  {label:<10}");
        let mut shares = Vec::new();
        for micro in &microcycles {
            let profile = ZoneProfile::of(
                placed
                    .iter()
                    .filter(|entry| {
                        entry.microcycle == *micro && candidate.contains(&entry.session)
                    })
                    .filter_map(|entry| entry.class.ride.as_ref()),
            );
            let share = profile.hard_share();
            shares.push(share);
            print!("{share:>6.0}%");
        }
        let mesocycle = microcycles.windows(4).enumerate().find_map(|(at, window)| {
            let run = shares.get(at..at + 4)?;
            let (first, last) = (window.first()?, window.last()?);
            is_three_to_one(run).then_some((*first, *last))
        });
        match mesocycle {
            Some((from, to)) => println!("   µ{from}–{to}"),
            None => println!("   none"),
        }
    }

    println!("\ndivergence from the whole programme's zone profile\n");
    for candidate in candidates.iter().filter(|c| **c != sessions) {
        let profile = ZoneProfile::of(rides(placed, candidate));
        println!(
            "  sessions {:<8} {:>6.1}   {}",
            candidate
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("+"),
            diverges(&profile, &whole),
            shares_line(&profile)
        );
    }
    println!(
        "  {:<17} {:>6.1}   {}",
        "all sessions",
        0.0,
        shares_line(&whole)
    );
}

fn rides<'a>(placed: &'a [Placed], sessions: &'a [u32]) -> impl Iterator<Item = &'a Ride> {
    placed
        .iter()
        .filter(move |entry| sessions.contains(&entry.session))
        .filter_map(|entry| entry.class.ride.as_ref())
}

fn shares_line(profile: &ZoneProfile) -> String {
    let shares: BTreeMap<PowerZone, f64> = profile.shares();
    PowerZone::ALL
        .into_iter()
        .filter(|zone| shares.contains_key(zone))
        .fold(String::new(), |mut acc, zone| {
            let _ = write!(
                acc,
                "{zone} {:.1}%  ",
                shares.get(&zone).copied().unwrap_or_default()
            );
            acc
        })
}
