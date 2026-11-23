//! History: the activities he has done, newest first and by day, a page at a
//! time as the list is scrolled, and one of them opened (#772, #817).
//!
//! The operator, 2026-11-10 (#749), asked whether measurements belong here:
//! *"Only sessions for now."*

use application::{
    PlayedPadelMatch,
    history::{Activity, Day, Number, Page as Listed, opened},
};
use configurator::edits::{Here, InForce, ToSettle};
use domain::{
    cycling::{CanonicalBikePlusRide, CanonicalCyclingSession},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Load, Performed,
        SetKind, exercise::RepsExercise,
    },
    measure::{Duration, RepCount},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, PreEscaped, html};

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
pub fn page(read: &Result<(Listed, Date, Vec<ToSettle>), String>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::History.name(), &[])) }
            body hx-boost="true" {
                main.history {
                    h1 { (Place::History.name()) }
                    @match read {
                        Ok((listed, today, settle)) => {
                            (crate::settle::list(settle, *today))
                            @if listed.days.is_empty() {
                                p.sub { "No activity is recorded." }
                            } @else {
                                (days(listed, *today))
                            }
                        }
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
pub fn day_name(on: Date, today: Date) -> String {
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

/// One activity opened, with the changes in force on it, or why it could not
/// be.
pub fn session(
    here: &str,
    on: Date,
    session: &Result<(Option<opened::Activity>, Vec<InForce>), String>,
) -> Markup {
    let session = session
        .as_ref()
        .map(|(activity, changes)| (activity.as_ref(), changes));
    let title = match session {
        Ok((Some(session), _)) => opened_name(session),
        Ok((None, _)) | Err(_) => "Activity",
    };
    let marked = session
        .as_ref()
        .ok()
        .and_then(|(activity, _)| *activity)
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
                        Ok((Some(opened::Activity::Gym { session, tested }), changes)) => {
                            (gym(here, on, session, tested))
                            (edits(here, session, changes))
                        }
                        Ok((Some(opened::Activity::Cycling(session)), _)) => (cycling(session)),
                        Ok((Some(opened::Activity::Padel(played)), _)) => (padel(played)),
                        Ok((None, _)) => p.sub { "No such activity on " (on.strftime("%A %-d %B %Y")) "." },
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
    let mut figures: Vec<(&str, String)> = Vec::new();
    if let Some(duration) = session.duration() {
        figures.push((
            "Duration",
            length(Duration::from_seconds(duration.value().as_seconds())),
        ));
    }
    figures.push(("Exercises", session.exercises().count().to_string()));
    figures.push(("Sets", session.set_count().to_string()));
    if let Some(heart_rate) = session.heart_rate() {
        let stated = heart_rate.value().stated();
        figures.push(("Heart rate", stated.average().to_string()));
        figures.push(("Highest", stated.highest().to_string()));
    }
    html! {
        p.when { (CALENDAR) (when) }
        dl.figures {
            @for (label, value) in &figures { div { dt { (label) } dd { (value) } } }
        }
        @for (item, first) in items(session) {
            @match item {
                CanonicalItem::Exercise(exercise) => section.card.exercise { (exercise_sets(here, first, exercise, tested)) },
                CanonicalItem::Superset(members) => section.card.exercise.superset {
                    p.badge { "Superset" }
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
    let named = exercise_name(exercise);
    let (measure, rows): (&str, Vec<Row>) = match exercise {
        CanonicalExercise::ForReps { sets, .. } => (
            "Reps",
            sets.iter()
                .map(|set| row(set, RepCount::to_string))
                .collect(),
        ),
        CanonicalExercise::ForDuration { sets, .. } => (
            "Time",
            sets.iter()
                .map(|set| row(set, |held| length(*held)))
                .collect(),
        ),
        CanonicalExercise::ForDistance { sets, .. } => (
            "Distance",
            sets.iter()
                .map(|set| row(set, ToString::to_string))
                .collect(),
        ),
    };
    let loaded = rows.iter().any(|row| row.load.is_some());
    // A set is changed from its row (#892), whichever source recorded it
    // (#893), where it is counted in reps.
    let editable = matches!(exercise, CanonicalExercise::ForReps { .. });
    let rated = rows.iter().any(|row| row.reserve.is_some());
    let mut working = 0;
    html! {
        div.named {
            span.initials aria-hidden="true" { (initials(&named)) }
            h2 { (named) }
            @if exercise.exercise_key().is_some() {
                a.edit href=(format!("{here}/exercise/{position}")) aria-label="Change" title="Change" { (PEN) }
            }
        }
        @if is_tested { p.badge.test { (ONE_REP_MAX_TEST) } }
        table.sets {
            thead { tr {
                th { "Set" }
                @if loaded { th { "kg" } }
                th { (measure) }
                @if rated { th { "In reserve" } }
                @if editable { th.pen aria-label="Change" {} }
            } }
            tbody {
                @for (index, row) in rows.iter().enumerate() {
                    tr.warm-up[row.warm_up].failed[row.failed] {
                        td.set {
                            @if row.warm_up { "W" } @else {
                                ({ working += 1; working })
                            }
                        }
                        @if loaded { td { (row.load.as_deref().unwrap_or("–")) } }
                        td { (row.performed) }
                        @if rated { td { (row.reserve.as_deref().unwrap_or("–")) } }
                        @if editable {
                            td.pen {
                                // A failed attempt has no reps to anchor a
                                // change on, so it is not offered one.
                                @if !row.failed {
                                    a.edit href=(crate::change_set::address(here, position, index + 1))
                                        aria-label="Change" title="Change" { (PEN) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What an exercise reads as: a guess is marked as one.
fn exercise_name(exercise: &CanonicalExercise) -> String {
    match exercise {
        CanonicalExercise::ForReps { identified, .. } => match identified.value() {
            domain::gym::Identified::Recorded(exercise) => exercise.name().to_owned(),
            domain::gym::Identified::Guessed(guess) => guess.exercise().map_or_else(
                || guess.to_string(),
                |exercise| format!("{}?", exercise.name()),
            ),
        },
        CanonicalExercise::ForDuration { exercise, .. } => exercise.value().name().to_owned(),
        CanonicalExercise::ForDistance { exercise, .. } => exercise.value().name().to_owned(),
    }
}

/// The changes in force on a session, each with why and a way to undo it
/// (#895); nothing where there are none.
fn edits(here: &str, session: &CanonicalGymSession, changes: &[InForce]) -> Markup {
    html! {
        @if !changes.is_empty() {
            section.card.edits {
                h2 { "Changes" }
                ul {
                    @for change in changes {
                        li {
                            p { (said(session, change.here)) }
                            p.sub { "Because " (change.edit.reason) }
                            form method="post" action=(format!("{here}/changed/{}", change.edit.id)) {
                                button {
                                    @if change.sessions > 1 {
                                        "Undo over " (counted(change.sessions, "session"))
                                    } @else { "Undo" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What a change says about the session, as its exercises and sets are
/// named on it.
fn said(session: &CanonicalGymSession, here: Here) -> String {
    match here {
        Here::Exercise { read, becomes } => format!("{} to {}", read.name(), becomes.name()),
        Here::Set {
            position,
            set,
            edit,
        } => session.exercises().nth(position - 1).map_or_else(
            || format!("Set {set}: {edit}"),
            |exercise| {
                format!(
                    "{}, {}: {edit}",
                    exercise_name(exercise),
                    crate::change_set::named(&warm_ups(exercise), set - 1)
                )
            },
        ),
    }
}

/// Which of an exercise's sets were warm-ups, in order.
fn warm_ups(exercise: &CanonicalExercise) -> Vec<bool> {
    fn warm_up<M>(set: &CanonicalSet<M>) -> bool {
        set.kind.as_ref().map(|kind| *kind.value()) == Some(SetKind::Warmup)
    }
    match exercise {
        CanonicalExercise::ForReps { sets, .. } => sets.iter().map(warm_up).collect(),
        CanonicalExercise::ForDuration { sets, .. } => sets.iter().map(warm_up).collect(),
        CanonicalExercise::ForDistance { sets, .. } => sets.iter().map(warm_up).collect(),
    }
}

/// The calendar a session's day is shown beside.
const CALENDAR: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="5" width="18" height="16" rx="2"/><path d="M16 3v4M8 3v4M3 10h18"/></svg>"#,
);

/// The pen an editable value is opened from.
const PEN: PreEscaped<&str> = PreEscaped(
    r#"<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4Z"/></svg>"#,
);

/// The first letters of an exercise's first two words, as its mark.
fn initials(named: &str) -> String {
    named
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .collect()
}

/// A set as a row of its table: its load, how much of it, how far from
/// failure, and whether it was a warm-up.
struct Row {
    warm_up: bool,
    failed: bool,
    load: Option<String>,
    performed: String,
    reserve: Option<String>,
}

/// **No load said where nothing was added to the body** (operator,
/// 2026-11-21): a pogo, a box jump and a dip have none, and an assisted front
/// lever reads -21. Kilograms are the column's.
fn row<M>(set: &CanonicalSet<M>, measured: impl Fn(&M) -> String) -> Row {
    let load = set
        .load
        .as_ref()
        .map(|load| *load.value())
        .and_then(|load| match load {
            Load::Absolute(mass) if mass.is_none() => None,
            Load::Absolute(mass) => Some(mass.to_string()),
            Load::Relative(delta) if delta.as_grams() == 0 => None,
            Load::Relative(delta) if delta.as_grams() < 0 => Some(delta.to_string()),
            Load::Relative(delta) => Some(format!("+{delta}")),
        });
    let (performed, failed) = match set.outcome.value() {
        Performed::Completed(Some(measure)) => (measured(measure), false),
        Performed::Completed(None) => ("done".to_owned(), false),
        Performed::Failed => ("failed".to_owned(), true),
    };
    Row {
        warm_up: set.kind.as_ref().map(|kind| *kind.value()) == Some(SetKind::Warmup),
        failed,
        load,
        performed,
        reserve: set
            .intensity
            .as_ref()
            .map(|intensity| intensity.value().to_string()),
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
    type Read = (Listed, Date, Vec<ToSettle>);

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
        // Settled from the top of the list, so not on a page scrolled to.
        let settle = match self.before {
            None => {
                configurator::history::Settling::open(store, zone)
                    .to_settle()
                    .await?
            }
            Some(_) => Vec::new(),
        };
        Ok((list.page(self.before).await?, list.today(), settle))
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
    type Read = (Option<opened::Activity>, Vec<InForce>);

    fn reads() -> application::graph::Reads {
        configurator::history::opened_page_reads()
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
        let activity = configurator::history::Opened::open(store, zone)
            .activity(self.on, self.number)
            .await?;
        let changes = match &activity {
            Some(opened::Activity::Gym { session, .. }) => {
                configurator::history::Changes::open(store, zone)
                    .of(session)
                    .await?
            }
            _ => Vec::new(),
        };
        Ok((activity, changes))
    }

    fn render(&self, read: Result<Self::Read, crate::drawn::Unreadable>) -> Markup {
        session(&self.here(), self.on, &read.map_err(|error| error.0))
    }
}

#[cfg(test)]
mod tests {
    use application::history::Number;
    use domain::measure::{Duration, Metres};
    use jiff::civil::date;

    use super::{address, day_name, kilometres, length};

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

    #[test]
    fn a_session_is_opened_at_its_day_and_number() {
        let first = Number::try_from(1).expect("one");
        assert_eq!(address(date(2026, 10, 30), first), "/history/2026-10-30/1");
    }
}
