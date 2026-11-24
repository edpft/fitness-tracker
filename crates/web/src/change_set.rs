//! Changing one set of an opened session: its reps, or its length where it is
//! counted in time or distance, its load, whether the load was assistance,
//! and its effort where it is counted in reps (#892, #893, #901).
//!
//! **This set, and the other sets of its exercise in this session** (#906),
//! ticked by default: one mistake in a session is usually one cause for the
//! whole exercise, as Hevy having no assisted variant left every set of 12
//! Oct 2026's front lever raise without its band. Only the fields changed are
//! applied to each, so each keeps its own reps, and each is its own change,
//! undone on its own.
//!
//! **Never another session's, nor another exercise's.** The operator,
//! 2026-11-21: *"There's nothing necessarily similar about two different sets
//! of 10 x 70kg. The idea behind similar sets was to be able to change
//! exercises."*
//!
//! **Whatever source recorded it.** The edit is anchored to the canonical set
//! (§ II.2), so a set a sheet or a watch recorded takes a reps in reserve it
//! never had, exactly as one logged in Hevy does.
//!
//! **Only what was changed is asserted.** A field posted as it already reads
//! is left out of the edit, so what the sources said, or an earlier edit,
//! still stands for it.

use std::{collections::BTreeMap, str::FromStr as _, sync::Arc};

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
        CanonicalExercise, CanonicalSet, Load, Performed, SetKind,
        edit::{EditReason, Measure, SparseExercise, SparseGymSession, SparseSet, StatedLoad},
        identity::{CanonicalExerciseId, CanonicalSessionId, CanonicalSetId},
        intensity::Rir,
    },
    measure::{Distance, Duration, Kg, Metres, RepCount},
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

/// What a set is counted in, as the form asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    Reps,
    Seconds,
    Metres,
}

impl Counted {
    /// The field's name, and its label.
    const fn field(self) -> (&'static str, &'static str) {
        match self {
            Self::Reps => ("reps", "Reps"),
            Self::Seconds => ("seconds", "Seconds"),
            Self::Metres => ("metres", "Metres"),
        }
    }

    const fn inputmode(self) -> &'static str {
        match self {
            Self::Reps | Self::Seconds => "numeric",
            Self::Metres => "decimal",
        }
    }

    /// The measure typed, which is never nought: a set of none of anything
    /// was not done.
    fn parsed(self, typed: &str) -> Result<Measure, Refusal> {
        let (field, _) = self.field();
        match self {
            Self::Reps => typed
                .parse::<u32>()
                .ok()
                .and_then(|reps| RepCount::new(reps).ok())
                .map(Measure::Reps)
                .ok_or_else(|| Refusal::of(field, "a whole number above nought".to_owned())),
            Self::Seconds => typed
                .parse::<u64>()
                .ok()
                .filter(|seconds| *seconds > 0)
                .map(|seconds| Measure::Duration(Duration::from_seconds(seconds)))
                .ok_or_else(|| {
                    Refusal::of(field, "a whole number of seconds above nought".to_owned())
                }),
            Self::Metres => typed
                .parse::<Metres>()
                .ok()
                .filter(|metres| *metres != Metres::ZERO)
                .map(|metres| Measure::Distance(Distance { metres }))
                .ok_or_else(|| Refusal::of(field, "metres above nought, as 20 or 12.5".to_owned())),
        }
    }

    /// How `measure` is typed into the field.
    fn typed(measure: Measure) -> String {
        match measure {
            Measure::Reps(reps) => reps.to_string(),
            Measure::Duration(duration) => duration.as_seconds().to_string(),
            Measure::Distance(distance) => {
                distance.metres.to_string().trim_end_matches('m').to_owned()
            }
        }
    }
}

/// What a set reads as now.
///
/// **Each may be unstated**: a sheet that wrote "Burpee" and not how many, a
/// set no source put a load on, a set nobody rated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reads {
    pub counted: Counted,
    pub measure: Option<Measure>,
    pub load: Option<Load>,
    pub effort: Option<Rir>,
    /// Whether it can be given an effort: a working set, counted in reps.
    pub rated: bool,
}

impl Reads {
    fn of<M: Copy>(
        set: &CanonicalSet<M>,
        counted: Counted,
        measure: impl Fn(M) -> Measure,
    ) -> Self {
        Self {
            counted,
            measure: set
                .outcome
                .value()
                .completed()
                .copied()
                .flatten()
                .map(measure),
            load: set.load.as_ref().map(Attributed::copied),
            effort: set.intensity.as_ref().map(Attributed::copied),
            rated: counted == Counted::Reps
                && set.kind.as_ref().map(Attributed::copied) != Some(SetKind::Warmup),
        }
    }
}

/// The form as posted or as first drawn.
#[derive(Debug, Default)]
pub struct Typed {
    pub measure: String,
    pub kg: String,
    pub assistance: bool,
    pub effort: String,
    pub because: String,
    /// The other sets ticked, by their ids as posted: [`None`] when the form
    /// is first drawn, which ticks every one.
    pub also: Option<Vec<String>>,
}

impl Typed {
    fn reading(reads: Reads) -> Self {
        let (kg, assistance) = match reads.load.map(StatedLoad::of) {
            None | Some(StatedLoad::Nothing) => (String::new(), false),
            Some(StatedLoad::Mass(mass)) => (mass.to_string(), false),
            Some(StatedLoad::Assistance(mass)) => (mass.to_string(), true),
        };
        Self {
            measure: reads.measure.map(Counted::typed).unwrap_or_default(),
            kg,
            assistance,
            effort: reads.effort.map(Rir::as_str).unwrap_or_default().to_owned(),
            because: String::new(),
            also: None,
        }
    }

    fn of(form: &[(String, String)]) -> Self {
        let mut typed = Self {
            also: Some(Vec::new()),
            ..Self::default()
        };
        for (key, value) in form {
            match key.as_str() {
                "reps" | "seconds" | "metres" => value.trim().clone_into(&mut typed.measure),
                "kg" => value.trim().clone_into(&mut typed.kg),
                "assistance" => typed.assistance = true,
                "effort" => value.clone_into(&mut typed.effort),
                "because" => value.clone_into(&mut typed.because),
                "also" => typed.also.get_or_insert_default().push(value.clone()),
                _ => {}
            }
        }
        typed
    }
}

/// What a post changed on the set its pen is on: each field that differs
/// from what that set reads as now, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changed {
    measure: Option<Measure>,
    load: Option<StatedLoad>,
    effort: Option<Rir>,
}

impl Changed {
    /// The edit it makes to a set that reads `reads`: each changed field
    /// that differs from what that set reads, the load on that set's own
    /// axis. [`None`] where the set already reads as asked.
    ///
    /// **An effort reaches only a set that has one**, so a warm-up ticked
    /// beside working sets takes their band and not their reps in reserve.
    pub fn on(self, reads: Reads) -> Option<SparseSet> {
        SparseSet::new(
            self.measure
                .filter(|measure| Some(*measure) != reads.measure),
            self.load
                .map(|stated| stated.on(reads.load))
                .filter(|load| Some(*load) != reads.load),
            self.effort
                .filter(|effort| reads.rated && Some(*effort) != reads.effort),
        )
    }
}

/// What a post asks for, and why.
///
/// **A field left blank where nothing was stated changes nothing**: an empty
/// kg on a set no source put a load on is not a claim that there was none.
///
/// # Errors
///
/// [`Refusal`] beside the field at fault, or of the whole form if nothing was
/// changed.
pub fn asked(reads: Reads, typed: &Typed) -> Result<(Changed, EditReason), Refusal> {
    let measure = if typed.measure.is_empty() && reads.measure.is_none() {
        None
    } else {
        Some(reads.counted.parsed(&typed.measure)?)
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
    let effort = if typed.effort.is_empty() {
        None
    } else if reads.rated {
        Some(
            Rir::from_str(&typed.effort)
                .map_err(|_| Refusal::of("effort", "choose how many were in reserve".to_owned()))?,
        )
    } else if reads.counted == Counted::Reps {
        return Err(Refusal::of(
            "effort",
            "only a working set has an effort".to_owned(),
        ));
    } else {
        return Err(Refusal::of(
            "effort",
            "only a set counted in reps has an effort".to_owned(),
        ));
    };
    let because = EditReason::try_from(typed.because.clone())
        .map_err(|_| Refusal::of("because", "say why".to_owned()))?;
    let asked = Changed {
        measure: measure.filter(|measure| Some(*measure) != reads.measure),
        load: stated.filter(|stated| Some(stated.on(reads.load)) != reads.load),
        effort: effort.filter(|effort| Some(*effort) != reads.effort),
    };
    asked
        .on(reads)
        .map(|_| (asked, because))
        .ok_or_else(|| Refusal::whole("nothing was changed".to_owned()))
}

/// The other sets a post ticked, each one that was offered.
///
/// **Only what was offered**: a set posted that is not among those found now
/// is a page drawn before the record changed, and is refused rather than
/// asserted over.
///
/// # Errors
///
/// [`Refusal`] if one was not offered.
pub fn ticked<'a>(typed: &Typed, others: &'a [Other]) -> Result<Vec<&'a Other>, Refusal> {
    let stale = || {
        Refusal::whole("the record has changed since this page was drawn; open it again".to_owned())
    };
    let mut ticked: Vec<&Other> = Vec::new();
    for posted in typed.also.iter().flatten() {
        let other = others
            .iter()
            .find(|other| other.id.to_string() == *posted)
            .ok_or_else(stale)?;
        if !ticked.iter().any(|already| already.id == other.id) {
            ticked.push(other);
        }
    }
    Ok(ticked)
}

/// What the session calls the set: a warm-up, or its place among the working
/// sets, as its table numbers them.
pub fn named(warm_ups: &[bool], index: usize) -> String {
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
                @let (counted, label) = found.reads.counted.field();
                (input::field(counted, counted, label, &html! {
                    input id=(counted) name=(counted) inputmode=(found.reads.counted.inputmode()) value=(typed.measure);
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
                @if !found.others.is_empty() {
                    fieldset.field {
                        legend { "The other sets of " (found.exercise) " in this session" }
                        @for other in &found.others {
                            @let id = other.id.to_string();
                            label {
                                input type="checkbox" name="also" value=(id)
                                    checked[typed.also.as_ref().is_none_or(|also| also.contains(&id))];
                                " " (capitalised(&other.name)) (reading(other.reads))
                            }
                        }
                    }
                }
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

/// What a set reads as, after its name: nothing where nothing was stated.
fn reading(reads: Reads) -> String {
    SparseSet::new(reads.measure, reads.load, reads.effort)
        .map(|reading| format!(": {reading}"))
        .unwrap_or_default()
}

fn capitalised(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The set a change is asked of, as the layer holds it.
pub struct Found {
    exercise: String,
    name: String,
    session: CanonicalSessionId,
    exercise_id: CanonicalExerciseId,
    id: CanonicalSetId,
    reads: Reads,
    /// The other sets of its exercise in this session that were done, in
    /// order.
    others: Vec<Other>,
}

/// Another set of the exercise in this session, offered beside it.
#[derive(Debug, Clone)]
pub struct Other {
    pub id: CanonicalSetId,
    pub name: String,
    pub reads: Reads,
}

/// The set there, or the page to show instead.
async fn found(store: &Store, zone: &OperatorZone, at: &At) -> Result<Found, Response> {
    let session = at.session();
    let unreadable = |error: &str| input::unreadable(&session, TITLE, error).into_response();
    let canonical = match change::located(store, zone, at.on, at.number, at.position).await {
        Ok(Some(canonical)) => canonical,
        Ok(None) => return Err(Redirect::to(&session).into_response()),
        Err(error) => return Err(unreadable(&error)),
    };
    let index = at.set - 1;
    // Each set as it reads now, where it was done.
    let every: Vec<Option<Reads>> = match &canonical {
        CanonicalExercise::ForReps { sets, .. } => sets
            .iter()
            .map(|set| completed(set).then(|| Reads::of(set, Counted::Reps, Measure::Reps)))
            .collect(),
        CanonicalExercise::ForDuration { sets, .. } => sets
            .iter()
            .map(|set| completed(set).then(|| Reads::of(set, Counted::Seconds, Measure::Duration)))
            .collect(),
        CanonicalExercise::ForDistance { sets, .. } => sets
            .iter()
            .map(|set| completed(set).then(|| Reads::of(set, Counted::Metres, Measure::Distance)))
            .collect(),
    };
    let Some(reads) = every.get(index).copied().flatten() else {
        return Err(Redirect::to(&session).into_response());
    };
    let (held, ids) = edits::ids_of(store, &canonical)
        .await
        .map_err(|error| unreadable(&error.to_string()))?
        .ok_or_else(|| unreadable("the session is being rebuilt; open it again in a moment"))?;
    let Some(id) = ids.sets.get(index).copied() else {
        return Err(Redirect::to(&session).into_response());
    };
    let warm_ups = history::warm_ups(&canonical);
    let others = every
        .iter()
        .zip(&ids.sets)
        .enumerate()
        .filter(|(other, _)| *other != index)
        .filter_map(|(other, (reads, id))| {
            reads.map(|reads| Other {
                id: *id,
                name: named(&warm_ups, other),
                reads,
            })
        })
        .collect();
    Ok(Found {
        exercise: history::exercise_name(&canonical),
        name: named(&warm_ups, index),
        session: held,
        exercise_id: ids.exercise,
        id,
        reads,
        others,
    })
}

/// Whether a set was done. A failed attempt has nothing to change its count
/// from, and is not offered a pen.
const fn completed<M>(set: &CanonicalSet<M>) -> bool {
    !matches!(set.outcome.value(), Performed::Failed)
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
    let (asked, because) = match asked(found.reads, &typed) {
        Ok(asked) => asked,
        Err(refusal) => return refused(&refusal),
    };
    let ticked = match ticked(&typed, &found.others) {
        Ok(ticked) => ticked,
        Err(refusal) => return refused(&refusal),
    };
    // One change for each set it reaches, so each is undone on its own; a
    // ticked set that already reads as asked has none.
    let each: Vec<BTreeMap<_, _>> = std::iter::once((found.id, found.reads))
        .chain(ticked.iter().map(|other| (other.id, other.reads)))
        .filter_map(|(id, reads)| asked.on(reads).map(|edit| (id, edit)))
        .map(|(id, edit)| {
            BTreeMap::from([(
                found.session,
                SparseGymSession::exercise(found.exercise_id, SparseExercise::set(id, edit)),
            )])
        })
        .collect();
    match edits::assert_each(store, &because, &each).await {
        Ok(ids) => {
            // What History collects, so the session is drawn again from the
            // layers derived under the change by the time it is opened.
            runs.start(&Asked::Only(configurator::history::collects()));
            // One set's change is shown with its undo; several are listed on
            // the session, each with its own.
            match ids.as_slice() {
                [id] => {
                    Redirect::to(&change::changed_address(at.on, at.number, *id)).into_response()
                }
                _ => Redirect::to(&session).into_response(),
            }
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

    use domain::gym::edit::Measure;

    use domain::gym::{edit::SparseSet, identity::CanonicalSetId};

    use super::{Counted, Reads, Typed, asked};

    /// 25 Sep's 90 kg front squat single, unrated.
    fn single() -> Reads {
        Reads {
            counted: Counted::Reps,
            measure: Some(Measure::Reps(RepCount::new(1).expect("one"))),
            load: Some(Load::absolute(Kg::from_grams(90_000))),
            effort: None,
            rated: true,
        }
    }

    fn typed(measure: &str, kg: &str, effort: &str) -> Typed {
        Typed {
            measure: measure.to_owned(),
            kg: kg.to_owned(),
            assistance: false,
            effort: effort.to_owned(),
            because: "forgot to rate it".to_owned(),
            also: None,
        }
    }

    /// The edit a post makes to the set its pen is on.
    fn edit_of(reads: Reads, typed: &Typed) -> Option<SparseSet> {
        let (changed, _) = asked(reads, typed).ok()?;
        changed.on(reads)
    }

    /// **Only what was changed is asserted**: rating the 90 kg single leaves
    /// its reps and load out of the edit.
    #[test]
    fn rating_a_set_asserts_only_its_effort() {
        let edit = edit_of(single(), &typed("1", "90", "1")).expect("an edit");

        assert_eq!(edit.measure(), None);
        assert_eq!(edit.load(), None);
        assert_eq!(edit.intensity(), Some(Rir::One));
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
        let edit = edit_of(single(), &typed("1", "92.5", "")).expect("an edit");
        assert_eq!(edit.load(), Some(Load::absolute(Kg::from_grams(92_500))));
        assert_eq!(edit.measure(), None);

        let pull_up = Reads {
            load: Some(Load::BODYWEIGHT),
            ..single()
        };
        let edit = edit_of(pull_up, &typed("1", "10", "")).expect("an edit");
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
        let edit = edit_of(sheet, &typed("1", "", "2")).expect("an edit");

        assert_eq!(edit.load(), None);
        assert_eq!(edit.intensity(), Some(Rir::Two));
    }

    /// 25 Sep's dead hang: 40 s, no load, as Hevy recorded it.
    fn hang() -> Reads {
        Reads {
            counted: Counted::Seconds,
            measure: Some(Measure::Duration(domain::measure::Duration::from_seconds(
                40,
            ))),
            load: Some(Load::UNLOADED),
            effort: None,
            rated: false,
        }
    }

    /// **A hold takes a length and a load** (#901), and only what changed:
    /// 45 s with 10 kg, or 10 kg alone with the 40 s left as it was.
    #[test]
    fn a_hold_is_given_a_length_and_a_load() {
        let edit = edit_of(hang(), &typed("45", "10", "")).expect("an edit");
        assert_eq!(edit.to_string(), "45 s, 10 kg");

        let edit = edit_of(hang(), &typed("40", "10", "")).expect("an edit");
        assert_eq!(edit.measure(), None);
        assert_eq!(edit.load(), Some(Load::absolute(Kg::from_grams(10_000))));
    }

    /// A hold of nought seconds was not done, and a hold has no reps in
    /// reserve to give it.
    #[test]
    fn a_hold_is_refused_nought_seconds_and_an_effort() {
        let refused = |typed| asked(hang(), &typed).map_err(|refusal| refusal.field);

        assert_eq!(refused(typed("0", "", "")), Err(Some("seconds")));
        assert_eq!(refused(typed("40", "", "1")), Err(Some("effort")));
    }

    /// A distance is typed in metres, to the millimetre.
    #[test]
    fn a_carry_is_given_metres() {
        let carry = Reads {
            counted: Counted::Metres,
            measure: Some(Measure::Distance(domain::measure::Distance {
                metres: domain::measure::Metres::from_millimetres(20_000),
            })),
            ..hang()
        };
        let edit = edit_of(carry, &typed("12.5", "", "")).expect("an edit");

        assert_eq!(edit.to_string(), "12.5m");
        assert_eq!(
            super::Counted::typed(carry.measure.expect("a distance")),
            "20"
        );
    }

    /// 12 Oct 2026's front lever raise, as Hevy recorded it: no variant
    /// with assistance, so a set of `reps` at bodyweight.
    fn front_lever(reps: u32) -> Reads {
        Reads {
            counted: Counted::Reps,
            measure: Some(Measure::Reps(RepCount::new(reps).expect("reps"))),
            load: Some(Load::BODYWEIGHT),
            effort: None,
            rated: true,
        }
    }

    fn assisted(reps: u32, kg: &str) -> Typed {
        Typed {
            assistance: true,
            ..typed(&reps.to_string(), kg, "")
        }
    }

    /// **Only the fields changed reach the other sets** (#906): 21 kg of
    /// assistance from the first set's pen gives a set of 5 its 21 kg and
    /// leaves it 5 reps.
    #[test]
    fn the_other_sets_take_only_what_was_changed() {
        let (changed, _) = asked(front_lever(6), &assisted(6, "21")).expect("a change");
        let twenty_one = Some(Load::relative(SignedKg::from_grams(-21_000)));

        let first = changed.on(front_lever(6)).expect("an edit");
        assert_eq!((first.measure(), first.load()), (None, twenty_one));

        let fewer = changed.on(front_lever(5)).expect("an edit");
        assert_eq!((fewer.measure(), fewer.load()), (None, twenty_one));
    }

    /// A ticked set that already reads as asked is given no change, so it
    /// has nothing to undo.
    #[test]
    fn a_set_already_as_asked_is_not_changed() {
        let (changed, _) = asked(front_lever(6), &assisted(6, "21")).expect("a change");
        let already = Reads {
            load: Some(Load::relative(SignedKg::from_grams(-21_000))),
            ..front_lever(6)
        };

        assert_eq!(changed.on(already), None);
    }

    /// **A warm-up ticked beside working sets takes their band, not their
    /// reps in reserve**: it has no effort to give.
    #[test]
    fn a_warm_up_takes_the_load_and_not_the_effort() {
        let (changed, _) = asked(
            front_lever(6),
            &Typed {
                effort: "2".to_owned(),
                ..assisted(6, "21")
            },
        )
        .expect("a change");
        let warm_up = Reads {
            rated: false,
            ..front_lever(6)
        };

        let edit = changed.on(warm_up).expect("an edit");
        assert_eq!(edit.intensity(), None);
        assert!(edit.load().is_some());
    }

    /// **Only the sets offered are changed**: a set posted that is not
    /// among them is a page drawn before the record changed.
    #[test]
    fn a_set_not_offered_is_refused() {
        let other = |id: i64| super::Other {
            id: CanonicalSetId::try_from(id).expect("an id"),
            name: format!("set {id}"),
            reads: front_lever(6),
        };
        let others = [other(2), other(3)];
        let posting = |also: &[&str]| Typed {
            also: Some(also.iter().map(|id| (*id).to_owned()).collect()),
            ..assisted(6, "21")
        };

        let ticked: Vec<_> = super::ticked(&posting(&["3"]), &others)
            .expect("offered sets")
            .iter()
            .map(|other| other.id)
            .collect();
        assert_eq!(ticked, vec![others[1].id]);
        assert!(super::ticked(&posting(&["9"]), &others).is_err());
        assert!(
            super::ticked(&posting(&[]), &others)
                .expect("none ticked")
                .is_empty()
        );
    }
}
