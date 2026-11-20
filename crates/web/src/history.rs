//! History: the activities he has done, newest first and by day, a page at a
//! time as the list is scrolled, and one of them opened (#772, #817).
//!
//! The operator, 2026-11-10 (#749), asked whether measurements belong here:
//! *"Only sessions for now."*

use application::{
    PlayedPadelMatch,
    history::{Activity, Day, Number, Page as Listed, opened},
};
use domain::{
    cycling::{CanonicalBikePlusRide, CanonicalCyclingSession},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Load, Performed,
        SetKind, exercise::RepsExercise,
    },
    measure::{Duration, RepCount},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::page::{self, Place};

/// Where an activity is opened.
pub fn address(on: Date, number: Number) -> String {
    format!("{}/{on}/{}", Place::History.path(), number.get())
}

/// Where the list's page of days before `before` is.
fn before_address(before: Date) -> String {
    format!("{}?before={before}", Place::History.path())
}

/// A page of the list, or the reason it could not be read.
///
/// **Scrolled in a page at a time** (operator, 2026-11-15: *"pagination on the
/// backend and infinite scroll on the front end"*): the end of a page asks for
/// the next as it comes into view, and the next replaces it.
pub fn page(read: &Result<(Listed, Date), String>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::History.name(), &[])) }
            body hx-boost="true" {
                main.history {
                    h1 { (Place::History.name()) }
                    @match read {
                        Ok((listed, _)) if listed.days.is_empty() => p.sub { "No activity is recorded." },
                        Ok((listed, today)) => (days(listed, *today)),
                        Err(error) => p.failed { "History could not be read: " (error) },
                    }
                    (page::bar(Place::History.path()))
                }
            }
        }
    }
}

/// One page's days, and where the next page is asked for.
fn days(listed: &Listed, today: Date) -> Markup {
    html! {
        section.days {
            @for day in &listed.days { (feed_day(day, today)) }
            @if let Some(before) = listed.before {
                div.more
                    hx-get=(before_address(before))
                    hx-trigger="revealed"
                    hx-select="section.days"
                    hx-swap="outerHTML" {}
            }
        }
    }
}

fn feed_day(day: &Day, today: Date) -> Markup {
    html! {
        h2.group { (day_name(day.on, today)) }
        ul.sessions {
            @for (number, activity) in &day.activities {
                li {
                    a href=(address(day.on, *number)) {
                        i.dot.gym[matches!(activity, Activity::Gym { .. })]
                            .cycling[matches!(activity, Activity::Cycling { .. })]
                            .padel[matches!(activity, Activity::Padel { .. })] {}
                        span.name { (name(activity)) }
                        @if let Some(test) = test(activity) { " " em.test { (test) } }
                        span.facts { (facts(activity)) }
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

/// The test it was, where it was one.
const fn test(activity: &Activity) -> Option<&'static str> {
    match activity {
        _ if !activity.is_test() => None,
        Activity::Gym { .. } => Some(ONE_REP_MAX_TEST),
        Activity::Cycling { .. } => Some(FTP_TEST),
        Activity::Padel { .. } => None,
    }
}

/// The test an opened activity was, where it was one.
const fn opened_test(activity: &opened::Activity) -> Option<&'static str> {
    match activity {
        _ if !activity.is_test() => None,
        opened::Activity::Gym { .. } => Some(ONE_REP_MAX_TEST),
        opened::Activity::Cycling(_) => Some(FTP_TEST),
        opened::Activity::Padel(_) => None,
    }
}

const ONE_REP_MAX_TEST: &str = "1RM test";
const FTP_TEST: &str = "FTP test";
const GYM: &str = "Gym session";
const PELOTON: &str = "Peloton session";
const PADEL: &str = "Padel match";

/// What the activity is: the classes a cycling session was ridden to are on
/// its page, not its name.
const fn name(activity: &Activity) -> &'static str {
    match activity {
        Activity::Gym { .. } => GYM,
        Activity::Cycling { .. } => PELOTON,
        Activity::Padel { .. } => PADEL,
    }
}

const fn opened_name(activity: &opened::Activity) -> &'static str {
    match activity {
        opened::Activity::Gym { .. } => GYM,
        opened::Activity::Cycling(_) => PELOTON,
        opened::Activity::Padel(_) => PADEL,
    }
}

/// Enough of it to recognise it by.
fn facts(activity: &Activity) -> String {
    let facts = match activity {
        Activity::Gym {
            length: lasted,
            exercises,
            sets,
            heart_rate,
            ..
        } => {
            let mut facts: Vec<String> = lasted
                .iter()
                .map(|lasted| length(Duration::from_seconds(lasted.as_seconds())))
                .collect();
            facts.push(counted(*exercises, "exercise"));
            facts.push(counted(*sets, "set"));
            facts.extend(heart_rate.map(|heart_rate| heart_rate.to_string()));
            facts
        }
        Activity::Cycling {
            length: ridden,
            power,
            ..
        } => vec![length(*ridden), power.to_string()],
        Activity::Padel {
            length: played,
            heart_rate,
        } => {
            let mut facts = vec![length(Duration::from_seconds(played.as_seconds()))];
            facts.extend(heart_rate.map(|heart_rate| heart_rate.to_string()));
            facts
        }
    };
    facts.join(" · ")
}

fn padel_facts(played: &PlayedPadelMatch) -> Vec<String> {
    let mut facts = vec![length(Duration::from_seconds(played.duration.as_seconds()))];
    facts.extend(
        played
            .heart_rate
            .map(|heart_rate| heart_rate.average().to_string()),
    );
    facts
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

/// One activity opened, or why it could not be.
pub fn session(here: &str, on: Date, session: &Result<Option<opened::Activity>, String>) -> Markup {
    let title = match session {
        Ok(Some(session)) => opened_name(session),
        Ok(None) | Err(_) => "Activity",
    };
    let marked = session
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .and_then(opened_test);
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(title, &[])) }
            body hx-boost="true" {
                main.history {
                    (page::header(here, title))
                    @if let Some(test) = marked { p.test { (test) } }
                    @match session {
                        Ok(Some(opened::Activity::Gym { session, tested })) => (gym(here, on, session, tested)),
                        Ok(Some(opened::Activity::Cycling(session))) => (cycling(session)),
                        Ok(Some(opened::Activity::Padel(played))) => (padel(played)),
                        Ok(None) => p.sub { "No such activity on " (on.strftime("%A %-d %B %Y")) "." },
                        Err(error) => p.failed { "The activity could not be read: " (error) },
                    }
                    (page::bar(here))
                }
            }
        }
    }
}

fn padel(played: &PlayedPadelMatch) -> Markup {
    html! {
        p.sub { (played.started_at.wall_clock().strftime("%A %-d %B %Y, %H:%M")) }
        p.sub { (padel_facts(played).join(" · ")) }
        @if let Some(heart_rate) = played.heart_rate {
            p.sub { "Highest " (heart_rate.highest()) }
        }
    }
}

fn gym(here: &str, on: Date, session: &CanonicalGymSession, tested: &[RepsExercise]) -> Markup {
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
        @for (item, first) in items(session) {
            @match item {
                CanonicalItem::Exercise(exercise) => section.card.exercise { (exercise_sets(here, first, exercise, tested)) },
                CanonicalItem::Superset(members) => section.card.exercise {
                    p.label { "Superset" }
                    @for (offset, exercise) in members.iter().enumerate() {
                        (exercise_sets(here, first + offset, exercise, tested))
                    }
                },
            }
        }
    }
}

/// Each item, with the position of its first exercise among the session's,
/// from one: what a change of exercise is addressed by (#754).
fn items(session: &CanonicalGymSession) -> Vec<(&CanonicalItem, usize)> {
    let mut first = 1;
    session
        .items()
        .iter()
        .map(|item| {
            let at = first;
            first += match item {
                CanonicalItem::Exercise(_) => 1,
                CanonicalItem::Superset(members) => members.iter().count(),
            };
            (item, at)
        })
        .collect()
}

fn exercise_sets(
    here: &str,
    position: usize,
    exercise: &CanonicalExercise,
    tested: &[RepsExercise],
) -> Markup {
    let is_tested = tested
        .iter()
        .any(|lift| exercise.exercise_key() == Some(lift.as_str()));
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
        @if is_tested { p.label { (ONE_REP_MAX_TEST) } }
        h2 {
            (named)
            @if exercise.exercise_key().is_some() {
                " " a.change href=(format!("{here}/exercise/{position}")) { "Change" }
            }
        }
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

/// A page of History, once a run has collected what it reads (#772): the
/// newest, or the one before a day.
#[derive(Debug)]
pub struct Page {
    before: Option<Date>,
}

impl crate::drawn::Drawn for Page {
    const ROUTE: &'static str = Place::History.path();
    type Read = (Listed, Date);

    fn reads() -> application::graph::Reads {
        configurator::history::reads()
    }

    fn asked(
        _path: &axum::extract::RawPathParams,
        query: &std::collections::HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        query
            .get("before")
            .map_or(Ok(Self { before: None }), |before| {
                before
                    .parse::<Date>()
                    .map(|before| Self {
                        before: Some(before),
                    })
                    .map_err(|_| axum::response::Redirect::to(Place::History.path()))
            })
    }

    fn here(&self) -> String {
        Place::History.path().to_owned()
    }

    fn address(&self) -> String {
        self.before.map_or_else(|| self.here(), before_address)
    }

    async fn read(
        &self,
        store: &configurator::store::Store,
        zone: &domain::normalised::OperatorZone,
    ) -> Result<Self::Read, crate::drawn::Unreadable> {
        let list = configurator::history::List::open(store, zone);
        Ok((list.page(self.before).await?, list.today()))
    }

    fn render(&self, read: Result<Self::Read, crate::drawn::Unreadable>) -> Markup {
        page(&read.map_err(|error| error.0))
    }
}

/// One activity opened from History (#772): the `number`th done `on`.
///
/// **No run of its own**: History started one as it was opened, and this
/// waits on it as any page reading the same layers does.
#[derive(Debug)]
pub struct Opened {
    on: Date,
    number: Number,
}

impl crate::drawn::Drawn for Opened {
    const ROUTE: &'static str = "/history/{on}/{number}";
    const OPENS: bool = false;
    type Read = Option<opened::Activity>;

    fn reads() -> application::graph::Reads {
        configurator::history::opened_reads()
    }

    fn asked(
        path: &axum::extract::RawPathParams,
        _query: &std::collections::HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        let named = |name: &str| {
            path.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value)
        };
        let on = named("on").and_then(|on| on.parse::<Date>().ok());
        let number = named("number")
            .and_then(|number| number.parse::<usize>().ok())
            .and_then(|number| Number::try_from(number).ok());
        match (on, number) {
            (Some(on), Some(number)) => Ok(Self { on, number }),
            _ => Err(axum::response::Redirect::to(Place::History.path())),
        }
    }

    fn here(&self) -> String {
        address(self.on, self.number)
    }

    async fn read(
        &self,
        store: &configurator::store::Store,
        zone: &domain::normalised::OperatorZone,
    ) -> Result<Self::Read, crate::drawn::Unreadable> {
        Ok(configurator::history::Opened::open(store, zone)
            .activity(self.on, self.number)
            .await?)
    }

    fn render(&self, read: Result<Self::Read, crate::drawn::Unreadable>) -> Markup {
        session(&self.here(), self.on, &read.map_err(|error| error.0))
    }
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
