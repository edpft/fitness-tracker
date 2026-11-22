//! Changing one set of an opened session: its reps, its load, whether the
//! load was assistance, and its effort (#892, #893).
//!
//! **One set, never the similar ones.** The operator, 2026-11-21: *"There's
//! nothing necessarily similar about two different sets of 10 x 70kg. The
//! idea behind similar sets was to be able to change exercises."*
//!
//! **Whatever source recorded it.** The edit is anchored to the canonical set
//! (§ II.2), so a set a sheet or a watch recorded takes a reps in reserve it
//! never had, exactly as one logged in Hevy does.
//!
//! **Only what was changed is asserted.** A field posted as it already reads
//! is left out of the edit, so what the sources said, or an earlier edit,
//! still stands for it.

use std::{str::FromStr as _, sync::Arc};

use application::history::Number;
use axum::{
    Form,
    extract::{Path, State},
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{edits, store::Store};
use domain::{
    canonical::Attributed,
    gym::{
        CanonicalExercise, CanonicalSet, Identified, Load, Performed, SetKind,
        edit::{EditReason, SetEdit, StatedLoad},
        identity::CanonicalSetId,
        intensity::Rir,
    },
    measure::{Kg, RepCount},
    normalised::OperatorZone,
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

/// What a set reads as now.
///
/// **Each may be unstated**: a sheet that wrote "Burpee" and not how many, a
/// set no source put a load on, a set nobody rated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reads {
    pub reps: Option<RepCount>,
    pub load: Option<Load>,
    pub effort: Option<Rir>,
    /// Whether it can be given an effort: not a warm-up.
    pub rated: bool,
}

impl Reads {
    fn of(set: &CanonicalSet<RepCount>) -> Self {
        Self {
            reps: set.outcome.value().completed().copied().flatten(),
            load: set.load.as_ref().map(Attributed::copied),
            effort: set.intensity.as_ref().map(Attributed::copied),
            rated: set.kind.as_ref().map(Attributed::copied) != Some(SetKind::Warmup),
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
        let (kg, assistance) = match reads.load.map(StatedLoad::of) {
            None | Some(StatedLoad::Nothing) => (String::new(), false),
            Some(StatedLoad::Mass(mass)) => (mass.to_string(), false),
            Some(StatedLoad::Assistance(mass)) => (mass.to_string(), true),
        };
        Self {
            reps: reads.reps.map(|reps| reps.to_string()).unwrap_or_default(),
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
                "reps" => value.trim().clone_into(&mut typed.reps),
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
/// **A field left blank where nothing was stated changes nothing**: an empty
/// kg on a set no source put a load on is not a claim that there was none.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault, or of the whole form if nothing was
/// changed.
pub fn asked(reads: Reads, typed: &Typed) -> Result<(SetEdit, EditReason), Refusal> {
    let reps = if typed.reps.is_empty() && reads.reps.is_none() {
        None
    } else {
        Some(
            typed
                .reps
                .parse::<u32>()
                .ok()
                .and_then(|reps| RepCount::new(reps).ok())
                .ok_or_else(|| Refusal::of("reps", "a whole number above nought".to_owned()))?,
        )
    };
    let stated = if typed.kg.is_empty() {
        if typed.assistance {
            return Err(Refusal::of("kg", "say how much assistance".to_owned()));
        }
        reads.load.map(|_| StatedLoad::Nothing)
    } else {
        let mass = Kg::from_str(&typed.kg)
            .map_err(|_| Refusal::of("kg", "a mass in kg, as 92.5".to_owned()))?;
        Some(if typed.assistance {
            StatedLoad::Assistance(mass)
        } else {
            StatedLoad::Mass(mass)
        })
    };
    let load = stated.map(|stated| stated.on(reads.load));
    let effort = if typed.effort.is_empty() {
        None
    } else if reads.rated {
        Some(
            Rir::from_str(&typed.effort)
                .map_err(|_| Refusal::of("effort", "choose how many were in reserve".to_owned()))?,
        )
    } else {
        return Err(Refusal::of(
            "effort",
            "only a working set has an effort".to_owned(),
        ));
    };
    let because = EditReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    let edit = SetEdit::new(
        reps.filter(|reps| Some(*reps) != reads.reps),
        load.filter(|load| Some(*load) != reads.load),
        effort.filter(|effort| Some(*effort) != reads.effort),
    )
    .ok_or_else(|| Refusal::whole("nothing was changed".to_owned()))?;
    Ok((edit, because))
}

/// What the session calls the set: a warm-up, or its place among the working
/// sets, as its table numbers them.
fn named(warm_ups: &[bool], index: usize) -> String {
    match warm_ups.get(index) {
        Some(true) => "a warm-up".to_owned(),
        _ => format!(
            "set {}",
            warm_ups
                .iter()
                .take(index + 1)
                .filter(|warm_up| !**warm_up)
                .count()
        ),
    }
}

/// The form.
pub fn form(session: &str, found: &Found, typed: &Typed, refused: Option<&Refusal>) -> Markup {
    input::page(
        session,
        TITLE,
        &html! {
            h2 { (found.exercise) ", " (found.name) }
            form.card method="post" {
                (input::field("reps", "reps", "Reps", &html! {
                    input id="reps" name="reps" inputmode="numeric" value=(typed.reps);
                }, refused))
                (input::field("kg", "kg", "kg", &html! {
                    input id="kg" name="kg" inputmode="decimal" value=(typed.kg);
                }, refused))
                (input::field("assistance", "assistance", "Assistance", &html! {
                    input id="assistance" name="assistance" type="checkbox" checked[typed.assistance];
                }, refused))
                @if found.reads.rated {
                    (input::field("effort", "effort", "In reserve", &html! {
                        select id="effort" name="effort" {
                            @if found.reads.effort.is_none() {
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

/// The set a change is asked of, as the layer holds it.
pub struct Found {
    exercise: String,
    name: String,
    id: CanonicalSetId,
    reads: Reads,
}

/// The set there, or the page to show instead.
async fn found(store: &Store, zone: &OperatorZone, at: &At) -> Result<Found, Response> {
    let session = at.session();
    let unreadable = |error: &str| input::unreadable(&session, TITLE, error).into_response();
    let canonical = match change::located(store, zone, at.on, at.number, at.position).await {
        Ok(Some(canonical @ CanonicalExercise::ForReps { .. })) => canonical,
        Ok(_) => return Err(Redirect::to(&session).into_response()),
        Err(error) => return Err(unreadable(&error)),
    };
    let CanonicalExercise::ForReps { identified, sets } = &canonical else {
        return Err(Redirect::to(&session).into_response());
    };
    let Some(set) = sets.iter().nth(at.set - 1) else {
        return Err(Redirect::to(&session).into_response());
    };
    // A failed attempt has no reps to change, and is not offered a pen.
    if matches!(set.outcome.value(), Performed::Failed) {
        return Err(Redirect::to(&session).into_response());
    }
    let ids = edits::ids_of(store, &canonical)
        .await
        .map_err(|error| unreadable(&error.to_string()))?
        .ok_or_else(|| unreadable("the session is being rebuilt; open it again in a moment"))?;
    let Some(id) = ids.sets.get(at.set - 1).copied() else {
        return Err(Redirect::to(&session).into_response());
    };
    let warm_ups: Vec<bool> = sets
        .iter()
        .map(|set| set.kind.as_ref().map(Attributed::copied) == Some(SetKind::Warmup))
        .collect();
    Ok(Found {
        exercise: match identified.value() {
            Identified::Recorded(exercise) => exercise.name().to_owned(),
            Identified::Guessed(guess) => guess.exercise().map_or_else(
                || guess.to_string(),
                |exercise| format!("{}?", exercise.name()),
            ),
        },
        name: named(&warm_ups, at.set - 1),
        id,
        reads: Reads::of(set),
    })
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
        Ok(found) => {
            form(&at.session(), &found, &Typed::reading(found.reads), None).into_response()
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
    let found = match found(store, zone, &at).await {
        Ok(found) => found,
        Err(shown) => return shown,
    };
    let typed = Typed::of(&form_posted);
    let refused = |refusal: &Refusal| form(&session, &found, &typed, Some(refusal)).into_response();
    let (edit, because) = match asked(found.reads, &typed) {
        Ok(asked) => asked,
        Err(refusal) => return refused(&refusal),
    };
    match edits::assert_set(store, &because, found.id, edit).await {
        Ok(id) => {
            // What History collects, so the session is drawn again from the
            // layers derived under the change by the time it is opened.
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&change::changed_address(at.on, at.number, id)).into_response()
        }
        Err(error) => refused(&Refusal::whole(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        gym::{Load, SignedKg, intensity::Rir},
        measure::{Kg, RepCount},
    };

    use super::{Reads, Typed, asked};

    /// 25 Sep's 90 kg front squat single, unrated.
    fn single() -> Reads {
        Reads {
            reps: Some(RepCount::new(1).expect("one")),
            load: Some(Load::absolute(Kg::from_grams(90_000))),
            effort: None,
            rated: true,
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
        let (edit, _) = asked(single(), &typed("1", "90", "1")).expect("an edit");

        assert_eq!(edit.reps(), None);
        assert_eq!(edit.load(), None);
        assert_eq!(edit.effort(), Some(Rir::One));
    }

    #[test]
    fn a_form_posted_as_it_reads_changes_nothing_and_is_refused() {
        let refused = asked(single(), &typed("1", "90", "")).map_err(|refusal| refusal.field);

        assert_eq!(refused, Err(None));
    }

    /// A warm-up has no effort, so one posted for it is refused rather than
    /// stored (#817's ruling on which sets are rated).
    #[test]
    fn a_warm_up_is_given_no_effort() {
        let warm_up = Reads {
            rated: false,
            ..single()
        };
        let refused = asked(warm_up, &typed("1", "90", "1")).map_err(|refusal| refusal.field);

        assert_eq!(refused, Err(Some("effort")));
    }

    /// **The set is named as the session's table numbers it**: warm-ups are
    /// not counted, so 25 Sep's 92.5 kg single, sixth logged, is set 2.
    #[test]
    fn a_set_is_named_as_the_session_numbers_it() {
        let warm_ups = [true, true, true, true, false, false];

        assert_eq!(super::named(&warm_ups, 5), "set 2");
        assert_eq!(super::named(&warm_ups, 0), "a warm-up");
    }

    #[test]
    fn a_changed_load_is_asserted_on_the_set_s_axis() {
        let (edit, _) = asked(single(), &typed("1", "92.5", "")).expect("an edit");
        assert_eq!(edit.load(), Some(Load::absolute(Kg::from_grams(92_500))));
        assert_eq!(edit.reps(), None);

        let pull_up = Reads {
            load: Some(Load::BODYWEIGHT),
            ..single()
        };
        let (edit, _) = asked(pull_up, &typed("1", "10", "")).expect("an edit");
        assert_eq!(
            edit.load(),
            Some(Load::relative(SignedKg::from_grams(10_000)))
        );
    }

    /// **A sheet's set with no load and no effort takes an effort alone**: a
    /// kg left blank where nothing was stated is not a claim of no load.
    #[test]
    fn a_blank_field_where_nothing_was_stated_changes_nothing() {
        let sheet = Reads {
            load: None,
            ..single()
        };
        let (edit, _) = asked(sheet, &typed("1", "", "2")).expect("an edit");

        assert_eq!(edit.load(), None);
        assert_eq!(edit.effort(), Some(Rir::Two));
    }
}
