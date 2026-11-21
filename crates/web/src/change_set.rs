//! Changing one set of an opened session: its reps, its load, whether the
//! load was assistance, and its effort (#892).
//!
//! **One set, never the similar ones.** The operator, 2026-11-21: *"There's
//! nothing necessarily similar about two different sets of 10 x 70kg. The
//! idea behind similar sets was to be able to change exercises."*
//!
//! **Only what was changed is asserted.** A field posted as it already reads
//! is left out of the edit, so the source's own value, or an earlier edit's,
//! still stands for it.
//!
//! **Which of Hevy's sets it is, is found by its place**: the session's sets
//! of the exercise and that day's Hevy sets of it, in the order performed.
//! What is stored is Hevy's own identity for the set (§ II.2), so the place is
//! only how the page finds it. Where the two counts differ there is no telling
//! which set is meant, and the page says so rather than guessing.

use std::{str::FromStr as _, sync::Arc};

use application::history::{Number, opened};
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{
    corrections::{self, LoggedSet},
    store::Store,
};
use domain::{
    gym::{CanonicalExercise, exercise::Exercise, intensity::Rir},
    measure::{Kg, RepCount},
    normalised::{CorrectionReason, OperatorZone, SetEdit, SetFigures, StatedLoad},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    change, history,
    input::{self, Refusal},
    page::Place,
    run::{Asked, Runs},
};

/// Where a set of an opened session is changed.
pub const ROUTE: &str = "/history/{on}/{number}/exercise/{position}/set/{set}";

const TITLE: &str = "Change set";

/// Where set `set` of the exercise at `position` is changed, from the
/// session's page at `session`.
pub fn address(session: &str, position: usize, set: usize) -> String {
    format!("{session}/exercise/{position}/set/{set}")
}

/// What a set reads as now: Hevy's figures with any edit over them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reads {
    pub reps: RepCount,
    pub load: StatedLoad,
    pub effort: Option<Rir>,
}

impl Reads {
    fn of(recorded: SetFigures, logged: &LoggedSet) -> Self {
        let edit = logged.edit;
        let figures = edit.map_or(recorded, |edit| edit.over(recorded));
        Self {
            reps: figures.reps(),
            load: StatedLoad::of(figures),
            effort: edit.and_then(SetEdit::effort).or(logged.effort),
        }
    }
}

/// The form as posted or as first drawn.
#[derive(Debug, Default)]
pub struct Typed {
    pub reps: String,
    pub kg: String,
    pub assistance: bool,
    pub effort: String,
    pub because: String,
}

impl Typed {
    fn reading(reads: Reads) -> Self {
        let (kg, assistance) = match reads.load {
            StatedLoad::Nothing => (String::new(), false),
            StatedLoad::Mass(mass) => (mass.to_string(), false),
            StatedLoad::Assistance(mass) => (mass.to_string(), true),
        };
        Self {
            reps: reads.reps.to_string(),
            kg,
            assistance,
            effort: reads.effort.map(Rir::as_str).unwrap_or_default().to_owned(),
            because: String::new(),
        }
    }

    fn of(form: &[(String, String)]) -> Self {
        let mut typed = Self::default();
        for (key, value) in form {
            match key.as_str() {
                "reps" => value.clone_into(&mut typed.reps),
                "kg" => value.trim().clone_into(&mut typed.kg),
                "assistance" => typed.assistance = true,
                "effort" => value.clone_into(&mut typed.effort),
                "because" => value.clone_into(&mut typed.because),
                _ => {}
            }
        }
        typed
    }
}

/// The edit a post asks for: each field that differs from what the set reads
/// as now, and why.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault, or of the whole form if nothing was
/// changed.
pub fn asked(
    reads: Reads,
    rated: bool,
    typed: &Typed,
) -> Result<(SetEdit, CorrectionReason), Refusal> {
    let reps = typed
        .reps
        .trim()
        .parse::<u32>()
        .ok()
        .and_then(|reps| RepCount::new(reps).ok())
        .ok_or_else(|| Refusal::of("reps", "a whole number above nought".to_owned()))?;
    let load = if typed.kg.is_empty() {
        if typed.assistance {
            return Err(Refusal::of("kg", "say how much assistance".to_owned()));
        }
        StatedLoad::Nothing
    } else {
        let mass = Kg::from_str(&typed.kg)
            .map_err(|_| Refusal::of("kg", "a mass in kg, as 92.5".to_owned()))?;
        if typed.assistance {
            StatedLoad::Assistance(mass)
        } else {
            StatedLoad::Mass(mass)
        }
    };
    let effort = if typed.effort.is_empty() {
        None
    } else if rated {
        Some(
            Rir::from_str(&typed.effort)
                .map_err(|_| Refusal::of("effort", "choose how many were in reserve".to_owned()))?,
        )
    } else {
        return Err(Refusal::of(
            "effort",
            "only a completed working set has an effort".to_owned(),
        ));
    };
    let because = CorrectionReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    let edit = SetEdit::new(
        (reps != reads.reps).then_some(reps),
        (load != reads.load).then_some(load),
        effort.filter(|effort| Some(*effort) != reads.effort),
    )
    .ok_or_else(|| Refusal::whole("nothing was changed".to_owned()))?;
    Ok((edit, because))
}

/// What the session calls the set: a warm-up, or its place among the working
/// sets, as its table numbers them.
fn named(logged: &[LoggedSet], index: usize) -> String {
    match logged.get(index) {
        Some(set) if set.warm_up => "a warm-up".to_owned(),
        _ => format!(
            "set {}",
            logged
                .iter()
                .take(index + 1)
                .filter(|set| !set.warm_up)
                .count()
        ),
    }
}

/// The form.
pub fn form(
    session: &str,
    exercise: Exercise,
    set: &str,
    logged: &LoggedSet,
    recorded: SetFigures,
    typed: &Typed,
    refused: Option<&Refusal>,
) -> Markup {
    let rated = !logged.warm_up;
    input::page(
        session,
        TITLE,
        &html! {
            h2 { (exercise.name()) ", " (set) }
            p.sub { "Logged in Hevy as " (recorded) }
            form.card method="post" {
                (input::field("reps", "reps", "Reps", &html! {
                    input id="reps" name="reps" inputmode="numeric" value=(typed.reps) required;
                }, refused))
                (input::field("kg", "kg", "kg", &html! {
                    input id="kg" name="kg" inputmode="decimal" value=(typed.kg);
                }, refused))
                (input::field("assistance", "assistance", "Assistance", &html! {
                    input id="assistance" name="assistance" type="checkbox" checked[typed.assistance];
                }, refused))
                @if rated {
                    (input::field("effort", "effort", "In reserve", &html! {
                        select id="effort" name="effort" {
                            @if logged.effort.is_none() && logged.edit.and_then(SetEdit::effort).is_none() {
                                option value="" selected[typed.effort.is_empty()] { "Not rated" }
                            }
                            @for position in Rir::ALL {
                                option value=(position.as_str()) selected[position.as_str() == typed.effort] {
                                    (position.as_str())
                                }
                            }
                        }
                    }, refused))
                }
                (input::field("because", "because", "Because", &html! {
                    input id="because" name="because" value=(typed.because) required;
                }, refused))
                (input::whole(refused))
                button { "Change" }
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// A set the page cannot change, and why.
fn cannot(session: &str, why: &str) -> Markup {
    input::page(
        session,
        TITLE,
        &html! {
            p { (why) }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// The set a change is asked of: the session, the exercise's place in it, and
/// the set's place in that, each from one.
struct At {
    on: Date,
    number: Number,
    position: usize,
    set: usize,
}

impl At {
    fn of(on: &str, number: &str, position: &str, set: &str) -> Option<Self> {
        Some(Self {
            on: on.parse().ok()?,
            number: number
                .parse::<usize>()
                .ok()
                .and_then(|number| Number::try_from(number).ok())?,
            position: position.parse().ok().filter(|position| *position > 0)?,
            set: set.parse().ok().filter(|set| *set > 0)?,
        })
    }

    fn session(&self) -> String {
        history::address(self.on, self.number)
    }
}

/// The set there, as Hevy logged it, or the page to show instead.
async fn found(
    store: &Store,
    zone: &OperatorZone,
    at: &At,
) -> Result<(Exercise, String, LoggedSet, SetFigures), Response> {
    let session = at.session();
    let unreadable = |error: &str| input::unreadable(&session, TITLE, error).into_response();
    let activity = configurator::history::Opened::open(store, zone)
        .activity(at.on, at.number)
        .await
        .map_err(|error| unreadable(&error.to_string()))?;
    let Some(opened::Activity::Gym { session: gym, .. }) = activity else {
        return Err(Redirect::to(&session).into_response());
    };
    let Some(canonical @ CanonicalExercise::ForReps { sets, .. }) =
        gym.exercises().nth(at.position - 1)
    else {
        return Err(Redirect::to(&session).into_response());
    };
    let Some(exercise) = canonical.exercise_key().and_then(Exercise::named) else {
        return Err(Redirect::to(&session).into_response());
    };
    let logged = corrections::sets_of(store, exercise, zone, at.on)
        .await
        .map_err(|error| unreadable(&error.to_string()))?;
    if logged.is_empty() {
        return Err(cannot(
            &session,
            "Only what was logged in Hevy can be changed so far, and Hevy has none of these sets.",
        )
        .into_response());
    }
    if logged.len() != sets.count() {
        return Err(cannot(
            &session,
            &format!(
                "Hevy logged {} sets of {} that day and the session shows {}, so which one this is cannot be told.",
                logged.len(),
                exercise.name(),
                sets.count()
            ),
        )
        .into_response());
    }
    let name = named(&logged, at.set - 1);
    let Some(set) = logged.into_iter().nth(at.set - 1) else {
        return Err(Redirect::to(&session).into_response());
    };
    let Some(recorded) = set.recorded else {
        return Err(cannot(
            &session,
            "A failed attempt has no reps to change it by, so it cannot be changed yet.",
        )
        .into_response());
    };
    Ok((exercise, name, set, recorded))
}

/// The form, filled with what the set reads as now.
pub async fn shown(
    State(runs): State<Arc<Runs>>,
    Path((on, number, position, set)): Path<(String, String, String, String)>,
) -> Response {
    let Some(at) = At::of(&on, &number, &position, &set) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    match found(store, zone, &at).await {
        Ok((exercise, name, logged, recorded)) => {
            let typed = Typed::reading(Reads::of(recorded, &logged));
            form(
                &at.session(),
                exercise,
                &name,
                &logged,
                recorded,
                &typed,
                None,
            )
            .into_response()
        }
        Err(shown) => shown,
    }
}

/// Make the change, then show what it changed; or the form again, with why
/// it was refused.
pub async fn posted(
    State(runs): State<Arc<Runs>>,
    Path((on, number, position, set)): Path<(String, String, String, String)>,
    Form(form_posted): Form<Vec<(String, String)>>,
) -> Response {
    let Some(at) = At::of(&on, &number, &position, &set) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let session = at.session();
    let (exercise, name, logged, recorded) = match found(store, zone, &at).await {
        Ok(found) => found,
        Err(shown) => return shown,
    };
    let typed = Typed::of(&form_posted);
    let refused = |refusal: &Refusal| {
        form(
            &session,
            exercise,
            &name,
            &logged,
            recorded,
            &typed,
            Some(refusal),
        )
        .into_response()
    };
    let (edit, because) = match asked(Reads::of(recorded, &logged), !logged.warm_up, &typed) {
        Ok(asked) => asked,
        Err(refusal) => return refused(&refusal),
    };
    match corrections::assert_logged_set(store, recorded, edit, &because, logged.term.clone()).await
    {
        Ok(id) => {
            // What History collects, so the session is drawn again from the
            // layers derived under the change by the time it is opened.
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&change::changed_address(at.on, at.number, id)).into_response()
        }
        Err(error) => refused(&Refusal::whole(error.to_string())),
    }
}

/// What a change of one set changed.
pub fn changed(session: &str, recorded: SetFigures, edit: SetEdit) -> Markup {
    input::page(
        session,
        "Changed",
        &html! {
            p { "The set logged in Hevy as " (recorded) " now reads " (edit) "." }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

#[cfg(test)]
mod tests {
    use domain::{
        gym::intensity::Rir,
        measure::{Kg, RepCount},
        normalised::StatedLoad,
    };

    use super::{Reads, Typed, asked};

    fn single() -> Reads {
        Reads {
            reps: RepCount::new(1).expect("one"),
            load: StatedLoad::Mass(Kg::from_grams(90_000)),
            effort: None,
        }
    }

    fn typed(reps: &str, kg: &str, effort: &str) -> Typed {
        Typed {
            reps: reps.to_owned(),
            kg: kg.to_owned(),
            assistance: false,
            effort: effort.to_owned(),
            because: "forgot to rate it".to_owned(),
        }
    }

    /// **Only what was changed is asserted**: rating the 90 kg single leaves
    /// its reps and load out of the edit.
    #[test]
    fn rating_a_set_asserts_only_its_effort() {
        let (edit, _) = asked(single(), true, &typed("1", "90", "1")).expect("an edit");

        assert_eq!(edit.reps(), None);
        assert_eq!(edit.load(), None);
        assert_eq!(edit.effort(), Some(Rir::One));
    }

    #[test]
    fn a_form_posted_as_it_reads_changes_nothing_and_is_refused() {
        let refused = asked(single(), true, &typed("1", "90", "")).map_err(|refusal| refusal.field);

        assert_eq!(refused, Err(None));
    }

    /// A warm-up has no effort, so one posted for it is refused rather than
    /// stored (#817's ruling on which sets are rated).
    #[test]
    fn a_warm_up_is_given_no_effort() {
        let refused =
            asked(single(), false, &typed("1", "90", "1")).map_err(|refusal| refusal.field);

        assert_eq!(refused, Err(Some("effort")));
    }

    /// **The set is named as the session's table numbers it**: warm-ups are
    /// not counted, so 25 Sep's 92.5 kg single, sixth logged, is set 2.
    #[test]
    fn a_set_is_named_as_the_session_numbers_it() {
        use configurator::corrections::LoggedSet;
        use domain::{
            landing::SourceRecordId,
            normalised::{CorrectedTerm, SourceTerm},
        };

        let logged = |warm_up: bool| LoggedSet {
            term: CorrectedTerm::new(
                SourceRecordId::try_from("a-workout").expect("a record"),
                SourceTerm::try_from("FS/0").expect("a term"),
            ),
            recorded: None,
            warm_up,
            effort: None,
            edit: None,
        };
        let sets = [true, true, true, true, false, false].map(logged);

        assert_eq!(super::named(&sets, 5), "set 2");
        assert_eq!(super::named(&sets, 0), "a warm-up");
    }

    #[test]
    fn a_changed_load_is_asserted_as_typed() {
        let (edit, _) = asked(single(), true, &typed("1", "92.5", "")).expect("an edit");

        assert_eq!(edit.load(), Some(StatedLoad::Mass(Kg::from_grams(92_500))));
        assert_eq!(edit.reps(), None);
    }
}
