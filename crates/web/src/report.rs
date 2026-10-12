//! What a run of `next` did, where the plan stands, and what is next, as a page.
//!
//! **The terminal's words, not its layout.** Every phrase here is the one
//! `fitness next` prints, so the two frontends say the same thing; the columns
//! the terminal pads with spaces are lists and tables, because a phone is
//! narrower than any terminal.

use std::fmt::Write as _;

use application::{
    Delivery, Placed, Prescription, RidesPlaced, Unexpressed,
    cycling::PrescribedRide,
    microcycle::Session,
    next::{
        CyclingPrescribed, Done, Found, GymPrescribed, MacrocycleStanding, PlanStanding,
        Prescribed, Report, Step, Undelivered, WeekPosition, WhatIsNext,
    },
};
use domain::{
    cycling::{CyclingMicrocycle, Ride, clock},
    gym::Load,
    measure::{PositiveDuration, Spans},
    prescription::{Prescribed as Set, PrescribedExercise, PrescribedSet},
    schedule::{DayPart, ScheduledSlot},
};
use maud::{Markup, html};

/// A step of the run, named.
pub fn step(step: &Step) -> String {
    match step {
        Step::Fetching(stream) => format!("Fetching {stream}"),
        Step::Normalising(stream) => format!("Normalising {stream}"),
        Step::Canonicalising => "Canonicalising gym sessions".to_owned(),
        // **The configurator knows which catalogue this build keeps**, as it
        // knows where rides are done.
        Step::RefreshingClasses => "Refreshing Peloton's classes".to_owned(),
        Step::Committing => "Committing the next mesocycle".to_owned(),
        Step::Skipping => "Skipping".to_owned(),
    }
}

/// How a step ended: done with a count of what it found, or failed and why.
pub fn outcome(done: &Done) -> String {
    let found = match &done.outcome {
        Err(failure) => return format!("failed: {failure}"),
        Ok(found) => found,
    };
    let what = match found {
        Found::Landed(records) if records.as_usize() == 0 => "nothing new".to_owned(),
        Found::Landed(records) => format!("{records} new"),
        Found::Sessions(sessions) => format!("{sessions} sessions"),
        Found::Canonical(sessions) => format!("{sessions} sessions"),
        Found::Classes {
            added,
            outstanding,
            stopped,
            ..
        } => {
            let mut said = match added {
                0 => "nothing new".to_owned(),
                added => format!("{added} new classes"),
            };
            if *outstanding > 0 {
                let _ = write!(said, ", {outstanding} still to read");
            }
            if let Some(error) = stopped {
                let _ = write!(said, "; reading stopped: {error}");
            }
            said
        }
        Found::Committed { concurrent, span } => format!("{concurrent}, {span}"),
        Found::Skipped { session, skip } => {
            let mut last = skip.from();
            while let Some(next) = last.next().filter(|next| *next < skip.until()) {
                last = next;
            }
            format!(
                "{} ({}): no training from {} to {}",
                named(session),
                skip.reason(),
                when(skip.from()),
                when(last),
            )
        }
    };
    format!("done, {what}")
}

/// How long a step will take, where that is worth saying (#369).
pub fn estimate(outstanding: u64, takes: std::time::Duration) -> Option<String> {
    (takes >= WORTH_ANNOUNCING)
        .then(|| format!("{outstanding} classes to read, about {}", roughly(takes)))
}

/// The first refresh against an empty catalogue is ten minutes and has to say
/// so. The weekly one is three classes and two seconds, and announcing that
/// would be commentary.
const WORTH_ANNOUNCING: std::time::Duration = std::time::Duration::from_secs(30);

/// A duration as the operator would say it.
fn roughly(takes: std::time::Duration) -> String {
    let seconds = takes.as_secs();
    if seconds < 90 {
        return format!("{seconds} seconds");
    }
    let minutes = (seconds + 30) / 60;
    format!("{minutes} minutes")
}

/// Where the plan stands, and what is next.
pub fn report(report: &Report) -> Markup {
    html! {
        (plan_standing(&report.plan))
        section.next { (what_is_next(&report.next)) }
    }
}

fn plan_standing(plan: &PlanStanding) -> Markup {
    let macrocycle = match &plan.macrocycle {
        MacrocycleStanding::Week {
            macrocycle,
            week,
            of,
        } => format!(
            "{} macrocycle, week {week} of {of}",
            capitalised(&macrocycle.to_string())
        ),
        // **Said even when it cannot be known**: past the school calendar's
        // reach there is no data, which is not the same as no term.
        MacrocycleStanding::Unknown { published_to } => format!(
            "Macrocycle unknown: the school calendar is published to {}",
            published_to.map_or_else(|| "nowhere".to_owned(), |to| to.to_string())
        ),
        MacrocycleStanding::Unreadable(error) => {
            format!("Macrocycle unknown: the school calendar could not be read ({error})")
        }
    };
    html! {
        section.plan {
            p { (macrocycle) }
            ul.disciplines {
                @for (discipline, position) in &plan.disciplines {
                    li {
                        span.name { (capitalised(discipline.as_str())) }
                        " "
                        @match position {
                            None => "no mesocycle this microcycle",
                            Some(position) => {
                                "mesocycle " (position.mesocycle) ", "
                                @match position.week {
                                    WeekPosition::Microcycle { number, of, rerun } => {
                                        "microcycle " (number) " of " (of)
                                        @if let Some(rerun) = rerun { ", rerun " (rerun) }
                                    }
                                    WeekPosition::Holding => "holding",
                                }
                            }
                        }
                    }
                }
            }
            @if let Some((monday, sessions)) = &plan.microcycle {
                p { "This microcycle, from " (monday.strftime("%A %-d %B")) }
                table.microcycle {
                    @for session in sessions {
                        tr {
                            td { (named(session)) }
                            td { (session.slot.date.strftime("%a")) " " (session.slot.slot.part) }
                            td { (session.state) }
                        }
                    }
                }
            }
        }
    }
}

fn what_is_next(next: &WhatIsNext) -> Markup {
    let (session, prescribed) = match next {
        WhatIsNext::Session {
            session,
            prescribed,
        } => (session, prescribed),
        WhatIsNext::MicrocycleComplete { after } => return html! { p { (complete(*after)) } },
        WhatIsNext::NoPlanCovers { week_of } => {
            return html! {
                p {
                    "the week of " (week_of) " has training slots and no plan covers it, \
                     so there is no microcycle to report. Author one: fitness plan"
                }
            };
        }
        WhatIsNext::NoSlot { from } => {
            return html! { p { "the schedule holds no slot on or after " (from) } };
        }
    };
    let until = session
        .closes
        .map_or_else(String::new, |closes| format!(", until {}", when(closes)));
    html! {
        p.session { "Next: " (named(session)) ", " (when(session.at())) (until) }
        @match prescribed {
            Prescribed::Gym(Err(error)) => p.failed { "not prescribed: " (error) },
            Prescribed::Gym(Ok(GymPrescribed::NothingPlanned { from, last })) => {
                p {
                    @match last {
                        Some((plan, day)) => {
                            "nothing is planned after " (day) " (" (format!("{:?}", day.weekday()))
                            "), when " (plan) "'s gym mesocycles end"
                        }
                        None => {
                            "nothing is planned on or after " (from)
                            ": no gym mesocycle has been authored"
                        }
                    }
                }
            }
            Prescribed::Gym(Ok(GymPrescribed::Issued { issued, delivery })) => {
                (gym_session(issued))
                @match delivery {
                    Ok(delivered) => (gym_delivery(delivered)),
                    Err(Undelivered::NoAccess(why)) => (not_delivered(&why.detail)),
                    Err(Undelivered::Failed(error)) => (not_delivered(&error.to_string())),
                }
            }
            Prescribed::Cycling(Err(error)) => p.failed { "not prescribed: " (error) },
            Prescribed::Cycling(Ok(prescribed)) => (cycling_session(prescribed)),
        }
    }
}

fn complete(after: Option<ScheduledSlot>) -> String {
    after.map_or_else(
        || {
            "nothing in this microcycle is still to be prescribed, and the schedule holds no \
             slot after it"
                .to_owned()
        },
        |slot| {
            format!(
                "nothing in this microcycle is still to be prescribed. The next slot is {} on {} \
                 ({:?} {})",
                slot.discipline, slot.date, slot.slot.weekday, slot.slot.part,
            )
        },
    )
}

fn not_delivered(why: &str) -> Markup {
    html! { p.failed { "not delivered: " (why) } }
}

/// The session itself: its blocks, its exercises and their sets, and any slot
/// that could not be derived.
fn gym_session(issued: &Prescription) -> Markup {
    let mut blocks: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    for item in issued.workout.shape().items().iter() {
        let Some(slot) = item.slots().next() else {
            continue;
        };
        let block = slot.block().to_string();
        if blocks.last().is_none_or(|(last, _)| *last != block) {
            blocks.push((block, Vec::new()));
        }
        if let Some((_, items)) = blocks.last_mut() {
            items.push(item.exercises().map(describe).collect());
        }
    }
    html! {
        @for (block, items) in &blocks {
            h3 { (block) }
            ul.exercises {
                @for members in items {
                    // Exercises performed back to back are one item.
                    li.paired[members.len() > 1] {
                        @for member in members { div { (member) } }
                    }
                }
            }
        }
        @if !issued.underivable.is_empty() {
            ul.failed {
                @for slot in &issued.underivable {
                    li { (slot.slot) " (" (slot.exercise) ") — not derivable: " (slot.reason) }
                }
            }
        }
    }
}

/// One exercise's line: its name, then its sets collapsed where they repeat.
fn describe(exercise: &PrescribedExercise) -> String {
    let lines: Vec<String> = match exercise {
        PrescribedExercise::ForReps { sets, .. } => sets.iter().map(set_line).collect(),
        PrescribedExercise::ForDuration { sets, .. } => sets.iter().map(set_line).collect(),
        PrescribedExercise::ForDistance { sets, .. } => sets.iter().map(set_line).collect(),
    };

    let mut collapsed: Vec<(String, usize)> = Vec::new();
    for line in lines {
        match collapsed.last_mut() {
            Some((last, count)) if *last == line => *count += 1,
            _ => collapsed.push((line, 1)),
        }
    }

    let sets = collapsed
        .into_iter()
        .map(|(line, count)| {
            if count == 1 {
                line
            } else {
                format!("{count} × {line}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("{} {sets}", exercise.exercise_key())
}

/// One set: the measure, then the load, then whatever qualifies it.
fn set_line<M: std::fmt::Display + Spans>(set: &PrescribedSet<M>) -> String {
    let mut line = match &set.prescription {
        Set::Fixed {
            load,
            measure,
            effort,
        } if unloaded(*load) => effort.as_ref().map_or_else(
            || format!("{measure}"),
            |effort| format!("{measure}, {effort} in reserve"),
        ),
        Set::Fixed {
            load,
            measure,
            effort,
        } => effort.as_ref().map_or_else(
            || format!("{measure} @ {}", weight(*load)),
            |effort| format!("{measure} @ {}, {effort} in reserve", weight(*load)),
        ),
        Set::ToEffort { load, effort, .. } => {
            format!("as many as @ {}, {effort} in reserve", weight(*load))
        }
        Set::Autoregulated { measure, effort } => {
            format!("{measure} — work up, {effort} in reserve")
        }
    };
    if set.warmup {
        line.push_str(" (warm-up)");
    }
    line
}

/// External load that is simply absent, as against bodyweight on an axis where
/// assistance is conventional.
const fn unloaded(load: Load) -> bool {
    matches!(load, Load::Absolute(mass) if mass.is_none())
}

/// A load, written the short way.
fn weight(load: Load) -> String {
    match load {
        Load::Absolute(mass) if mass.is_none() => "bodyweight".to_owned(),
        Load::Absolute(mass) => format!("{mass}kg"),
        Load::Relative(delta) if delta.as_grams() == 0 => "bodyweight".to_owned(),
        Load::Relative(delta) if delta.as_grams() < 0 => format!("bodyweight {delta}kg"),
        Load::Relative(delta) => format!("bodyweight +{delta}kg"),
    }
}

/// What a delivery amounted to.
fn gym_delivery(delivered: &Delivery) -> Markup {
    let lead = match delivered.placed {
        Placed::Created => "delivered to",
        Placed::Replaced { .. } => "replaced at",
        Placed::Unchanged => "already delivered to",
    };
    html! {
        p.delivered {
            (lead) " " (delivered.destination) " as session " (delivered.ordinal)
            " (" (delivered.reference) ")"
            @match delivered.placed {
                Placed::Replaced { superseding: Some(previous) } => {
                    br; "in place of prescription " (previous) ", which it supersedes"
                }
                Placed::Replaced { superseding: None } => {
                    br; "the same session, as this build renders it"
                }
                Placed::Created | Placed::Unchanged => {}
            }
        }
        (unexpressed(&delivered.unexpressed))
    }
}

/// What the destination had no way to state.
fn unexpressed(unexpressed: &[Unexpressed]) -> Markup {
    html! {
        @if !unexpressed.is_empty() {
            p { "what would not go:" }
            ul.failed {
                @for item in unexpressed { li { (item.exercise) " — " (item.reason) } }
            }
        }
    }
}

/// The ride, where it is done, and where it went.
fn cycling_session(prescribed: &CyclingPrescribed) -> Markup {
    let ride = &prescribed.ride;
    html! {
        p { (heading(ride)) }
        (cycling_ride(ride))
        @match &prescribed.delivery {
            Ok(placed) => (cycling_delivery(ride, placed)),
            Err(Undelivered::NoAccess(why)) => (not_delivered(&why.detail)),
            Err(Undelivered::Failed(error)) => (not_delivered(&error.to_string())),
        }
    }
}

/// Where a ride sits, as its first line says it.
fn heading(ride: &PrescribedRide) -> String {
    let next = &ride.next;
    let position = next.session.as_u8();
    if ride.holding() {
        return format!("a holding microcycle — session {position} of 2");
    }
    let sessions = ride
        .mesocycle
        .microcycle(next.microcycle)
        .map_or(0, CyclingMicrocycle::session_count);
    format!(
        "{} — microcycle {} of {}, session {position} of {sessions}",
        ride.mesocycle.provenance(),
        next.microcycle,
        ride.mesocycle.duration_weeks(),
    )
}

/// The ride itself: where it is done, its intervals, and its zones in watts.
fn cycling_ride(ride: &PrescribedRide) -> Markup {
    let session = &ride.session;
    let ftp = ride.ftp;
    html! {
        ul.venues {
            @for venue in ride.next.ride.at().iter() {
                li {
                    a href=(configurator::rides::link(venue.reference())) { (venue) }
                    @if configurator::rides::known_unavailable(venue.reference()) {
                        p.failed {
                            "! this class reads Unavailable on the operator's account. \
                             A substitute has to match its zone profile."
                        }
                    }
                }
            }
        }
        table.ride {
            tr { td { "warm up" } td { (clock(session.warm_up())) } }
            @match session.ride() {
                Ride::Effort(duration) => {
                    tr { td { "ride" } td { (clock(*duration)) " — as hard as you can hold" } }
                }
                Ride::Intervals(_) => {
                    tr { td { "ride" } td { (clock(session.ride().duration())) } }
                }
            }
            @if let Some(cool_down) = session.cool_down() {
                tr { td { "cool down" } td { (clock(cool_down)) } }
            }
            tr { td { "total" } td { (clock(session.total())) } }
        }
        @match session.ride() {
            Ride::Effort(_) => {
                p { "No zones: this is the test that measures what a zone is a share of." }
            }
            Ride::Intervals(intervals) => {
                table.intervals {
                    @for interval in intervals.iter() {
                        tr {
                            td { (interval.zone()) }
                            td { (clock(interval.duration())) }
                            // Watts only where an FTP was given.
                            @if let Some(ftp) = ftp {
                                td { (interval.zone().band().watts_at(ftp)) }
                            }
                        }
                    }
                }
                p { "time in zone" }
                table.zones {
                    @for (zone, seconds) in session.ride().time_in_zone() {
                        tr {
                            td { (zone) }
                            td {
                                (PositiveDuration::from_seconds(seconds)
                                    .map_or_else(|_| "\u{2014}".to_owned(), clock))
                            }
                            td { (zone.purpose()) }
                        }
                    }
                }
            }
        }
        @match ftp {
            Some(ftp) => p { "at FTP " (ftp) },
            None => p {
                "No FTP test precedes this session, so the zones print without watts. \
                 Ride one and collect it, or pass --ftp."
            },
        }
    }
}

/// What went to the stack, and what it now holds.
fn cycling_delivery(ride: &PrescribedRide, placed: &RidesPlaced) -> Markup {
    html! {
        ul.delivered {
            @if placed.replaced > 0 {
                li { "replaced the " (placed.replaced) " class(es) that were in the stack" }
            }
            @for venue in ride.next.ride.at().iter() { li { "delivered " (venue) } }
            li {
                "delivered " (placed.cool_down.called()) " — "
                (placed.taught_by.as_deref().unwrap_or(
                    "instructor unknown, so this is the one every session falls back to"
                ))
            }
        }
        p {
            @match placed.total {
                Some(total) => { (placed.holds) " classes in the stack, " (clock(total)) }
                None => { (placed.holds) " classes in the stack, and Peloton gives it no total" }
            }
        }
    }
}

/// A word with its first letter capitalised, to open a line.
fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A session as the microcycle names it: `Gym 1`, `Cycling 2`.
fn named(session: &Session) -> String {
    format!(
        "{} {}",
        capitalised(session.slot.discipline.as_str()),
        session.number
    )
}

/// A part of a day, by its weekday: `Wednesday evening`.
fn when(part: DayPart) -> String {
    format!("{} {}", part.date.strftime("%A"), part.part)
}
