//! What a run of `next` did, and what is next, as a page.
//!
//! **The terminal's words, not its layout**, the columns the terminal pads
//! with spaces being lists and tables, because a phone is narrower than any
//! terminal. **Except where a session went**, which is one line here (#666):
//! the terminal says what was replaced and what the stack holds.

use std::fmt::Write as _;

use application::{
    Delivery, Prescription, Unexpressed,
    cycling::PrescribedRide,
    microcycle::Session,
    next::{CyclingPrescribed, Done, Found, GymPrescribed, Prescribed, Report, Step, WhatIsNext},
};
use domain::{
    cycling::{CyclingMicrocycle, Ride, clock},
    gym::Load,
    measure::{Duration, PositiveDuration, Spans},
    planner::{Discipline, ScheduledSlot},
    prescription::{Prescribed as Set, PrescribedExercise, PrescribedSet, Target},
    schedule::DayPart,
};
use maud::{Markup, html};

/// A step of the run, named.
pub fn step(step: &Step) -> String {
    match step {
        Step::Joining(parts) => joining(parts),
        Step::Fetching(stream) => format!("Fetching {stream}"),
        Step::Normalising(stream) => format!("Normalising {stream}"),
        Step::Canonicalising(discipline) => {
            format!("Canonicalising {} sessions", discipline.as_str())
        }
        Step::CanonicalisingWeighIns => "Canonicalising weigh-ins".to_owned(),
        // **The configurator knows which catalogue this build keeps**, as it
        // knows where rides are done.
        Step::RefreshingClasses => "Refreshing Peloton's classes".to_owned(),
        Step::Committing => "Committing the next mesocycle".to_owned(),
        Step::Skipping => "Skipping".to_owned(),
    }
}

/// The runs a run joined, by what they collect: the whole of `next`, or the
/// streams and canonical layers (#559).
fn joining(parts: &std::collections::BTreeSet<application::Part>) -> String {
    use application::Part;
    if parts.contains(&Part::Delivering) {
        return "Joining the run of next going".to_owned();
    }
    let mut named = std::collections::BTreeSet::new();
    for part in parts {
        named.insert(match part {
            Part::Fetching(stream) | Part::Normalising(stream) => stream.to_string(),
            Part::Canonicalising(layer) => format!("canonical {}", layer.as_str()),
            Part::Delivering => continue,
        });
    }
    format!(
        "Joining the run collecting {}",
        named.into_iter().collect::<Vec<_>>().join(", ")
    )
}

/// How a step ended, and how long it took (#560).
pub fn outcome(done: &Done) -> String {
    format!("{} ({:.1} s)", ended(done), done.took.as_secs_f64())
}

/// Done with a count of what it found, or failed and why.
fn ended(done: &Done) -> String {
    let found = match &done.outcome {
        Err(failure) => return format!("failed: {failure}"),
        Ok(Found::NotDue(why)) => return why.to_string(),
        Ok(Found::Unchanged) => return "unchanged".to_owned(),
        Ok(Found::Joined) => return "done".to_owned(),
        Ok(found) => found,
    };
    let what = match found {
        Found::NotDue(_) | Found::Unchanged | Found::Joined => String::new(),
        Found::Landed(records) if records.as_usize() == 0 => "nothing new".to_owned(),
        Found::Landed(records) => format!("{records} new"),
        Found::Written(written) => format!("{written} written"),
        Found::Canonical(sessions) => format!("{sessions} sessions"),
        Found::CanonicalWeighIns { written, left_out } => {
            let mut said = format!("{written} weigh-ins");
            for left_out in left_out {
                let _ = write!(said, "; left out {left_out}");
            }
            said
        }
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

/// What is next, then this microcycle's sessions (#666), then the curves.
///
/// **Not where each discipline stands in its mesocycle**: the operator,
/// 2026-11-09, *"isn't very meaningful"*. The volume and intensity curves
/// replace it (#742).
pub fn report(report: &Report) -> Markup {
    html! {
        section.card.next {
            p.label { "Next" }
            (what_is_next(&report.next))
        }
        @if let Some((_, sessions)) = &report.plan.microcycle {
            section.card.microcycle {
                h2 { "This microcycle" }
                @for session in sessions {
                    div.row {
                        span { (named(session)) }
                        span.on { (session.slot.date.strftime("%a")) " " (session.slot.slot.part) }
                        span.state { (session.state) }
                    }
                }
            }
        }
        (crate::curves::card(&report.plan.macrocycle, &report.plan.curves))
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
        WhatIsNext::Ill { since } => {
            return html! {
                p { "ill since " (since) ": nothing is prescribed until " code { "fitness schedule better" } }
            };
        }
    };
    // **Not "until midnight"**, which adds nothing (#666).
    let until = match session.closes {
        _ if session.closes_at_midnight() => String::new(),
        Some(closes) => format!(", until {}", when(closes)),
        None => String::new(),
    };
    html! {
        p.session { (named(session)) ", " (when(session.at())) (until) }
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
                    Err(_) => (not_delivered(Discipline::Gym)),
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

/// A session that did not reach its destination, in one line. Why is in
/// telemetry (#741): the operator, 2026-11-09, *"I just need to know that it
/// didn't work and that why is stored somewhere."*
fn not_delivered(discipline: Discipline) -> Markup {
    html! { p.status { i.dot.failed {} "Not in " (destination(discipline)) } }
}

/// Where a discipline's sessions go, as a name to read.
fn destination(discipline: Discipline) -> String {
    configurator::next::destination(discipline).map_or_else(
        |_| capitalised(discipline.as_str()),
        |name| capitalised(name.as_str()),
    )
}

/// The session itself, drawn as Hevy draws a routine (#666): its blocks, then
/// each exercise with a row per set, the members of a superset boxed together,
/// and any slot that could not be derived.
fn gym_session(issued: &Prescription) -> Markup {
    let mut blocks: Vec<(String, Vec<Vec<&PrescribedExercise>>)> = Vec::new();
    for item in issued.workout.shape().items().iter() {
        let Some(slot) = item.slots().next() else {
            continue;
        };
        let block = slot.section().to_string();
        if blocks.last().is_none_or(|(last, _)| *last != block) {
            blocks.push((block, Vec::new()));
        }
        if let Some((_, items)) = blocks.last_mut() {
            items.push(item.exercises().collect());
        }
    }
    html! {
        @for (block, items) in &blocks {
            h3 { (block) }
            @for members in items {
                @if members.len() > 1 {
                    div.superset {
                        p.badge { "Superset" }
                        @for member in members { (exercise(member)) }
                    }
                } @else {
                    @for member in members { (exercise(member)) }
                }
            }
        }
        @if !issued.underivable.is_empty() {
            ul.failed {
                @for slot in &issued.underivable {
                    li { (slot.slot) " (" (slot.exercise.name()) ") — not derivable: " (slot.reason) }
                }
            }
        }
    }
}

/// One exercise: its name, the bands that assist it, and its sets.
fn exercise(prescribed: &PrescribedExercise) -> Markup {
    let (sets, bands) = match prescribed {
        PrescribedExercise::ForReps { sets, assisted, .. } => (
            set_lines(sets.iter()),
            assisted.as_ref().map(ToString::to_string),
        ),
        PrescribedExercise::ForDuration { sets, .. } => (set_lines(sets.iter()), None),
        PrescribedExercise::ForDistance { sets, .. } => (set_lines(sets.iter()), None),
    };
    html! {
        div.exercise {
            p.name { (prescribed.exercise().name()) }
            @if let Some(bands) = bands { p.note { (bands) } }
            (sets)
        }
    }
}

/// An exercise's sets as lines, repeats collapsed to `3 × 10`: the warm-ups on
/// one quiet line, then the working sets. The operator, 2026-11-09: *"we
/// don't need a row for each, 3x 10 etc is enough."* A rest every working set
/// shares is said once, above them.
fn set_lines<'a, M>(sets: impl Iterator<Item = &'a PrescribedSet<M>>) -> Markup
where
    M: Spans + std::fmt::Display + 'a,
{
    let (warmups, working): (Vec<&PrescribedSet<M>>, Vec<&PrescribedSet<M>>) =
        sets.partition(|set| set.warmup);
    let rests: Vec<Option<&Target<Duration>>> = working.iter().map(|set| rest(set)).collect();
    let shared = match rests.first() {
        Some(Some(first)) if rests.iter().all(|rest| *rest == Some(*first)) => Some(*first),
        _ => None,
    };
    let warmups = collapsed(warmups.iter().map(|set| set_line(set)));
    let working = collapsed(working.iter().map(|set| {
        let line = set_line(set);
        match rest(set) {
            Some(rest) if shared.is_none() => format!("{line}, rest {rest}"),
            _ => line,
        }
    }));
    html! {
        @if let Some(rest) = shared { p.note { "Rest " (rest) } }
        @if !warmups.is_empty() { p.warmup { "Warm-up " (warmups.join(", ")) } }
        @for line in &working { p.sets { (line) } }
    }
}

/// The rest after a set, unless it is none, which is how a superset's members
/// follow one another.
fn rest<M: Spans>(set: &PrescribedSet<M>) -> Option<&Target<Duration>> {
    set.rest_after
        .as_ref()
        .filter(|rest| rest.maximum() != Duration::ZERO)
}

/// Lines in order, a run of the same line written once with its count.
fn collapsed(lines: impl Iterator<Item = String>) -> Vec<String> {
    let mut runs: Vec<(String, usize)> = Vec::new();
    for line in lines {
        match runs.last_mut() {
            Some((last, count)) if *last == line => *count += 1,
            _ => runs.push((line, 1)),
        }
    }
    runs.into_iter()
        .map(|(line, count)| {
            if count == 1 {
                line
            } else {
                format!("{count} × {line}")
            }
        })
        .collect()
}

/// One set: what it asks for, the load where there is one, and the effort.
fn set_line<M: Spans + std::fmt::Display>(set: &PrescribedSet<M>) -> String {
    let asked = match &set.prescription {
        Set::Fixed { measure, .. } => measure.to_string(),
        Set::ToEffort {
            predicted: Some(predicted),
            ..
        } => format!("as many as ({predicted})"),
        Set::ToEffort { .. } => "as many as".to_owned(),
    };
    let loaded = match weight(set.prescription.load()) {
        Some(weight) => format!("{asked} @ {weight}"),
        None => asked,
    };
    match set.prescription.effort() {
        Some(effort) => format!("{loaded}, {effort} in reserve"),
        None => loaded,
    }
}

/// A load, written the short way, or nothing for a set without one: plain
/// bodyweight is no load to write.
fn weight(load: Load) -> Option<String> {
    match load {
        _ if unloaded(load) || load == Load::BODYWEIGHT => None,
        Load::Absolute(mass) => Some(format!("{mass}kg")),
        Load::Relative(delta) if delta.as_grams() < 0 => Some(format!("bodyweight {delta}kg")),
        Load::Relative(delta) => Some(format!("bodyweight +{delta}kg")),
    }
}

/// External load that is simply absent, as against bodyweight on an axis where
/// assistance is conventional.
const fn unloaded(load: Load) -> bool {
    matches!(load, Load::Absolute(mass) if mass.is_none())
}

/// Where the session went, in one line, and what it could not say there.
fn gym_delivery(delivered: &Delivery) -> Markup {
    html! {
        p.status {
            i.dot.done {} "In " (capitalised(delivered.destination.as_str()))
            ", session " (delivered.ordinal)
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
                @for item in unexpressed { li { (item.exercise.name()) " — " (item.reason) } }
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
            Ok(_) => p.status { i.dot.done {} "In " (destination(Discipline::Cycling)) },
            Err(_) => (not_delivered(Discipline::Cycling)),
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
