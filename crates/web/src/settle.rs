//! The edits a rebuild left standing on something they did not stand on when
//! they were made, listed at the top of History and settled there (§ II.2,
//! #905).
//!
//! **There only when it has entries** (operator, 2026-11-23): from now on gym
//! data arrives through Hevy, so a source adding a set to an edited exercise
//! is rare.

use std::{collections::HashMap, sync::Arc};

use axum::{
    Form,
    extract::State,
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::edits::{self, ToSettle};
use domain::gym::identity::{CanonicalExerciseId, CanonicalSetId};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    history, input,
    page::Place,
    run::{Asked, Runs},
};

/// Where an edit is settled.
pub const ROUTE: &str = "/history/settle";

/// The list, or nothing where there is nothing to settle.
pub fn list(settle: &[ToSettle], today: Date) -> Markup {
    html! {
        @if !settle.is_empty() {
            section.settle {
                h2.group { "Edits to settle" }
                ul {
                    @for entry in settle { li { (entry_of(entry, today)) } }
                }
            }
        }
    }
}

fn entry_of(entry: &ToSettle, today: Date) -> Markup {
    match entry {
        ToSettle::Added { set, edit, at } => html! {
            p {
                @match at {
                    Some(at) => {
                        a href=(history::address(at.on, at.number)) {
                            (history::day_name(at.on, today))
                        }
                        ": set " (at.set) " of " (edit.becomes.name())
                    }
                    None => { "A set of " (edit.becomes.name()) }
                }
                " was added after you changed " (edit.read.name()) " to "
                (edit.becomes.name()) "."
            }
            form method="post" action=(ROUTE) {
                input type="hidden" name="exercise" value=(edit.exercise.as_i64());
                input type="hidden" name="set" value=(set.as_i64());
                button name="settled" value="keep" { "Keep" }
                " "
                button name="settled" value="remove" { "Remove the edit" }
            }
        },
        ToSettle::ExerciseGone(edit) => html! {
            p {
                "You changed " (edit.read.name()) " to " (edit.becomes.name())
                " on an exercise no session holds any more."
            }
            form method="post" action=(ROUTE) {
                input type="hidden" name="exercise" value=(edit.exercise.as_i64());
                button name="settled" value="remove" { "Remove the edit" }
            }
        },
        ToSettle::SetGone { set, edit } => html! {
            p { "You changed a set to " (edit) ", and no session holds that set any more." }
            form method="post" action=(ROUTE) {
                input type="hidden" name="set" value=(set.as_i64());
                button name="settled" value="remove" { "Remove the edit" }
            }
        },
    }
}

/// How one entry was settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settled {
    /// The added set takes the exercise's edit.
    Keep(CanonicalExerciseId, CanonicalSetId),
    /// The exercise's edit goes.
    RemoveExercise(CanonicalExerciseId),
    /// The set's edit goes.
    RemoveSet(CanonicalSetId),
}

impl Settled {
    fn of(form: &HashMap<String, String>) -> Option<Self> {
        let exercise = form
            .get("exercise")
            .and_then(|id| id.parse::<i64>().ok())
            .and_then(|id| CanonicalExerciseId::try_from(id).ok());
        let set = form
            .get("set")
            .and_then(|id| id.parse::<i64>().ok())
            .and_then(|id| CanonicalSetId::try_from(id).ok());
        match (form.get("settled").map(String::as_str), exercise, set) {
            (Some("keep"), Some(exercise), Some(set)) => Some(Self::Keep(exercise, set)),
            (Some("remove"), Some(exercise), _) => Some(Self::RemoveExercise(exercise)),
            (Some("remove"), None, Some(set)) => Some(Self::RemoveSet(set)),
            _ => None,
        }
    }
}

/// Settle one entry, then go back to History.
pub async fn posted(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let history = Place::History.path();
    let Some(settled) = Settled::of(&form) else {
        return Redirect::to(history).into_response();
    };
    let store = &runs.setting().store;
    let written = match settled {
        Settled::Keep(exercise, set) => edits::keep(store, exercise, set).await,
        Settled::RemoveExercise(exercise) => edits::unedit_exercise(store, exercise).await,
        Settled::RemoveSet(set) => edits::unedit_set(store, set).await,
    };
    match written {
        Ok(()) => {
            // A removed edit is undone by rebuilding the days it reached;
            // a kept set already reads as the edit says.
            if !matches!(settled, Settled::Keep(..)) {
                runs.start(&Asked::Only(configurator::history::collects()));
            }
            Redirect::to(history).into_response()
        }
        Err(error) => {
            input::unreadable(history, "Edits to settle", &error.to_string()).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use domain::gym::identity::{CanonicalExerciseId, CanonicalSetId};

    use super::Settled;

    fn form(fields: &[(&str, &str)]) -> HashMap<String, String> {
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// **Each button says what it settles**, and a form naming nothing it
    /// can settle settles nothing.
    #[test]
    fn a_form_names_how_it_was_settled() {
        let exercise = CanonicalExerciseId::try_from(7).expect("an id");
        let set = CanonicalSetId::try_from(31).expect("an id");
        assert_eq!(
            Settled::of(&form(&[
                ("settled", "keep"),
                ("exercise", "7"),
                ("set", "31")
            ])),
            Some(Settled::Keep(exercise, set))
        );
        assert_eq!(
            Settled::of(&form(&[
                ("settled", "remove"),
                ("exercise", "7"),
                ("set", "31")
            ])),
            Some(Settled::RemoveExercise(exercise))
        );
        assert_eq!(
            Settled::of(&form(&[("settled", "remove"), ("set", "31")])),
            Some(Settled::RemoveSet(set))
        );
        assert_eq!(
            Settled::of(&form(&[("settled", "keep"), ("set", "31")])),
            None
        );
        assert_eq!(Settled::of(&form(&[("exercise", "7")])), None);
    }
}
