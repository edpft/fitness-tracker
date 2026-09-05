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

use domain::cycling::{
    PowerZone, ZoneProfile, diverges, is_mesocycle, mesocycles, partition, span, zones_lost,
};
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
        .ok_or("usage: transcribe <skeleton file> [microcycles] [sessions]")?;
    // **The request** (decision 0035). Absent, the programme is only described.
    let request = match (std::env::args().nth(2), std::env::args().nth(3)) {
        (Some(micro), Some(session)) => Some((micro.parse::<usize>()?, session.parse::<usize>()?)),
        _ => None,
    };
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
    if let Some((wanted_microcycles, wanted_sessions)) = request {
        answers(&placed, wanted_microcycles, wanted_sessions);
    }
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

/// Every microcycle the skeleton places a class in, in order.
fn microcycles(placed: &[Placed]) -> Vec<u32> {
    let mut seen: Vec<u32> = placed.iter().map(|entry| entry.microcycle).collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

/// Every session position the skeleton uses, in order.
fn sessions(placed: &[Placed]) -> Vec<u32> {
    let mut seen: Vec<u32> = placed.iter().map(|entry| entry.session).collect();
    seen.sort_unstable();
    seen.dedup();
    seen
}

fn derivation(placed: &[Placed]) {
    let microcycles = microcycles(placed);

    println!("\nhard work (Z4+) as a share of each microcycle's riding\n");
    print!("  {:<10}", "sessions");
    for micro in &microcycles {
        print!("{:>7}", format!("µ{micro}"));
    }
    println!("   3:1?");

    let sessions = sessions(placed);
    let mut candidates: Vec<Vec<u32>> = vec![sessions.clone()];
    for (index, first) in sessions.iter().enumerate() {
        for second in sessions.iter().skip(index + 1) {
            candidates.push(vec![*first, *second]);
        }
    }

    for candidate in &candidates {
        print!("  {:<10}", label(candidate, &sessions));
        let mut shares = Vec::new();
        for micro in &microcycles {
            let share = microcycle(placed, *micro, candidate).hard_share();
            shares.push(share);
            print!("{share:>6.0}%");
        }
        println!("   {}", found(&shares, &microcycles));
    }

    // **The same microcycles weighed the other way** (issue #71). Hard share
    // thresholds at Z4 and so reports zeros for a programme built entirely below
    // it; TSS multiplies time by intensity and sees the volume and the drift
    // from Z2 toward Z3 that such a programme is made of. Both are printed
    // because they can disagree about where a mesocycle starts, and which of
    // them bounds one is the operator's judgement rather than this code's.
    println!("\nvolume, intensity and TSS of each microcycle — no FTP, no heart rate\n");
    print!("  {:<11}", "sessions");
    for micro in &microcycles {
        print!("{:>7}", format!("µ{micro}"));
    }
    println!("   mesocycles");

    let mut windows: Vec<Option<(u32, u32)>> = Vec::new();
    for candidate in &candidates {
        // **All three, because a programme moves the two axes independently.**
        // Base raises volume at a flat intensity and Build raises intensity at a
        // flat volume; both read as a rising TSS, and only the pair says which.
        let profiles: Vec<_> = microcycles
            .iter()
            .map(|micro| microcycle(placed, *micro, candidate))
            .collect();
        let scores: Vec<f64> = profiles.iter().map(ZoneProfile::tss).collect();

        print!("  {:<7}{:<4}", label(candidate, &sessions), "min");
        for profile in &profiles {
            print!("{:>7}", profile.total() / 60);
        }
        println!();
        print!("  {:<7}{:<4}", "", "int");
        for profile in &profiles {
            print!("{:>7.1}", profile.intensity());
        }
        println!();
        print!("  {:<7}{:<4}", "", "TSS");
        for score in &scores {
            print!("{score:>7.0}");
        }
        println!("   {}", found(&scores, &microcycles));

        windows.push(first_window(&scores, &microcycles));
    }

    // **Scored over the mesocycle the candidate answers with**, not over the
    // whole programme. Once the microcycles are chosen the question is which
    // *sessions* represent them, so the candidate is compared against the same
    // microcycles taken whole; including one the candidate does not cover mixes
    // in a different question. Decision 0032's figures are this window, and
    // reading Build over all five microcycles instead gives 6.0 where 0032
    // records 8.5. Where no 3:1 run is found there is no mesocycle to score
    // against and the whole programme is used, which the column says.
    println!("\ndivergence from the same microcycles taken whole\n");
    for (candidate, mesocycle) in candidates.iter().zip(&windows) {
        let span = mesocycle.map_or_else(
            || {
                (
                    microcycles.first().copied().unwrap_or(0),
                    microcycles.last().copied().unwrap_or(0),
                )
            },
            |window| window,
        );
        let profile = over(placed, span, candidate);
        let whole = over(placed, span, &sessions);
        let window = format!(
            "µ{}–{}{}",
            span.0,
            span.1,
            if mesocycle.is_some() { "" } else { ", no 3:1" }
        );
        println!(
            "  {:<10} {:>6.1}   over {window:<14}{}",
            label(candidate, &sessions),
            diverges(&profile, &whole),
            shares_line(&profile)
        );
    }
}

/// The zone profile of a run of microcycles, taking only the sessions asked for.
fn over(placed: &[Placed], (from, to): (u32, u32), sessions: &[u32]) -> ZoneProfile {
    ZoneProfile::of(
        placed
            .iter()
            .filter(|entry| (from..=to).contains(&entry.microcycle))
            .filter(|entry| sessions.contains(&entry.session))
            .filter_map(|entry| entry.class.ride.as_ref()),
    )
}

/// What this programme answers when asked for `n` microcycles of `m` sessions.
///
/// **A provider supplies mesocycles, not programmes** (decision 0036), so the
/// programme is split first and each mesocycle answers for itself. Asking an
/// eight-microcycle programme for one four-microcycle shape that represents the
/// whole of it is the wrong question, and it produced selections straddling both
/// halves of Base and of Peak.
///
/// **Subsets, never permutations** (0035): microcycles may be dropped, the
/// written order kept. Two structural checks refuse before anything is scored —
/// the run must end at its bottom level, and it must not stop training a zone
/// the mesocycle trains. What survives is ranked by composition, lowest wins.
fn answers(placed: &[Placed], wanted_microcycles: usize, wanted_sessions: usize) {
    let (microcycles, sessions) = (microcycles(placed), sessions(placed));
    if wanted_microcycles == 0 || wanted_sessions == 0 {
        println!("\n\nnothing was asked for");
        return;
    }

    let whole: Vec<f64> = microcycles
        .iter()
        .map(|micro| microcycle(placed, *micro, &sessions).tss())
        .collect();
    let split = partition(&whole);

    println!("\n\nasked for {wanted_microcycles} microcycles of {wanted_sessions} sessions\n");
    if split.is_empty() {
        println!("  this programme contains no mesocycle to answer with");
        return;
    }

    for (at, run) in split.iter().enumerate() {
        let within: Vec<u32> = run
            .clone()
            .filter_map(|index| microcycles.get(index).copied())
            .collect();
        let reference = of(placed, &within, &sessions);
        println!(
            "  mesocycle {} — µ{}\n",
            at + 1,
            within
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join("-")
        );

        let mut admitted: Vec<(f64, String, String)> = Vec::new();
        for chosen in subsets(&within, wanted_microcycles) {
            for taken in subsets(&sessions, wanted_sessions) {
                let profile = of(placed, &chosen, &taken);
                let scores: Vec<f64> = chosen
                    .iter()
                    .map(|micro| microcycle(placed, *micro, &taken).tss())
                    .collect();
                let (mus, ss) = (name(&chosen, "-"), name(&taken, "+"));
                let lost = zones_lost(&profile, &reference);

                let verdict = if !is_mesocycle(&scores) {
                    "refused — does not end in a deload".to_owned()
                } else if !lost.is_empty() {
                    let named: Vec<String> = lost.iter().map(ToString::to_string).collect();
                    format!("refused — stops training {}", named.join(", "))
                } else {
                    admitted.push((diverges(&profile, &reference), mus.clone(), ss.clone()));
                    String::new()
                };
                let working = scores.split_last().map_or(&[][..], |(_, rest)| rest);
                println!(
                    "    µ{mus:<10}{ss:<8}{:>7.1}{:>7}   {verdict}",
                    diverges(&profile, &reference),
                    span(working).map_or_else(|| "—".to_owned(), |ratio| format!("{ratio:.2}×")),
                );
            }
        }

        // **Simply the lowest** (decision 0036). Where two candidates are close
        // enough to be a tie, the operator's reasoning is that the choice is
        // therefore immaterial, so there is nothing to preserve by offering both.
        admitted.sort_by(|a, b| a.0.total_cmp(&b.0));
        match admitted.first() {
            Some((score, mus, ss)) => {
                println!("\n    answers µ{mus} by sessions {ss}, diverging {score:.1}\n");
            }
            None => println!("\n    answers nothing — every candidate was refused\n"),
        }
    }
}

/// A selection, written the way the tables write it.
fn name(items: &[u32], between: &str) -> String {
    items
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(between)
}

/// Every subset of `items` of the given size, keeping the order they are given.
///
/// **Subsets, never permutations** (decision 0035): a microcycle may be dropped
/// but the written order is kept, so this only ever leaves things out. Built by
/// recursion rather than by index arithmetic, because indexing can panic and
/// panics are forbidden.
///
/// A size of zero yields one empty subset, which is what makes the recursion
/// terminate. Callers asking for nothing are refused before they get here.
fn subsets(items: &[u32], size: usize) -> Vec<Vec<u32>> {
    if size == 0 {
        return vec![Vec::new()];
    }
    let Some((first, rest)) = items.split_first() else {
        return Vec::new();
    };
    let mut out: Vec<Vec<u32>> = subsets(rest, size - 1)
        .into_iter()
        .map(|mut including| {
            including.insert(0, *first);
            including
        })
        .collect();
    out.extend(subsets(rest, size));
    out
}

/// The zone profile of a set of microcycles, taking only the sessions asked for.
fn of(placed: &[Placed], chosen: &[u32], taken: &[u32]) -> ZoneProfile {
    ZoneProfile::of(
        placed
            .iter()
            .filter(|entry| chosen.contains(&entry.microcycle) && taken.contains(&entry.session))
            .filter_map(|entry| entry.class.ride.as_ref()),
    )
}

/// Every mesocycle a run of scores contains, named by microcycle.
fn found(scores: &[f64], microcycles: &[u32]) -> String {
    let runs = mesocycles(scores, 4);
    if runs.is_empty() {
        return "none".to_owned();
    }
    runs.iter()
        .filter_map(|run| {
            let from = microcycles.get(run.start)?;
            let to = microcycles.get(run.end - 1)?;
            Some(format!("µ{from}–{to}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first mesocycle, for scoring divergence against (decision 0032).
fn first_window(scores: &[f64], microcycles: &[u32]) -> Option<(u32, u32)> {
    let run = mesocycles(scores, 4).into_iter().next()?;
    Some((*microcycles.get(run.start)?, *microcycles.get(run.end - 1)?))
}

/// The zone profile of one microcycle, taking only the sessions asked for.
fn microcycle(placed: &[Placed], micro: u32, sessions: &[u32]) -> ZoneProfile {
    ZoneProfile::of(
        placed
            .iter()
            .filter(|entry| entry.microcycle == micro && sessions.contains(&entry.session))
            .filter_map(|entry| entry.class.ride.as_ref()),
    )
}

/// How a candidate selection is named in a table.
fn label(candidate: &[u32], every: &[u32]) -> String {
    if candidate == every {
        return "all".to_owned();
    }
    candidate
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("+")
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
