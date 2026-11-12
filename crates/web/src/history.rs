//! History: the sessions he has done, newest first and by day, and one of
//! them opened (#772).
//!
//! The operator, 2026-11-10 (#749), asked whether measurements belong here:
//! *"Only sessions for now."*

use application::history::{Day, Number, Session};
use domain::{
    cycling::{CanonicalBikePlusRide, CanonicalCyclingSession},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Load, Performed,
        SetKind,
    },
    measure::{Duration, RepCount},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::page::{self, Place};

/// Where a session is opened.
pub fn address(on: Date, number: Number) -> String {
    format!("{}/{on}/{}", Place::History.path(), number.get())
}

/// The feed, or the reason it could not be read.
pub fn page(days: &Result<Vec<Day>, String>, today: Date) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::History.name(), &[])) }
            body {
                main.history {
                    h1 { (Place::History.name()) }
                    @match days {
                        Ok(days) if days.is_empty() => p.sub { "No session is recorded." },
                        Ok(days) => @for day in days { (feed_day(day, today)) },
                        Err(error) => p.failed { "History could not be read: " (error) },
                    }
                    (page::bar(Place::History.path()))
                }
            }
        }
    }
}

fn feed_day(day: &Day, today: Date) -> Markup {
    html! {
        h2.group { (day_name(day.on, today)) }
        ul.sessions {
            @for (number, session) in &day.sessions {
                li {
                    a href=(address(day.on, *number)) {
                        i.dot.gym[matches!(session, Session::Gym(_))]
                            .cycling[matches!(session, Session::Cycling(_))] {}
                        span.name { (name(session)) }
                        span.facts { (facts(session)) }
                    }
                }
            }
        }
    }
}

/// The day, with its year where it is not this one.
fn day_name(on: Date, today: Date) -> String {
    if on.year() == today.year() {
        on.strftime("%a %-d %b").to_string()
    } else {
        on.strftime("%a %-d %b %Y").to_string()
    }
}

/// What the session is called: a ride by the class it was ridden to.
fn name(session: &Session) -> String {
    match session {
        Session::Gym(_) => "Gym session".to_owned(),
        Session::Cycling(session) => session.working_ride().at().value().called().to_owned(),
    }
}

/// Enough of it to recognise it by.
fn facts(session: &Session) -> String {
    match session {
        Session::Gym(session) => gym_facts(session).join(" · "),
        Session::Cycling(session) => [
            length(ridden(session)),
            session.working_ride().average_power().value().to_string(),
        ]
        .join(" · "),
    }
}

fn gym_facts(session: &CanonicalGymSession) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(duration) = session.duration() {
        facts.push(length(Duration::from_seconds(
            duration.value().as_seconds(),
        )));
    }
    facts.push(counted(session.exercises().count(), "exercise"));
    facts.push(counted(session.set_count(), "set"));
    if let Some(heart_rate) = session.heart_rate() {
        facts.push(heart_rate.value().stated().average().to_string());
    }
    facts
}

/// How long every ride of the session took, warm-up and cool-down included.
fn ridden(session: &CanonicalCyclingSession) -> Duration {
    Duration::from_seconds(
        session
            .rides()
            .into_iter()
            .map(|ride| ride.duration().value().as_seconds())
            .sum(),
    )
}

fn counted(count: usize, what: &str) -> String {
    if count == 1 {
        format!("1 {what}")
    } else {
        format!("{count} {what}s")
    }
}

/// A length of time in hours and minutes, or minutes and seconds under one.
fn length(duration: Duration) -> String {
    let seconds = duration.as_seconds();
    let minutes = seconds.saturating_add(30) / 60;
    if seconds < 60 {
        format!("{seconds} s")
    } else if minutes < 60 {
        format!("{minutes} min")
    } else {
        format!("{} h {} min", minutes / 60, minutes % 60)
    }
}

/// One session opened, or why it could not be.
pub fn session(here: &str, on: Date, session: &Result<Option<Session>, String>) -> Markup {
    let title = match session {
        Ok(Some(session)) => name(session),
        Ok(None) | Err(_) => "Session".to_owned(),
    };
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(&title, &[])) }
            body {
                main.history {
                    (page::header(here, &title))
                    @match session {
                        Ok(Some(Session::Gym(session))) => (gym(on, session)),
                        Ok(Some(Session::Cycling(session))) => (cycling(session)),
                        Ok(None) => p.sub { "No such session on " (on.strftime("%A %-d %B %Y")) "." },
                        Err(error) => p.failed { "The session could not be read: " (error) },
                    }
                    (page::bar(here))
                }
            }
        }
    }
}

fn gym(on: Date, session: &CanonicalGymSession) -> Markup {
    let when = session.occurred().instant().map_or_else(
        || on.strftime("%A %-d %B %Y").to_string(),
        |started| {
            started
                .wall_clock()
                .strftime("%A %-d %B %Y, %H:%M")
                .to_string()
        },
    );
    html! {
        p.sub { (when) }
        p.sub { (gym_facts(session).join(" · ")) }
        @if let Some(heart_rate) = session.heart_rate() {
            p.sub { "Highest " (heart_rate.value().stated().highest()) }
        }
        @for item in session.items().iter() {
            @match item {
                CanonicalItem::Exercise(exercise) => section.card.exercise { (exercise_sets(exercise)) },
                CanonicalItem::Superset(members) => section.card.exercise {
                    p.label { "Superset" }
                    @for exercise in members.iter() { (exercise_sets(exercise)) }
                },
            }
        }
    }
}

fn exercise_sets(exercise: &CanonicalExercise) -> Markup {
    let (named, sets): (String, Vec<String>) = match exercise {
        CanonicalExercise::ForReps { identified, sets } => (
            match identified.value() {
                domain::gym::Identified::Recorded(exercise) => exercise.name().to_owned(),
                domain::gym::Identified::Guessed(guess) => guess.exercise().map_or_else(
                    || guess.to_string(),
                    |exercise| format!("{}?", exercise.name()),
                ),
            },
            sets.iter()
                .map(|set| set_line(set, |reps, loaded| reps_line(*reps, loaded)))
                .collect(),
        ),
        CanonicalExercise::ForDuration { exercise, sets } => (
            exercise.value().name().to_owned(),
            sets.iter()
                .map(|set| set_line(set, |held, _| length(*held)))
                .collect(),
        ),
        CanonicalExercise::ForDistance { exercise, sets } => (
            exercise.value().name().to_owned(),
            sets.iter()
                .map(|set| set_line(set, |distance, _| distance.to_string()))
                .collect(),
        ),
    };
    html! {
        h2 { (named) }
        ol.sets { @for set in &sets { li { (set) } } }
    }
}

/// A set as it was performed: its load, how much of it, how far from failure,
/// and whether it was a warm-up.
///
/// **No load said where there was none**: a pogo's ten is ten.
fn set_line<M>(set: &CanonicalSet<M>, measured: impl Fn(&M, bool) -> String) -> String {
    let load = set
        .load
        .as_ref()
        .map(|load| *load.value())
        .filter(|load| !matches!(load, Load::Absolute(mass) if mass.is_none()))
        .map(|load| format!("{load} × "))
        .unwrap_or_default();
    let performed = match set.outcome.value() {
        Performed::Completed(Some(measure)) => measured(measure, !load.is_empty()),
        Performed::Completed(None) => "done".to_owned(),
        Performed::Failed => "failed".to_owned(),
    };
    let reserve = set
        .intensity
        .as_ref()
        .map(|intensity| format!(" @ {} in reserve", intensity.value()))
        .unwrap_or_default();
    let warm_up = if set.kind.as_ref().map(|kind| *kind.value()) == Some(SetKind::Warmup) {
        " (warm-up)"
    } else {
        ""
    };
    format!("{load}{performed}{reserve}{warm_up}")
}

/// Repetitions after a load, as "75 kg × 6", and named where there is none:
/// a pogo's "10 reps".
fn reps_line(reps: RepCount, loaded: bool) -> String {
    match (loaded, reps.as_u32()) {
        (true, _) => reps.to_string(),
        (false, 1) => "1 rep".to_owned(),
        (false, _) => format!("{reps} reps"),
    }
}

fn cycling(session: &CanonicalCyclingSession) -> Markup {
    let parts: Vec<(&str, &CanonicalBikePlusRide)> = match session {
        CanonicalCyclingSession::Ride { main, cool_down } => {
            let mut parts = vec![("Ride", main)];
            parts.extend(cool_down.iter().map(|ride| ("Cool-down", ride)));
            parts
        }
        CanonicalCyclingSession::Test {
            warm_up,
            effort,
            cool_down,
        } => {
            let mut parts = vec![("Warm-up", warm_up), ("Effort", effort)];
            parts.extend(cool_down.iter().map(|ride| ("Cool-down", ride)));
            parts
        }
    };
    html! {
        p.sub { (session.started_at().wall_clock().strftime("%A %-d %B %Y, %H:%M")) }
        p.sub { (length(ridden(session))) }
        @for (part, ride) in parts {
            section.card.ride {
                p.label { (part) }
                h2 { (ride.at().value().called()) }
                p.sub {
                    (length(*ride.duration().value())) " · "
                    (ride.average_power().value()) " average · "
                    (kilometres(*ride.distance().value()))
                }
            }
        }
    }
}

fn kilometres(distance: domain::measure::Metres) -> String {
    let tenths = distance.as_millimetres().saturating_add(50_000) / 100_000;
    format!("{}.{} km", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use application::history::Number;
    use domain::measure::{Duration, Metres, RepCount};
    use jiff::civil::date;

    use super::{address, day_name, kilometres, length, reps_line};

    /// **Lengths as the feed shows them**: 30 Oct's gym session, 25 Oct's test
    /// with its warm-up and cool-down, and a hold.
    #[test]
    fn lengths_read_in_hours_and_minutes() {
        assert_eq!(length(Duration::from_seconds(5_460)), "1 h 31 min");
        assert_eq!(length(Duration::from_seconds(2_100)), "35 min");
        assert_eq!(length(Duration::from_seconds(55)), "55 s");
        assert_eq!(kilometres(Metres::from_millimetres(11_340_000)), "11.3 km");
    }

    /// **A day this year has no year**, one before it does.
    #[test]
    fn a_day_names_its_year_only_when_it_is_not_this_one() {
        let today = date(2026, 11, 12);
        assert_eq!(day_name(date(2026, 10, 30), today), "Fri 30 Oct");
        assert_eq!(day_name(date(2025, 10, 31), today), "Fri 31 Oct 2025");
    }

    /// **Repetitions are named only where no load says what they are of.**
    #[test]
    fn reps_are_named_without_a_load() {
        let ten = RepCount::new(10).expect("ten");
        let one = RepCount::new(1).expect("one");
        assert_eq!(reps_line(ten, true), "10");
        assert_eq!(reps_line(ten, false), "10 reps");
        assert_eq!(reps_line(one, false), "1 rep");
    }

    #[test]
    fn a_session_is_opened_at_its_day_and_number() {
        let first = Number::try_from(1).expect("one");
        assert_eq!(address(date(2026, 10, 30), first), "/history/2026-10-30/1");
    }
}
