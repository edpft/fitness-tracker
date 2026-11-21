//! Changing what an exercise in an opened session was (#754).
//!
//! The operator's reference is his budgeting app (2026-11-10): *"it allows me
//! to edit a transaction and then asks me if I want this edit to apply to more
//! than one transaction."* So the change is two steps: what it was and why,
//! then whether it goes for this session alone or for the similar ones too,
//! listed by day with the button saying how many sets it changes. Then what
//! was changed.
//!
//! **Similar is every other entry that reads as the same exercise**, which is
//! what a stand-in is: Hevy had no entry for what he did, so he logged it
//! under the nearest one, every time.
//!
//! **What arrives later is not changed** (§ II.2, *"overrides do not
//! propagate"*): the entries are the ones listed when he changes them.

use std::{collections::BTreeMap, sync::Arc};

use application::history::{Number, opened};
use axum::{
    Form,
    extract::{Path, Query, State},
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{
    corrections::{self, CorrectionError, StandIn},
    store::Store,
};
use domain::{
    gym::{
        CanonicalExercise,
        exercise::{DistanceExercise, DurationExercise, Exercise, RepsExercise},
    },
    landing::SourceRecordId,
    normalised::{CorrectedTerm, CorrectionId, CorrectionReason, OperatorZone, SourceTerm},
    sequence::NonEmpty,
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

/// Where what a change changed is shown.
pub const CHANGED: &str = "/history/{on}/{number}/changed/{correction}";

const TITLE: &str = "Change exercise";

/// Recounts the sets the button changes as entries are ticked.
const SCRIPT: &str = "
function recount(form) {
  const chosen = [...form.querySelectorAll('input[name=entry]')]
    .filter((entry) => entry.type === 'hidden' || entry.checked);
  const sets = chosen.reduce((total, entry) => total + Number(entry.dataset.sets), 0);
  const days = new Set(chosen.map((entry) => entry.dataset.day)).size;
  form.querySelector('button.all').textContent =
    'Change ' + counted(sets, 'set') + ' over ' + counted(days, 'session');
}
function counted(count, what) { return count === 1 ? '1 ' + what : count + ' ' + what + 's'; }
";

/// Where what correction `id` changed is shown, from the session it was made
/// on.
pub fn changed_address(on: Date, number: Number, id: CorrectionId) -> String {
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
    this: &[StandIn],
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
            p.sub { (logged(this)) }
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
    let sets: usize = change
        .this
        .iter()
        .chain(change.similar)
        .map(|entry| entry.sets)
        .sum();
    let days = days(change.this.iter().chain(change.similar));
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
                p { (logged(change.this)) }
                @for entry in change.this {
                    input type="hidden" name="entry" value=(entry_value(entry))
                        data-sets=(entry.sets) data-day=(entry.day);
                }
                button name="only" value="this" { "Change this session only" }
                @if !similar.is_empty() {
                    h2 { "Similar" }
                    p.sub { "Every other session where " (change.exercise.name()) " was logged." }
                    @for (day, entries) in &similar {
                        fieldset.field {
                            legend { (day.0.strftime("%a %-d %b %Y")) }
                            @for entry in entries {
                                label {
                                    input type="checkbox" name="entry" value=(entry_value(entry))
                                        data-sets=(entry.sets) data-day=(entry.day) checked;
                                    " " (sets_logged(entry))
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

/// An exercise that cannot be changed yet, and why.
pub fn not_hevy(session: &str, on: Date, exercise: Exercise) -> Markup {
    input::page(
        session,
        TITLE,
        &html! {
            p {
                "Nothing logged in Hevy on " (on.strftime("%-d %b %Y")) " reads as " (exercise.name())
                ", and only what was logged in Hevy can be changed so far."
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
}

/// What a change changed.
pub fn changed(session: &str, exercise: Exercise, reached: &[StandIn]) -> Markup {
    let sets: usize = reached.iter().map(|entry| entry.sets).sum();
    let by_day = by_day(reached);
    input::page(
        session,
        "Changed",
        &html! {
            p {
                (counted(sets, "set")) " over " (counted(by_day.len(), "session"))
                " now read as " (exercise.name()) "."
            }
            ul.sessions {
                @for (day, entries) in &by_day {
                    li {
                        (day.0.strftime("%a %-d %b %Y")) ": "
                        (counted(entries.iter().map(|entry| entry.sets).sum(), "set"))
                    }
                }
            }
            p { a href=(session) { "Back to the session" } }
        },
    )
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
    pub because: &'a CorrectionReason,
    pub this: &'a [StandIn],
    pub similar: &'a [StandIn],
}

/// What the first step asks: the exercise it was, and why.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault.
pub fn asked(exercise: Exercise, typed: &Typed) -> Result<(Exercise, CorrectionReason), Refusal> {
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
    let because = CorrectionReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    Ok((to, because))
}

/// The entries a post chose, each one that was offered.
///
/// **Only what was offered**: an entry posted that is not among those found
/// now is a page drawn before the record changed, and is refused rather than
/// asserted over.
///
/// # Errors
///
/// [`Refusal`] if none was chosen or one was not offered.
pub fn chosen(
    form: &[(String, String)],
    this: &[StandIn],
    similar: &[StandIn],
) -> Result<Vec<CorrectedTerm>, Refusal> {
    let only_this = form
        .iter()
        .any(|(key, value)| key == "only" && value == "this");
    let offered: Vec<&StandIn> = this.iter().chain(similar).collect();
    let mut chosen = Vec::new();
    for (key, value) in form {
        if key != "entry" {
            continue;
        }
        let term = parse_entry(value).ok_or_else(stale)?;
        if !offered.iter().any(|entry| entry.term == term) {
            return Err(stale());
        }
        if only_this && !this.iter().any(|entry| entry.term == term) {
            continue;
        }
        if !chosen.contains(&term) {
            chosen.push(term);
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

/// An entry as a form posts it: the workout, then the source's term for the
/// exercise in it.
fn entry_value(entry: &StandIn) -> String {
    format!("{}|{}", entry.term.record(), entry.term.term())
}

fn parse_entry(value: &str) -> Option<CorrectedTerm> {
    let (record, term) = value.split_once('|')?;
    Some(CorrectedTerm::new(
        SourceRecordId::try_from(record).ok()?,
        SourceTerm::try_from(term).ok()?,
    ))
}

fn by_day(entries: &[StandIn]) -> BTreeMap<std::cmp::Reverse<Date>, Vec<&StandIn>> {
    let mut days: BTreeMap<std::cmp::Reverse<Date>, Vec<&StandIn>> = BTreeMap::new();
    for entry in entries {
        days.entry(std::cmp::Reverse(entry.day))
            .or_default()
            .push(entry);
    }
    days
}

fn days<'a>(entries: impl Iterator<Item = &'a StandIn>) -> usize {
    entries
        .map(|entry| entry.day)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

/// How this session logged it: its sets, under Hevy's own name.
fn logged(entries: &[StandIn]) -> String {
    entries
        .iter()
        .map(sets_logged)
        .collect::<Vec<_>>()
        .join(", ")
}

fn sets_logged(entry: &StandIn) -> String {
    format!("{} logged as {}", counted(entry.sets, "set"), entry.title)
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

/// The exercise there.
async fn located(store: &Store, zone: &OperatorZone, at: &At) -> Result<Option<Exercise>, String> {
    let activity = configurator::history::Opened::open(store, zone)
        .activity(at.on, at.number)
        .await
        .map_err(|error| error.to_string())?;
    let Some(opened::Activity::Gym { session, .. }) = activity else {
        return Ok(None);
    };
    Ok(session
        .exercises()
        .nth(at.position - 1)
        .and_then(CanonicalExercise::exercise_key)
        .and_then(Exercise::named))
}

/// Every other entry that reads as `exercise`.
async fn similar(
    store: &Store,
    zone: &OperatorZone,
    exercise: Exercise,
    this: &[StandIn],
) -> Result<Vec<StandIn>, CorrectionError> {
    Ok(corrections::reading_as(store, exercise, zone, None, None)
        .await?
        .into_iter()
        .filter(|entry| !this.contains(entry))
        .collect())
}

/// What a change is asked of, read from the store: the exercise and this
/// session's entries of it, or the page to show instead.
async fn found(
    store: &Store,
    zone: &OperatorZone,
    at: &At,
) -> Result<(Exercise, Vec<StandIn>), Response> {
    let session = at.session();
    let unreadable = |error: &str| input::unreadable(&session, TITLE, error).into_response();
    let exercise = match located(store, zone, at).await {
        Ok(Some(exercise)) => exercise,
        Ok(None) => return Err(Redirect::to(&session).into_response()),
        Err(error) => return Err(unreadable(&error)),
    };
    let this = corrections::in_session(store, exercise, zone, at.on)
        .await
        .map_err(|error| unreadable(&error.to_string()))?;
    if this.is_empty() {
        return Err(not_hevy(&session, at.on, exercise).into_response());
    }
    Ok((exercise, this))
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
    match similar(store, zone, exercise, &this).await {
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
        Err(error) => input::unreadable(&session, TITLE, &error.to_string()).into_response(),
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
    let similar = match similar(store, zone, exercise, &this).await {
        Ok(similar) => similar,
        Err(error) => {
            return input::unreadable(&session, TITLE, &error.to_string()).into_response();
        }
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
    let terms = match chosen(&form, &this, &similar).map(NonEmpty::new) {
        Ok(Ok(terms)) => terms,
        Ok(Err(_)) => return refused(&Refusal::whole("choose at least one session".to_owned())),
        Err(refusal) => return refused(&refusal),
    };
    match corrections::assert_exercise(store, to, &because, &terms).await {
        Ok(id) => {
            // What History collects, so the session is drawn again from the
            // layers derived under the change by the time it is opened.
            runs.start(&Asked::Only(configurator::history::collects()));
            Redirect::to(&changed_address(at.on, at.number, id)).into_response()
        }
        Err(error) => refused(&Refusal::whole(error.to_string())),
    }
}

/// What a change made from this session changed.
pub async fn made(
    State(runs): State<Arc<Runs>>,
    Path((on, number, correction)): Path<(String, String, String)>,
) -> Response {
    let (Some(at), Ok(id)) = (At::of(&on, &number, "1"), correction.parse::<i64>()) else {
        return Redirect::to(Place::History.path()).into_response();
    };
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let session = at.session();
    let shown = async {
        let Some(correction) = corrections::one(store, CorrectionId::from(id)).await? else {
            return Ok(None);
        };
        let (exercise, entries) = match correction.corrected() {
            domain::normalised::Corrected::Exercise { exercise, entries } => (exercise, entries),
            domain::normalised::Corrected::Set { recorded, edit, .. } => {
                return Ok(Some(crate::change_set::changed(&session, *recorded, *edit)));
            }
        };
        let reached: Vec<StandIn> = corrections::reading_as(store, *exercise, zone, None, None)
            .await?
            .into_iter()
            .filter(|entry| entries.iter().any(|term| *term == entry.term))
            .collect();
        Ok::<_, CorrectionError>(Some(changed(&session, *exercise, &reached)))
    };
    match shown.await {
        Ok(Some(page)) => page.into_response(),
        Ok(None) => Redirect::to(&session).into_response(),
        Err(error) => input::unreadable(&session, "Changed", &error.to_string()).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use configurator::corrections::StandIn;
    use domain::{
        gym::exercise::{DurationExercise, Exercise, RepsExercise},
        landing::SourceRecordId,
        normalised::{CorrectedTerm, SourceTerm},
    };
    use jiff::civil::date;

    use super::{Typed, asked, chosen, entry_value};

    fn entry(record: &str, day: i8, sets: usize) -> StandIn {
        StandIn {
            term: CorrectedTerm::new(
                SourceRecordId::try_from(record).expect("a record"),
                SourceTerm::try_from("A1B2").expect("a term"),
            ),
            day: date(2025, 3, day),
            title: "Triceps Extension (Cable)".to_owned(),
            sets,
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

    /// **"This session only" changes this session's entries whatever is
    /// ticked**, and otherwise what is ticked is changed.
    #[test]
    fn this_session_only_leaves_the_similar_ones() {
        let this = [entry("this", 2, 3)];
        let similar = [entry("other", 1, 4), entry("another", 3, 2)];
        let all = posted(&[
            ("entry", entry_value(&this[0])),
            ("entry", entry_value(&similar[0])),
        ]);

        let ticked = chosen(&all, &this, &similar).expect("offered entries");
        assert_eq!(ticked, vec![this[0].term.clone(), similar[0].term.clone()]);

        let mut only = all;
        only.push(("only".to_owned(), "this".to_owned()));
        let alone = chosen(&only, &this, &similar).expect("offered entries");
        assert_eq!(alone, vec![this[0].term.clone()]);
    }

    /// **An entry that was not offered is refused**, rather than asserted
    /// over: the page was drawn from a record that has since changed.
    #[test]
    fn an_entry_not_offered_is_refused() {
        let this = [entry("this", 2, 3)];
        let gone = entry("gone", 1, 4);

        assert!(chosen(&posted(&[("entry", entry_value(&gone))]), &this, &[]).is_err());
        assert!(chosen(&posted(&[]), &this, &[]).is_err());
    }
}
