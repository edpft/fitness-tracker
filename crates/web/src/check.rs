//! What in the gym record may need correcting, by kind, each opening the
//! exercise's pen, and each said right from here (#896).

use std::{collections::HashMap, sync::Arc};

use axum::{
    Form,
    extract::State,
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::checks::{self, ToCheck};
use domain::gym::{
    check::{Doubt, Dumbbells, Kind},
    identity::CanonicalExerciseId,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{history, input, page, run::Runs};

/// Where the list is.
pub const HERE: &str = "/history/check";

/// Where one is said to be right.
pub const RIGHT: &str = "/history/check/right";

const TITLE: &str = "To check";

/// The list, or the reason it could not be read.
fn page(read: &Result<Vec<ToCheck>, String>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(TITLE, &[])) }
            body hx-boost="true" {
                main.history.check {
                    h1 { (TITLE) }
                    @match read {
                        Ok(found) if found.is_empty() => p.sub { "Nothing to check." },
                        Ok(found) => {
                            @for (kind, found) in checks::by_kind(found) {
                                h2.group { (heading(kind)) " · " (found.len()) }
                                ul.sessions {
                                    @for check in found { li { (entry(check)) } }
                                }
                            }
                        }
                        Err(error) => p.failed { "What to check could not be read: " (error) },
                    }
                    (page::bar(HERE))
                }
            }
        }
    }
}

const fn heading(kind: Kind) -> &'static str {
    match kind {
        Kind::DumbbellLoad => "Barbell lifts on dumbbell steps",
        Kind::HalvedOrDoubled => "Loads that halved or doubled",
        Kind::Switched => "Logged after you switched",
    }
}

fn entry(check: &ToCheck) -> Markup {
    let pen = format!(
        "{}/exercise/{}",
        history::address(check.on, check.number),
        check.position
    );
    html! {
        a href=(pen) {
            span.name { (check.lift.name()) }
            span.facts { (day(check.on)) " · " (why(&check.doubt)) }
        }
        form method="post" action=(RIGHT) {
            input type="hidden" name="exercise" value=(check.exercise.as_i64());
            input type="hidden" name="kind" value=(check.doubt.kind().as_str());
            button { "This is right" }
        }
    }
}

fn day(on: Date) -> String {
    on.strftime("%a %-d %b %Y").to_string()
}

/// What makes it doubtful, in a line.
fn why(doubt: &Doubt) -> String {
    match doubt {
        Doubt::DumbbellLoad(loads) => loads
            .iter()
            .map(|load| match load {
                Dumbbells::Pair { total, each } => format!("{total} kg: 2 × {each} kg dumbbells?"),
                Dumbbells::One(load) => format!("{load} kg: one dumbbell?"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        Doubt::HalvedOrDoubled { before, on, now } => {
            format!("{now} kg, after {before} kg on {}", day(*on))
        }
        Doubt::Switched(switch) => {
            let to = switch
                .to
                .iter()
                .map(|exercise| exercise.name())
                .collect::<Vec<_>>()
                .join(" or ");
            format!("{to} from {}?", switch.from.strftime("%B %Y"))
        }
    }
}

/// The list, once a run has collected what it reads.
#[derive(Debug)]
pub struct Page;

impl crate::drawn::Drawn for Page {
    const ROUTE: &'static str = HERE;
    type Read = Vec<ToCheck>;

    fn reads() -> application::graph::Reads {
        checks::reads()
    }

    fn asked(
        _path: &axum::extract::RawPathParams,
        _query: &HashMap<String, String>,
    ) -> Result<Self, Redirect> {
        Ok(Self)
    }

    fn here(&self) -> String {
        HERE.to_owned()
    }

    async fn read(
        &self,
        store: &configurator::store::Store,
        zone: &domain::normalised::OperatorZone,
    ) -> Result<Self::Read, crate::drawn::Unreadable> {
        Ok(checks::Checking::open(store, zone).to_check().await?)
    }

    fn render(&self, read: Result<Self::Read, crate::drawn::Unreadable>) -> Markup {
        page(&read.map_err(|error| error.0))
    }
}

/// What a form says is right.
fn said(form: &HashMap<String, String>) -> Option<(CanonicalExerciseId, Kind)> {
    let exercise = form
        .get("exercise")
        .and_then(|id| id.parse::<i64>().ok())
        .and_then(|id| CanonicalExerciseId::try_from(id).ok())?;
    let kind = form
        .get("kind")
        .and_then(|kind| kind.parse::<Kind>().ok())?;
    Some((exercise, kind))
}

/// Say one is right, then go back to the list.
pub async fn posted(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let Some((exercise, kind)) = said(&form) else {
        return Redirect::to(HERE).into_response();
    };
    match checks::say_right(&runs.setting().store, exercise, kind).await {
        Ok(()) => Redirect::to(HERE).into_response(),
        Err(error) => input::unreadable(HERE, TITLE, &error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use domain::gym::{check::Kind, identity::CanonicalExerciseId};

    use super::said;

    fn form(fields: &[(&str, &str)]) -> HashMap<String, String> {
        fields
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// **A form names an exercise and a kind**, and one naming either
    /// wrongly says nothing.
    #[test]
    fn a_form_names_what_is_right() {
        assert_eq!(
            said(&form(&[("exercise", "7"), ("kind", "switched")])),
            Some((
                CanonicalExerciseId::try_from(7).expect("an id"),
                Kind::Switched
            ))
        );
        assert_eq!(said(&form(&[("exercise", "7"), ("kind", "nope")])), None);
        assert_eq!(said(&form(&[("kind", "switched")])), None);
    }
}
