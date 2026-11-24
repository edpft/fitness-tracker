//! Changing what an exercise in an opened session was (#754, #893).
//!
//! The operator's reference is his budgeting app (2026-11-10): *"it allows me
//! to edit a transaction and then asks me if I want this edit to apply to more
//! than one transaction."* So the change is two steps: what it was and why,
//! then whether it goes for this session alone or for the similar ones too,
//! listed by day with the button saying how many sets it changes. Then what
//! was changed, and a way to undo it.
//!
//! **Whatever source the session was drawn from.** An edit is anchored to the
//! canonical exercise (§ II.2), so a session only the watch and a sheet
//! recorded is changed exactly as one logged in Hevy is.
//!
//! **Similar is every other exercise that reads as the same one**, which is
//! what a stand-in is: the source had no entry for what he did, so he logged
//! it under the nearest one, every time.
//!
//! **What arrives later is not changed** (§ II.2, *"overrides do not
//! propagate"*): the exercises are the ones listed when he changes them.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use application::history::{Number, opened};
use axum::{
    Form,
    extract::{Path, Query, State},
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{
    edits::{self, HeldExercise},
    store::Store,
};
use domain::{
    gym::{
        CanonicalExercise,
        edit::{EditId, EditReason, Found, SparseExercise, SparseGymSession},
        exercise::{DistanceExercise, DurationExercise, Exercise, RepsExercise},
        identity::CanonicalExerciseId,
    },
    normalised::OperatorZone,
};
use jiff::civil::Date;
use maud::{Markup, PreEscaped, html};

use crate::{
    history,
    input::{self, Refusal},
    page::Place,
    run::{Asked, Runs},
};

/// Where an exercise of an opened session is changed.
pub const ROUTE: &str = "/history/{on}/{number}/exercise/{position}";

/// Where what a change changed is shown, and undone.
pub const CHANGED: &str = "/history/{on}/{number}/changed/{edit}";

const TITLE: &str = "Change exercise";

/// Recounts the sets the button changes as exercises are ticked.
const SCRIPT: &str = "
function recount(form) {
  const chosen = [...form.querySelectorAll('input[name=exercise]')]
    .filter((exercise) => exercise.type === 'hidden' || exercise.checked);
  const sets = chosen.reduce((total, exercise) => total + Number(exercise.dataset.sets), 0);
  const days = new Set(chosen.map((exercise) => exercise.dataset.day)).size;
  form.querySelector('button.all').textContent =
    'Change ' + counted(sets, 'set') + ' over ' + counted(days, 'session');
}
function counted(count, what) { return count === 1 ? '1 ' + what : count + ' ' + what + 's'; }
";

/// Where what edit `id` changed is shown, from the session it was made on.
pub fn changed_address(on: Date, number: Number, id: EditId) -> String {
    format!("{}/changed/{id}", history::address(on, number))
}

/// Every exercise an exercise could have been instead: those of its own kind,
/// since a set for reps has no time to carry and a hold has no count.
fn instead(exercise: Exercise) -> Vec<Exercise> {
    let all: Vec<Exercise> = match exercise {
        Exercise::Reps(_) => RepsExercise::ALL
            .iter()
            .copied()
            .map(Exercise::Reps)
            .collect(),
        Exercise::Duration(_) => DurationExercise::ALL
            .iter()
            .copied()
            .map(Exercise::Duration)
            .collect(),
        Exercise::Distance(_) => DistanceExercise::ALL
            .iter()
            .copied()
            .map(Exercise::Distance)
            .collect(),
    };
    let mut all: Vec<Exercise> = all.into_iter().filter(|other| *other != exercise).collect();
    all.sort_by_key(|other| other.name());
    all
}

/// The first step: what it was, and why.
pub fn choose(
    session: &str,
    exercise: Exercise,
    this: &HeldExercise,
    typed: &Typed,
    refused: Option<&Refusal>,
) -> Markup {
    let to = typed.to.map(Exercise::as_str).unwrap_or_default();
    let options = instead(exercise);
    input::page(
        session,
        TITLE,
        &html! {
            h2 { (exercise.name()) }
            p.sub { (counted(this.sets.len(), "set")) }
            form.card method="get" {
                (input::field("to", "to", "It was", &html! {
                    select id="to" name="to" {
                        option value="" selected[to.is_empty()] { "Choose an exercise" }
                        @for option in &options {
                            option value=(option.as_str()) selected[option.as_str() == to] { (option.name()) }
                        }
                    }
                }, refused))
                (input::field("because", "because", "Because", &html! {
                    input id="because" name="because" value=(typed.because) required;
                }, refused))
                (input::whole(refused))
                button { "Next" }
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// The second step: this session alone, or the similar ones too.
pub fn scope(session: &str, change: &Change<'_>, refused: Option<&Refusal>) -> Markup {
    let similar = by_day(change.similar);
    let sets: usize = std::iter::once(change.this)
        .chain(change.similar)
        .map(|held| held.sets.len())
        .sum();
    let days = days(std::iter::once(change.this).chain(change.similar));
    input::page(
        session,
        TITLE,
        &html! {
            script { (PreEscaped(SCRIPT)) }
            p.sub { (change.exercise.name()) " to " (change.to.name()) }
            form.card method="post" onchange="recount(this)" {
                input type="hidden" name="to" value=(change.to.as_str());
                input type="hidden" name="because" value=(change.because);
                h2 { "This session" }
                p { (counted(change.this.sets.len(), "set")) }
                input type="hidden" name="exercise" value=(change.this.exercise)
                    data-sets=(change.this.sets.len()) data-day=(change.this.day);
                button name="only" value="this" { "Change this session only" }
                @if !similar.is_empty() {
                    h2 { "Similar" }
                    p.sub { "Every other session where " (change.exercise.name()) " was performed." }
                    @for (day, exercises) in &similar {
                        fieldset.field {
                            legend { (day.0.strftime("%a %-d %b %Y")) }
                            @for held in exercises {
                                label {
                                    input type="checkbox" name="exercise" value=(held.exercise)
                                        data-sets=(held.sets.len()) data-day=(held.day) checked;
                                    " " (counted(held.sets.len(), "set"))
                                }
                            }
                        }
                    }
                    (input::whole(refused))
                    button.all { "Change " (counted(sets, "set")) " over " (counted(days, "session")) }
                } @else {
                    (input::whole(refused))
                }
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// What an edit changed, and the button that undoes it.
fn changed(session: &str, undo: &str, said: &Markup) -> Markup {
    input::page(
        session,
        "Changed",
        &html! {
            (said)
            form method="post" action=(undo) {
                button { "Undo this change" }
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// What an exercise edit changed: how many sets, over which sessions.
fn exercises_changed(exercise: Exercise, reached: &[HeldExercise]) -> Markup {
    let sets: usize = reached.iter().map(|held| held.sets.len()).sum();
    let by_day = by_day(reached);
    html! {
        p {
            (counted(sets, "set")) " over " (counted(by_day.len(), "session"))
            " now read as " (exercise.name()) "."
        }
        ul.sessions {
            @for (day, exercises) in &by_day {
                li {
                    (day.0.strftime("%a %-d %b %Y")) ": "
                    (counted(exercises.iter().map(|held| held.sets.len()).sum(), "set"))
                }
            }
        }
    }
}

/// What the first step was given, as typed.
#[derive(Debug, Default)]
pub struct Typed {
    pub to: Option<Exercise>,
    pub because: String,
}

impl Typed {
    /// From the query or the form: `to`, a vocabulary key, and `because`.
    pub fn of<'a>(fields: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut typed = Self::default();
        for (key, value) in fields {
            match key {
                "to" => typed.to = Exercise::named(value),
                "because" => value.clone_into(&mut typed.because),
                _ => {}
            }
        }
        typed
    }
}

/// A change ready for its second step.
pub struct Change<'a> {
    pub exercise: Exercise,
    pub to: Exercise,
    pub because: &'a EditReason,
    pub this: &'a HeldExercise,
    pub similar: &'a [HeldExercise],
}

/// What the first step asks: the exercise it was, and why.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault.
pub fn asked(exercise: Exercise, typed: &Typed) -> Result<(Exercise, EditReason), Refusal> {
    let to = typed
        .to
        .ok_or_else(|| Refusal::of("to", "choose the exercise it was".to_owned()))?;
    if to == exercise {
        return Err(Refusal::of(
            "to",
            format!("it already reads as {}", exercise.name()),
        ));
    }
    if !instead(exercise).contains(&to) {
        return Err(Refusal::of(
            "to",
            format!("{} is not measured as {} is", to.name(), exercise.name()),
        ));
    }
    let because = EditReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    Ok((to, because))
}

/// The exercises a post chose, each one that was offered.
///
/// **Only what was offered**: an exercise posted that is not among those
/// found now is a page drawn before the record changed, and is refused rather
/// than asserted over.
///
/// # Errors
///
/// [`Refusal`] if none was chosen or one was not offered.
pub fn chosen<'a>(
    form: &[(String, String)],
    this: &'a HeldExercise,
    similar: &'a [HeldExercise],
) -> Result<Vec<&'a HeldExercise>, Refusal> {
    let only_this = form
        .iter()
        .any(|(key, value)| key == "only" && value == "this");
    let mut chosen: Vec<&HeldExercise> = Vec::new();
    for (key, value) in form {
        if key != "exercise" {
            continue;
        }
        let id = value
            .parse::<i64>()
            .ok()
            .and_then(|id| CanonicalExerciseId::try_from(id).ok())
            .ok_or_else(stale)?;
        let offered = std::iter::once(this)
            .chain(similar)
            .find(|held| held.exercise == id)
            .ok_or_else(stale)?;
        if only_this && offered.exercise != this.exercise {
            continue;
        }
        if !chosen.iter().any(|held| held.exercise == id) {
            chosen.push(offered);
        }
    }
    if chosen.is_empty() {
        return Err(Refusal::whole("choose at least one session".to_owned()));
    }
    Ok(chosen)
}

fn stale() -> Refusal {
    Refusal::whole("the record has changed since this page was drawn; open it again".to_owned())
}

fn by_day(exercises: &[HeldExercise]) -> BTreeMap<std::cmp::Reverse<Date>, Vec<&HeldExercise>> {
    let mut days: BTreeMap<std::cmp::Reverse<Date>, Vec<&HeldExercise>> = BTreeMap::new();
    for held in exercises {
        days.entry(std::cmp::Reverse(held.day))
            .or_default()
            .push(held);
    }
    days
}

fn days<'a>(exercises: impl Iterator<Item = &'a HeldExercise>) -> usize {
    exercises
        .map(|held| held.day)
        .collect::<BTreeSet<_>>()
        .len()
}

fn counted(count: usize, what: &str) -> String {
    if count == 1 {
        format!("1 {what}")
    } else {
        format!("{count} {what}s")
    }
}

/// The exercise a change is asked of: the session it is in, and where.
struct At {
    on: Date,
    number: Number,
    position: usize,
}

impl At {
    fn of(on: &str, number: &str, position: &str) -> Option<Self> {
        Some(Self {
            on: on.parse().ok()?,
            number: number
                .parse::<usize>()
                .ok()
                .and_then(|number| Number::try_from(number).ok())?,
            position: position.parse().ok().filter(|position| *position > 0)?,
        })
    }

    fn session(&self) -> String {
        history::address(self.on, self.number)
    }
}

/// The exercise there, as the opened session reads it.
pub async fn located(
    store: &Store,
    zone: &OperatorZone,
    on: Date,
    number: Number,
    position: usize,
) -> Result<Option<CanonicalExercise>, String> {
    let activity = configurator::history::Opened::open(store, zone)
        .activity(on, number)
        .await
        .map_err(|error| error.to_string())?;
    let Some(opened::Activity::Gym { session, .. }) = activity else {
        return Ok(None);
    };
    Ok(session.exercises().nth(position - 1).cloned())
}

/// What a change is asked of, read from the store: the exercise and this
/// session's own, or the page to show instead.
async fn found(
    store: &Store,
    zone: &OperatorZone,
    at: &At,
) -> Result<(Exercise, HeldExercise), Response> {
    let session = at.session();
    let unreadable = |error: &str| input::unreadable(&session, TITLE, error).into_response();
    let canonical = match located(store, zone, at.on, at.number, at.position).await {
        Ok(Some(canonical)) => canonical,
        Ok(None) => return Err(Redirect::to(&session).into_response()),
        Err(error) => return Err(unreadable(&error)),
    };
    let Some(exercise) = canonical.exercise_key().and_then(Exercise::named) else {
        return Err(Redirect::to(&session).into_response());
    };
    let (held, ids) = edits::ids_of(store, &canonical)
        .await
        .map_err(|error| unreadable(&error.to_string()))?
        .ok_or_else(|| unreadable("the session is being rebuilt; open it again in a moment"))?;
    Ok((
        exercise,
        HeldExercise {
            session: held,
            exercise: ids.exercise,
            day: at.on,
            sets: ids.sets,
        },
    ))
}

/// Every other exercise that reads as `exercise`.
async fn similar(
    store: &Store,
    exercise: Exercise,
    this: &HeldExercise,
) -> Result<Vec<HeldExercise>, String> {
    Ok(edits::reading_as(store, exercise)
        .await
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|held| held.exercise != this.exercise)
        .collect())
}

/// The change, at the step its query has reached.
pub async fn shown(
    State(runs): State<Arc<Runs>>,
    Path((on, number, position)): Path<(String, String, String)>,
    Query(query): Query<Vec<(String, String)>>,
) -> Response {
    let Some(at) = At::of(&on, &number, &position) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let session = at.session();
    let (exercise, this) = match found(store, zone, &at).await {
        Ok(found) => found,
        Err(shown) => return shown,
    };
    let typed = Typed::of(
        query
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    if typed.to.is_none() && typed.because.is_empty() {
        return choose(&session, exercise, &this, &typed, None).into_response();
    }
    let (to, because) = match asked(exercise, &typed) {
        Ok(asked) => asked,
        Err(refusal) => {
            return choose(&session, exercise, &this, &typed, Some(&refusal)).into_response();
        }
    };
    match similar(store, exercise, &this).await {
        Ok(similar) => scope(
            &session,
            &Change {
                exercise,
                to,
                because: &because,
                this: &this,
                similar: &similar,
            },
            None,
        )
        .into_response(),
        Err(error) => input::unreadable(&session, TITLE, &error).into_response(),
    }
}

/// Make the change, then show what it changed; or the step again, with why
/// it was refused.
pub async fn posted(
    State(runs): State<Arc<Runs>>,
    Path((on, number, position)): Path<(String, String, String)>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let Some(at) = At::of(&on, &number, &position) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let session = at.session();
    let (exercise, this) = match found(store, zone, &at).await {
        Ok(found) => found,
        Err(shown) => return shown,
    };
    let typed = Typed::of(
        form.iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    let (to, because) = match asked(exercise, &typed) {
        Ok(asked) => asked,
        Err(refusal) => {
            return choose(&session, exercise, &this, &typed, Some(&refusal)).into_response();
        }
    };
    let similar = match similar(store, exercise, &this).await {
        Ok(similar) => similar,
        Err(error) => return input::unreadable(&session, TITLE, &error).into_response(),
    };
    let refused = |refusal: &Refusal| {
        scope(
            &session,
            &Change {
                exercise,
                to,
                because: &because,
                this: &this,
                similar: &similar,
            },
            Some(refusal),
        )
        .into_response()
    };
    let chosen = match chosen(&form, &this, &similar) {
        Ok(chosen) => chosen,
        Err(refusal) => return refused(&refusal),
    };
    if chosen.is_empty() {
        return refused(&Refusal::whole("choose at least one session".to_owned()));
    }
    let mut sessions: BTreeMap<_, SparseGymSession> = BTreeMap::new();
    let mut found = BTreeMap::new();
    for held in chosen {
        sessions
            .entry(held.session)
            .or_default()
            .exercises
            .insert(held.exercise, SparseExercise::was(to));
        found.insert(
            held.exercise,
            Found {
                read: exercise,
                sets: held.sets.iter().copied().collect(),
            },
        );
    }
    match edits::assert(store, &because, &sessions, &found).await {
        Ok(id) => {
            // What History collects, so the session is drawn again from the
            // layers derived under the change by the time it is opened.
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&changed_address(at.on, at.number, id)).into_response()
        }
        Err(error) => refused(&Refusal::whole(error.to_string())),
    }
}

/// The edit a `changed` address names, and the session it was made from.
fn edit_at(on: &str, number: &str, edit: &str) -> Option<(At, EditId)> {
    Some((
        At::of(on, number, "1")?,
        EditId::from(edit.parse::<i64>().ok()?),
    ))
}

/// What a change made from this session changed.
pub async fn made(
    State(runs): State<Arc<Runs>>,
    Path((on, number, edit)): Path<(String, String, String)>,
) -> Response {
    let Some((at, id)) = edit_at(&on, &number, &edit) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let session = at.session();
    let undo = format!("{session}/changed/{id}");
    let shown = async {
        let Some(edit) = edits::one(store, id).await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let renamed: Vec<(CanonicalExerciseId, Exercise)> = edit.exercises().collect();
        let said = if let Some((_, becomes)) = renamed.first() {
            let ids: BTreeSet<CanonicalExerciseId> =
                renamed.iter().map(|(exercise, _)| *exercise).collect();
            let reached = edits::exercises_held(store, &ids)
                .await
                .map_err(|e| e.to_string())?;
            exercises_changed(*becomes, &reached)
        } else if edit.bounds().next().is_some() {
            html! { p { "When the session started and ended is changed." } }
        } else {
            html! {
                @for (_, fields) in edit.sets() { p { "The set now reads " (fields) "." } }
            }
        };
        Ok::<_, String>(Some(changed(&session, &undo, &said)))
    };
    match shown.await {
        Ok(Some(page)) => page.into_response(),
        Ok(None) => Redirect::to(&session).into_response(),
        Err(error) => input::unreadable(&session, "Changed", &error).into_response(),
    }
}

/// Undo a change made from this session, then go back to it.
pub async fn undone(
    State(runs): State<Arc<Runs>>,
    Path((on, number, edit)): Path<(String, String, String)>,
) -> Response {
    let Some((at, id)) = edit_at(&on, &number, &edit) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let session = at.session();
    match edits::retract(&runs.setting().store, id).await {
        Ok(_) => {
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&session).into_response()
        }
        Err(error) => input::unreadable(&session, "Changed", &error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use configurator::edits::HeldExercise;
    use domain::gym::{
        exercise::{DurationExercise, Exercise, RepsExercise},
        identity::{CanonicalExerciseId, CanonicalSessionId, CanonicalSetId},
    };
    use jiff::civil::date;

    use super::{Typed, asked, chosen};

    fn held(id: i64, day: i8, sets: i64) -> HeldExercise {
        HeldExercise {
            session: CanonicalSessionId::try_from(id).expect("an id"),
            exercise: CanonicalExerciseId::try_from(id).expect("an id"),
            day: date(2025, 3, day),
            sets: (1..=sets)
                .map(|set| CanonicalSetId::try_from(id * 100 + set).expect("an id"))
                .collect(),
        }
    }

    fn posted(fields: &[(&str, String)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    const SINGLE_ARM: Exercise =
        Exercise::Reps(RepsExercise::SingleArmOverheadTricepsExtensionCable);

    /// **What it was must be another exercise of its own kind**, and the
    /// reason is required (§ II.2).
    #[test]
    fn the_first_step_asks_for_another_exercise_of_its_kind_and_why() {
        let to = Exercise::Reps(RepsExercise::OverheadTricepsExtensionCable);
        let given = |to: Option<Exercise>, because: &str| Typed {
            to,
            because: because.to_owned(),
        };

        assert!(asked(SINGLE_ARM, &given(Some(to), "both arms")).is_ok());
        assert_eq!(
            asked(SINGLE_ARM, &given(None, "both arms")).map_err(|refusal| refusal.field),
            Err(Some("to"))
        );
        assert_eq!(
            asked(SINGLE_ARM, &given(Some(SINGLE_ARM), "x")).map_err(|refusal| refusal.field),
            Err(Some("to"))
        );
        assert_eq!(
            asked(
                SINGLE_ARM,
                &given(Some(Exercise::Duration(DurationExercise::Stretching)), "x")
            )
            .map_err(|refusal| refusal.field),
            Err(Some("to"))
        );
        assert_eq!(
            asked(SINGLE_ARM, &given(Some(to), "  ")).map_err(|refusal| refusal.field),
            Err(Some("because"))
        );
    }

    /// **"This session only" changes this session's exercise whatever is
    /// ticked**, and otherwise what is ticked is changed.
    #[test]
    fn this_session_only_leaves_the_similar_ones() {
        let this = held(1, 2, 3);
        let similar = [held(2, 1, 4), held(3, 3, 2)];
        let all = posted(&[
            ("exercise", this.exercise.to_string()),
            ("exercise", similar[0].exercise.to_string()),
        ]);

        let ticked: Vec<CanonicalExerciseId> = chosen(&all, &this, &similar)
            .expect("offered exercises")
            .iter()
            .map(|held| held.exercise)
            .collect();
        assert_eq!(ticked, vec![this.exercise, similar[0].exercise]);

        let mut only = all;
        only.push(("only".to_owned(), "this".to_owned()));
        let alone: Vec<CanonicalExerciseId> = chosen(&only, &this, &similar)
            .expect("offered exercises")
            .iter()
            .map(|held| held.exercise)
            .collect();
        assert_eq!(alone, vec![this.exercise]);
    }

    /// **An exercise that was not offered is refused**, rather than asserted
    /// over: the page was drawn from a record that has since changed.
    #[test]
    fn an_exercise_not_offered_is_refused() {
        let this = held(1, 2, 3);
        let gone = held(9, 1, 4);

        assert!(
            chosen(
                &posted(&[("exercise", gone.exercise.to_string())]),
                &this,
                &[]
            )
            .is_err()
        );
        assert!(chosen(&posted(&[]), &this, &[]).is_err());
    }
}
