//! Changing when an opened gym session started and ended (#894).
//!
//! The operator, 2026-11-23: *"there are definitely times when I know I pressed
//! the watch part way into a workout … and we know there are times when I left
//! the watch or phone running or the watch or phone died part way through a
//! workout, in those cases it would be useful to either choose the source of
//! the start or end time or provide my own."*
//!
//! **Each is a source's or a time he gives**, and only what is posted
//! differently from what the session reads as now is asserted. A time is
//! read on the session's own day, in the zone it started in.

use std::{collections::BTreeMap, sync::Arc};

use application::{
    PlayedPadelSession,
    history::{Number, opened},
};
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{edits, store::Store};
use domain::{
    canonical::{Occurred, TakenFrom},
    gym::{
        CanonicalGymSession,
        edit::{self, EditId, EditReason, SparseGymSession, Taken, Unmoved},
    },
    landing::LandingStream,
    measure::PositiveDuration,
    normalised::{OperatorZone, StartedAt},
};
use jiff::{
    Timestamp,
    civil::{Date, Time},
};
use maud::{Markup, html};

use crate::{
    change, history,
    input::{self, Refusal},
    page::Place,
    run::{Asked, Runs},
};

/// Where an opened session's start and end are changed.
pub const ROUTE: &str = "/history/{on}/{number}/when";

const TITLE: &str = "Change when";

/// Where a padel session's change of when is undone (#894).
pub const UNDO: &str = "/history/{on}/{number}/when/{edit}";

/// Where the session at `session` has its start and end changed.
pub fn address(session: &str) -> String {
    format!("{session}/when")
}

/// Where the change `edit` to the padel session at `session` is undone.
pub fn undo_address(session: &str, edit: EditId) -> String {
    format!("{session}/when/{edit}")
}

/// What the operator calls a source.
pub fn called(stream: &LandingStream) -> String {
    match stream.source().as_str() {
        "hevy" => "Hevy".to_owned(),
        "garmin" => "the watch".to_owned(),
        "btwb" => "Beyond the Whiteboard".to_owned(),
        "spreadsheets" => "a spreadsheet".to_owned(),
        other => other.to_owned(),
    }
}

/// Where a start or end was taken from, as the page says it.
pub fn by(taken: TakenFrom, session: &CanonicalGymSession) -> String {
    match taken {
        TakenFrom::Edit => "as you said".to_owned(),
        TakenFrom::Normalised(id) => session
            .accounts()
            .iter()
            .find(|account| account.normalised == id)
            .map_or_else(String::new, |account| {
                format!("{}'s", called(&account.stream))
            }),
    }
}

/// A clock time as the page shows it, in `zone`.
fn clock(at: Timestamp, zone: &OperatorZone) -> String {
    at.to_zoned(zone.as_time_zone())
        .strftime("%H:%M:%S")
        .to_string()
}

/// The form as posted or as first drawn: for each bound, which source, or
/// `stated` for the time typed, or nothing to leave it be.
#[derive(Debug, Default)]
pub struct Typed {
    pub started: String,
    pub started_at: String,
    pub ended: String,
    pub ended_at: String,
    pub because: String,
}

impl Typed {
    fn reading(session: &CanonicalGymSession, zone: &OperatorZone) -> Self {
        let zone = zone_of(session, zone);
        Self {
            started_at: session
                .occurred()
                .instant()
                .map(|at| clock(at.instant(), &zone))
                .unwrap_or_default(),
            ended_at: session
                .ended()
                .map(|ended| clock(ended.copied(), &zone))
                .unwrap_or_default(),
            ..Self::default()
        }
    }

    fn of(form: &[(String, String)]) -> Self {
        let mut typed = Self::default();
        for (key, value) in form {
            let into = match key.as_str() {
                "started" => &mut typed.started,
                "started_at" => &mut typed.started_at,
                "ended" => &mut typed.ended,
                "ended_at" => &mut typed.ended_at,
                "because" => &mut typed.because,
                _ => continue,
            };
            value.trim().clone_into(into);
        }
        typed
    }
}

/// The zone a session's times are read in: the one it started in, or the
/// operator's where it has only a day.
fn zone_of(session: &CanonicalGymSession, zone: &OperatorZone) -> OperatorZone {
    session
        .occurred()
        .instant()
        .map_or_else(|| zone.clone(), |at| at.zone().clone())
}

/// A time typed on the session's day, in its zone.
fn instant(
    typed: &str,
    on: Date,
    zone: &OperatorZone,
    field: &'static str,
) -> Result<Timestamp, Refusal> {
    let time: Time = typed
        .parse()
        .map_err(|_| Refusal::of(field, "a time of day, as 18:05".to_owned()))?;
    on.to_datetime(time)
        .to_zoned(zone.as_time_zone())
        .map(|zoned| zoned.timestamp())
        .map_err(|_| Refusal::of(field, "a time that happened on that day".to_owned()))
}

/// The edit a post asks for, and why: each bound posted differently from
/// what the session reads as now.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault, or of the whole form where nothing
/// was changed or the two cannot be one session's.
pub fn asked(
    session: &CanonicalGymSession,
    zone: &OperatorZone,
    typed: &Typed,
) -> Result<(SparseGymSession, EditReason), Refusal> {
    let on = session.occurred().day();
    let zone = zone_of(session, zone);
    let started = match typed.started.as_str() {
        "" => None,
        "stated" => Some(Taken::Stated(Occurred::At(StartedAt::new(
            instant(&typed.started_at, on, &zone, "started_at")?,
            zone.clone(),
        )))),
        stream => Some(Taken::From(stream.parse::<LandingStream>().map_err(
            |_| Refusal::of("started", "choose a source, or the time below".to_owned()),
        )?)),
    };
    let ended = match typed.ended.as_str() {
        "" => None,
        "stated" => Some(Taken::Stated(instant(
            &typed.ended_at,
            on,
            &zone,
            "ended_at",
        )?)),
        stream => Some(Taken::From(stream.parse::<LandingStream>().map_err(
            |_| Refusal::of("ended", "choose a source, or the time below".to_owned()),
        )?)),
    };
    let because = EditReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    let sparse = edit::moved(session, started, ended).map_err(|unmoved| match unmoved {
        Unmoved::NoStart => Refusal::of("started", unmoved.to_string()),
        Unmoved::NoEnd => Refusal::of("ended", unmoved.to_string()),
        Unmoved::Unchanged | Unmoved::Invalid(_) => Refusal::whole(unmoved.to_string()),
    })?;
    Ok((sparse, because))
}

/// The form.
pub fn form(
    here: &str,
    session: &CanonicalGymSession,
    zone: &OperatorZone,
    typed: &Typed,
    refused: Option<&Refusal>,
) -> Markup {
    let zone = zone_of(session, zone);
    let starts: Vec<(&LandingStream, Timestamp)> = session
        .accounts()
        .iter()
        .filter_map(|account| Some((&account.stream, account.occurred.instant()?.instant())))
        .collect();
    let ends: Vec<(&LandingStream, Timestamp)> = session
        .accounts()
        .iter()
        .filter_map(|account| Some((&account.stream, account.ended()?)))
        .collect();
    let now_started = session.occurred().instant().map_or_else(
        || "a day, with no time".to_owned(),
        |at| {
            format!(
                "{}, {}",
                clock(at.instant(), &zone),
                by(session.started().taken_from(), session)
            )
        },
    );
    let now_ended = session.ended().map_or_else(
        || "no time".to_owned(),
        |ended| {
            format!(
                "{}, {}",
                clock(ended.copied(), &zone),
                by(ended.taken_from(), session)
            )
        },
    );
    let choose =
        |name: &'static str, now: &str, from: &[(&LandingStream, Timestamp)], chosen: &str| {
            html! {
                select id=(name) name=(name) {
                    option value="" selected[chosen.is_empty()] { "As it reads: " (now) }
                    @for (stream, at) in from {
                        @let value = stream.to_string();
                        option value=(value) selected[chosen == value] {
                            (called(stream)) ": " (clock(*at, &zone))
                        }
                    }
                    option value="stated" selected[chosen == "stated"] { "The time below" }
                }
            }
        };
    input::page(
        here,
        TITLE,
        &html! {
            h2 { (session.occurred().day().strftime("%A %-d %B %Y")) }
            form.card method="post" {
                (input::field("started", "started", "Started", &choose("started", &now_started, &starts, &typed.started), refused))
                (input::field("started_at", "started_at", "Start time", &html! {
                    input id="started_at" name="started_at" type="time" step="1" value=(typed.started_at);
                }, refused))
                (input::field("ended", "ended", "Ended", &choose("ended", &now_ended, &ends, &typed.ended), refused))
                (input::field("ended_at", "ended_at", "End time", &html! {
                    input id="ended_at" name="ended_at" type="time" step="1" value=(typed.ended_at);
                }, refused))
                (input::field("because", "because", "Because", &html! {
                    input id="because" name="because" value=(typed.because) required;
                }, refused))
                (input::whole(refused))
                button { "Change" }
            }
            p { a href=(here) { "Back to the session" } }
        },
    )
}

/// The session a change is asked of.
struct At {
    on: Date,
    number: Number,
}

impl At {
    fn of(on: &str, number: &str) -> Option<Self> {
        Some(Self {
            on: on.parse().ok()?,
            number: number
                .parse::<usize>()
                .ok()
                .and_then(|number| Number::try_from(number).ok())?,
        })
    }

    fn session(&self) -> String {
        history::address(self.on, self.number)
    }
}

/// An opened session whose start and end can be changed.
enum Bounded {
    Gym(Box<CanonicalGymSession>),
    Padel(Box<PlayedPadelSession>),
}

/// The opened session, or the page to show instead.
async fn found(store: &Store, zone: &OperatorZone, at: &At) -> Result<Bounded, Response> {
    let here = at.session();
    let activity = configurator::history::Opened::open(store, zone)
        .activity(at.on, at.number)
        .await
        .map_err(|error| input::unreadable(&here, TITLE, &error.to_string()).into_response())?;
    match activity {
        Some(opened::Activity::Gym { session, .. }) => Ok(Bounded::Gym(session)),
        Some(opened::Activity::Padel(played)) => Ok(Bounded::Padel(played)),
        _ => Err(Redirect::to(&here).into_response()),
    }
}

/// A padel session's start and end as the page shows them, in the zone it
/// started in.
fn padel_reading(played: &PlayedPadelSession) -> Typed {
    let zone = played.started_at().zone();
    Typed {
        started_at: clock(played.started_at().instant(), zone),
        ended_at: clock(played.ended(), zone),
        ..Typed::default()
    }
}

/// The change a post asks of a padel session, and why: both times, read on
/// its day in its zone, as a start and how long it lasted.
///
/// # Errors
///
/// [`Refusal`] beside the time at fault, or of the whole form where the two
/// cannot be the session's or nothing was changed.
pub fn padel_asked(
    played: &PlayedPadelSession,
    typed: &Typed,
) -> Result<(StartedAt, PositiveDuration, EditReason), Refusal> {
    let on = played.started_at().wall_clock().date();
    let zone = played.started_at().zone();
    let start = instant(&typed.started_at, on, zone, "started_at")?;
    let end = instant(&typed.ended_at, on, zone, "ended_at")?;
    let because = EditReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    let (started, duration) = played
        .moved(start, end)
        .map_err(|unmoved| Refusal::whole(unmoved.to_string()))?;
    Ok((started, duration, because))
}

/// The form for a padel session: the watch is its only source, so there is
/// a start and an end to type and nothing to choose.
fn padel_form(
    here: &str,
    played: &PlayedPadelSession,
    typed: &Typed,
    refused: Option<&Refusal>,
) -> Markup {
    input::page(
        here,
        TITLE,
        &html! {
            h2 { (played.started_at().wall_clock().strftime("%A %-d %B %Y")) }
            form.card method="post" {
                (input::field("started_at", "started_at", "Start time", &html! {
                    input id="started_at" name="started_at" type="time" step="1" value=(typed.started_at) required;
                }, refused))
                (input::field("ended_at", "ended_at", "End time", &html! {
                    input id="ended_at" name="ended_at" type="time" step="1" value=(typed.ended_at) required;
                }, refused))
                (input::field("because", "because", "Because", &html! {
                    input id="because" name="because" value=(typed.because) required;
                }, refused))
                (input::whole(refused))
                button { "Change" }
            }
            p { a href=(here) { "Back to the session" } }
        },
    )
}

/// The form, filled with what the session reads as now.
pub async fn shown(
    State(runs): State<Arc<Runs>>,
    Path((on, number)): Path<(String, String)>,
) -> Response {
    let Some(at) = At::of(&on, &number) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let zone = &runs.setting().zone;
    match found(&runs.setting().store, zone, &at).await {
        Ok(Bounded::Gym(session)) => form(
            &at.session(),
            &session,
            zone,
            &Typed::reading(&session, zone),
            None,
        )
        .into_response(),
        Ok(Bounded::Padel(played)) => {
            padel_form(&at.session(), &played, &padel_reading(&played), None).into_response()
        }
        Err(shown) => shown,
    }
}

/// Make the change, then show what it changed; or the form again, with why
/// it was refused.
pub async fn posted(
    State(runs): State<Arc<Runs>>,
    Path((on, number)): Path<(String, String)>,
    Form(form_posted): Form<Vec<(String, String)>>,
) -> Response {
    let Some(at) = At::of(&on, &number) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let here = at.session();
    let session = match found(store, zone, &at).await {
        Ok(Bounded::Gym(session)) => session,
        Ok(Bounded::Padel(played)) => {
            let typed = Typed::of(&form_posted);
            let (started, duration, because) = match padel_asked(&played, &typed) {
                Ok(asked) => asked,
                Err(refusal) => {
                    return padel_form(&here, &played, &typed, Some(&refusal)).into_response();
                }
            };
            return match edits::change_padel(store, &because, played.session(), &started, duration)
                .await
            {
                Ok(_) => Redirect::to(&here).into_response(),
                Err(error) => padel_form(
                    &here,
                    &played,
                    &typed,
                    Some(&Refusal::whole(error.to_string())),
                )
                .into_response(),
            };
        }
        Err(shown) => return shown,
    };
    let typed = Typed::of(&form_posted);
    let refused =
        |refusal: &Refusal| form(&here, &session, zone, &typed, Some(refusal)).into_response();
    let (sparse, because) = match asked(&session, zone, &typed) {
        Ok(asked) => asked,
        Err(refusal) => return refused(&refusal),
    };
    let ids = match session.exercises().next() {
        Some(exercise) => edits::ids_of(store, exercise).await,
        None => Ok(None),
    };
    let held = match ids {
        Ok(Some((held, _))) => held,
        Ok(None) => {
            return refused(&Refusal::whole(
                "the session is being rebuilt; open it again in a moment".to_owned(),
            ));
        }
        Err(error) => return refused(&Refusal::whole(error.to_string())),
    };
    let sessions = BTreeMap::from([(held, sparse)]);
    match edits::assert(store, &because, &sessions, &BTreeMap::new()).await {
        Ok(id) => {
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&change::changed_address(at.on, at.number, id)).into_response()
        }
        Err(error) => refused(&Refusal::whole(error.to_string())),
    }
}

/// Undo a change of when a padel session started and ended, then go back to
/// it.
pub async fn undone(
    State(runs): State<Arc<Runs>>,
    Path((on, number, edit)): Path<(String, String, String)>,
) -> Response {
    let (Some(at), Some(id)) = (
        At::of(&on, &number),
        edit.parse::<i64>().ok().map(EditId::from),
    ) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let here = at.session();
    match edits::undo_padel(&runs.setting().store, id).await {
        Ok(()) => Redirect::to(&here).into_response(),
        Err(error) => input::unreadable(&here, TITLE, &error.to_string()).into_response(),
    }
}
