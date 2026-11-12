//! The workout page (#771): one workout drawn as it runs, a row per slot, and
//! each row opening to change that slot in one form.
//!
//! **What a row changes takes effect two ways**, until #543 lets a workout
//! change from a date: the exercise, what assists or loads it, and a static
//! slot's sets and reps are a fill change from the date given; any other
//! slot's sets, reps and rest, and the hold, are a new version of the workout,
//! which the next plan takes.
//!
//! **The structure is the template's**: rows are not added, removed or
//! reordered. The primary's row opens its progression rather than a change of
//! exercise, since the primary is what the mesocycle progresses.

use application::{FillChangeError, MesocycleInForce, design::InForce};
use domain::{
    gym::{
        equipment::{EquipmentFacts, Maker, Model, Specific},
        exercise::{DurationExercise, Exercise, Implement, RepsExercise},
    },
    measure::{Duration, RepCount},
    planner::{Relative, SessionRole},
    prescription::{
        AccessoryScheme, FillChange, GymMesocycle, Progression, SbsChart, Section, SlotId,
        StaticFill, Superset, Target, Workout,
        linear::{Position, PrimaryPattern, SlotContent},
    },
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    input::{self, Refusal},
    progression,
};

pub const HERE: &str = "/workout";

const TITLE: &str = "Workout";

/// The two workouts, with the words the choice uses for them.
const WORKOUTS: [(Relative, &str); 2] = [
    (Relative::Higher, "Higher intensity"),
    (Relative::Lower, "Lower intensity"),
];

/// A post that was refused: the form as it was posted, and why.
pub struct Refused<'a> {
    pub typed: &'a [(String, String)],
    pub refusal: &'a Refusal,
}

impl Refused<'_> {
    fn posted(&self, name: &str) -> Option<&str> {
        self.typed
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// What was typed under `name`, if this is the form for that slot.
    fn typed(&self, slot: SlotId, name: &str) -> Option<&str> {
        (self.posted("slot") == Some(slot.as_str())).then(|| self.posted(name).unwrap_or_default())
    }
}

/// Everything the page is drawn from.
pub struct Shown<'a> {
    pub intensity: Relative,
    pub found: Option<&'a MesocycleInForce>,
    pub in_force: &'a InForce,
    pub facts: &'a EquipmentFacts,
    pub today: Date,
}

/// The page for one workout, with the last post if it was refused.
pub fn page(shown: &Shown<'_>, refused: Option<&Refused<'_>>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (crate::page::head(TITLE, &[progression::STYLE]))
                script { (maud::PreEscaped(progression::SCRIPT)) }
            }
            body {
                main {
                    (crate::page::bar(HERE))
                    (crate::page::header(HERE, TITLE))
                    nav.choice aria-label="Workout" {
                        @for (intensity, words) in WORKOUTS {
                            @let href = format!("{HERE}?intensity={}", intensity.as_str());
                            @if intensity == shown.intensity {
                                a href=(href) aria-current="page" { (words) }
                            } @else {
                                a href=(href) { (words) }
                            }
                        }
                    }
                    @match (shown.found, shown.in_force.workouts.of(shown.intensity)) {
                        (None, _) => p { "No mesocycle is in force today." },
                        (_, None) => p { "No workout is stored. `fitness init` stores the first." },
                        (Some(found), Some((at, workout))) => {
                            p.foot { (found.plan) ", mesocycle " (found.ordinal) }
                            @if found.planned_at < *at {
                                p.legend {
                                    "This workout was changed after the plan in force was made: "
                                    "its sets, reps, rest and hold reach the next plan."
                                }
                            }
                            section.card.next.workout {
                                (drawn(shown, found, workout, refused))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    input::unreadable(HERE, TITLE, error)
}

/// The session role a workout of this intensity is, for reading its fills.
const fn role(intensity: Relative) -> SessionRole {
    SessionRole::new(intensity, intensity)
}

/// The superset a slot runs in, if it runs in one.
fn within(primary: PrimaryPattern, slot: SlotId) -> Option<Superset> {
    primary.sequence().into_iter().find_map(|position| {
        let slots: Vec<SlotId> = position.slots().collect();
        match slots.as_slice() {
            [first, second, rest @ ..] if slots.contains(&slot) => {
                Some(Superset::of(*first, *second, rest))
            }
            _ => None,
        }
    })
}

/// How a slot's numbers are stated, which decides what its form asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// The primary: its progression's.
    Primary,
    /// Plyometric and power: the fill's own sets and reps.
    Static,
    /// Strength and hypertrophy: the workout's sets and reps.
    Progressing,
    /// Mobility: the workout's hold.
    Hold,
}

fn kind(slot: SlotId, primary: PrimaryPattern) -> Kind {
    if slot == primary.slot() {
        return Kind::Primary;
    }
    match slot.section() {
        Section::Plyometric | Section::Power => Kind::Static,
        Section::Strength | Section::Hypertrophy => Kind::Progressing,
        Section::Mobility => Kind::Hold,
    }
}

/// One slot as the workout runs it today, and what its form starts from.
struct Row<'a> {
    slot: SlotId,
    kind: Kind,
    within: Option<Superset>,
    held: Exercise,
    statics: Option<StaticFill>,
    /// What it is to hold: the latest change, where one is dated later.
    latest: Option<&'a FillChange>,
    /// Whether the other workout holds the same, so a change reaches both.
    same: bool,
}

fn rows(found: &MesocycleInForce, intensity: Relative, today: Date) -> Vec<Row<'_>> {
    let changed = found.mesocycle.clone().changed_on(&found.changes, today);
    let fills = changed.fills();
    let primary = found.mesocycle.primary();
    let content = |slot, intensity| match fills.content(slot, role(intensity)) {
        SlotContent::Single(exercise) => (*exercise, None),
        SlotContent::Static(fill) => (fill.exercise, Some(*fill)),
    };
    let other = match intensity {
        Relative::Higher => Relative::Lower,
        Relative::Lower => Relative::Higher,
    };
    primary
        .sequence()
        .into_iter()
        .flat_map(Position::slots)
        .map(|slot| {
            let (held, statics) = content(slot, intensity);
            Row {
                slot,
                kind: kind(slot, primary),
                within: within(primary, slot),
                held,
                statics,
                latest: found.changes.latest(slot, intensity),
                same: content(slot, other) == (held, statics)
                    && found.changes.latest(slot, other) == found.changes.latest(slot, intensity),
            }
        })
        .collect()
}

/// The workout, section by section, the members of a superset boxed together
/// with the rest after their round.
fn drawn(
    shown: &Shown<'_>,
    found: &MesocycleInForce,
    workout: &Workout,
    refused: Option<&Refused<'_>>,
) -> Markup {
    let rows = rows(found, shown.intensity, shown.today);
    let mut sections: Vec<(Section, Vec<Vec<&Row<'_>>>)> = Vec::new();
    for row in &rows {
        let section = row.slot.section();
        if sections.last().is_none_or(|(last, _)| *last != section) {
            sections.push((section, Vec::new()));
        }
        let Some((_, items)) = sections.last_mut() else {
            continue;
        };
        match items.last_mut() {
            Some(item)
                if row.within.is_some()
                    && item.first().is_some_and(|first| first.within == row.within) =>
            {
                item.push(row);
            }
            _ => items.push(vec![row]),
        }
    }
    html! {
        @for (section, items) in &sections {
            h3 { (section) }
            @for members in items {
                @match members.as_slice() {
                    [only] => (slot_row(shown, found, workout, only, refused)),
                    members => {
                        div.superset {
                            p.badge { "Superset" }
                            @for member in members { (slot_row(shown, found, workout, member, refused)) }
                            @if let Some(rest) = members.first().map(|first| workout.rest.ending(first.slot, first.within.as_ref())).filter(|rest| rest.maximum() != Duration::ZERO) {
                                p.note { "Rest " (rest) " after the round" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What assists and what loads a change, as the page says it.
fn by(change: &FillChange) -> Markup {
    html! {
        @if let Some(by) = &change.assisted_by { "assisted by " (by) }
        @if change.assisted_by.is_some() && change.loaded_by.is_some() { ", " }
        @if let Some(by) = change.loaded_by { "loaded by " (by) }
    }
}

/// What a slot runs: its sets and reps, or its hold, and its rest where it
/// stands on its own.
fn numbers(
    found: &MesocycleInForce,
    workout: &Workout,
    row: &Row<'_>,
    intensity: Relative,
) -> String {
    let rest = |line: String| match row.within {
        Some(_) => line,
        None => format!("{line}, rest {}", workout.rest.ending(row.slot, None)),
    };
    match row.kind {
        Kind::Primary => primary_line(&found.mesocycle, intensity),
        Kind::Static => row.statics.map_or_else(String::new, |fill| {
            rest(format!("{} × {}", fill.sets, fill.reps))
        }),
        Kind::Progressing => {
            let scheme = workout.reps.for_slot(row.slot, row.within.as_ref());
            rest(format!("{} × {}", scheme.sets, scheme.reps))
        }
        Kind::Hold => {
            let holds = match row.held {
                Exercise::Duration(exercise) => exercise.sides().holds(),
                _ => 1,
            };
            let each = if holds > 1 { " a side" } else { "" };
            rest(format!("Hold {}{each}", workout.static_hold))
        }
    }
}

/// The primary's line: what its progression prescribes on this workout.
fn primary_line(mesocycle: &GymMesocycle, intensity: Relative) -> String {
    match mesocycle {
        GymMesocycle::Progression(Progression::Linear(_)) => {
            let session = match intensity {
                Relative::Higher => "heavy",
                Relative::Lower => "light",
            };
            format!("Linear progression, the {session} session")
        }
        GymMesocycle::Progression(Progression::Provided { .. }) => "SBS's chart".to_owned(),
        GymMesocycle::Progression(Progression::BlockPeriodisation(_)) => {
            "Block periodisation".to_owned()
        }
        GymMesocycle::Test(_) => "A test".to_owned(),
        GymMesocycle::Recovery(_) => "The recovery session".to_owned(),
    }
}

fn slot_row(
    shown: &Shown<'_>,
    found: &MesocycleInForce,
    workout: &Workout,
    row: &Row<'_>,
    refused: Option<&Refused<'_>>,
) -> Markup {
    let today = shown.today;
    let current = found.changes.on(row.slot, shown.intensity, today);
    let mine = refused.filter(|refused| refused.typed(row.slot, "slot").is_some());
    let design = refused.filter(|refused| refused.posted("design").is_some());
    let open = mine.is_some() || (row.kind == Kind::Primary && design.is_some());
    html! {
        details.slot open[open] {
            summary {
                p.name { (row.held.name()) }
                @if let Some(change) = current {
                    @if change.assisted_by.is_some() || change.loaded_by.is_some() {
                        p.note { (by(change)) }
                    }
                }
                p.sets { (numbers(found, workout, row, shown.intensity)) }
                @for change in found.changes.iter().filter(|change| {
                    change.slot == row.slot
                        && change.from > today
                        && change.intensity.is_none_or(|of| of == shown.intensity)
                }) {
                    p.note { "From " (change.from) ": " (change.exercise.name()) }
                }
            }
            @match row.kind {
                Kind::Primary => (primary_forms(shown, found, design)),
                _ => (slot_form(shown, workout, row, mine)),
            }
        }
    }
}

/// The primary's progressions, the one this mesocycle runs first.
fn primary_forms(
    shown: &Shown<'_>,
    found: &MesocycleInForce,
    refused: Option<&Refused<'_>>,
) -> Markup {
    let typed = |design: &str| {
        refused
            .map(|refused| progression::Fields::new(refused.typed.to_vec()))
            .filter(|fields| fields.is_for(design))
    };
    let said = |design: &str| {
        typed(design)
            .and(refused)
            .map(|refused| refused.refusal.reason.as_str())
    };
    let linear = html! {
        h4 { "Linear progression" }
        @match &shown.in_force.linear {
            None => p { "None stored. `fitness init` stores the first." },
            Some((_, linear)) => {
                @if let Some(said) = said("linear") { p.reason { (said) } }
                (progression::linear_form(&typed("linear").unwrap_or_else(|| progression::linear_fields(linear))))
            }
        }
    };
    let sbs = html! {
        h4 { "SBS" }
        @match &shown.in_force.sbs {
            None => p { "None stored. `fitness init` stores the first." },
            Some((_, sbs)) => {
                @match &SbsChart::published() {
                    Ok(chart) => (progression::chart_table(chart)),
                    Err(error) => p.failed { (error) },
                }
                @if let Some(said) = said("sbs") { p.reason { (said) } }
                (progression::sbs_form(&typed("sbs").unwrap_or_else(|| progression::sbs_fields(sbs))))
            }
        }
    };
    let sbs_first = matches!(
        found.mesocycle,
        GymMesocycle::Progression(Progression::Provided { .. })
    );
    html! {
        @if sbs_first { (sbs) (linear) } @else { (linear) (sbs) }
    }
}

/// A range of bands as its option posts it: maker and model, apart.
fn assistance(specific: &Specific) -> String {
    format!("{}\u{1f}{}", specific.maker, specific.model)
}

/// What the form offers as it stands today: the ranges of bands, and the
/// kinds that can load an exercise.
fn offered<'a>(shown: &Shown<'a>) -> (Vec<&'a Specific>, Vec<Implement>) {
    let today = shown.today;
    let banded = shown
        .facts
        .bands
        .iter()
        .filter(|(_, timeline)| timeline.on(today).is_some())
        .map(|(specific, _)| specific)
        .collect();
    let loaders = shown
        .facts
        .kinds
        .iter()
        .filter(|(_, timeline)| timeline.on(today).is_some())
        .map(|(implement, _)| *implement)
        .collect();
    (banded, loaders)
}

/// The exercises a slot can hold: held for a time on a mobility slot,
/// counted in reps on any other.
fn exercises(kind: Kind, selected: &str) -> Markup {
    html! {
        @if kind == Kind::Hold {
            @for option in DurationExercise::ALL {
                option value=(option.as_str()) selected[option.as_str() == selected] { (option.name()) }
            }
        } @else {
            @for option in RepsExercise::ALL {
                option value=(option.as_str()) selected[option.as_str() == selected] { (option.name()) }
            }
        }
    }
}

/// The sets and reps a slot's form starts from: a static slot's from the
/// fill it is to hold, any other's from the workout.
fn counts(workout: &Workout, row: &Row<'_>) -> (String, String) {
    if row.kind == Kind::Static {
        return row
            .latest
            .and_then(|change| change.sets.zip(change.reps))
            .or_else(|| row.statics.map(|fill| (fill.sets, fill.reps)))
            .map_or_else(Default::default, |(sets, reps)| {
                (sets.to_string(), reps.to_string())
            });
    }
    let scheme = workout.reps.for_slot(row.slot, row.within.as_ref());
    (scheme.sets.to_string(), progression::reps_text(scheme.reps))
}

/// One slot's form: its exercise, what assists or loads it, its numbers, and
/// the date the exercise changes from.
fn slot_form(
    shown: &Shown<'_>,
    workout: &Workout,
    row: &Row<'_>,
    refused: Option<&Refused<'_>>,
) -> Markup {
    let refusal = refused.map(|refused| refused.refusal);
    let id = |name: &str| format!("{}-{name}", row.slot.as_str());
    let start = row.latest.map_or(row.held, |change| change.exercise);
    let stated = |name: &str, default: String| {
        refused
            .and_then(|refused| refused.typed(row.slot, name))
            .map_or(default, ToOwned::to_owned)
    };
    let exercise = stated("exercise", start.as_str().to_owned());
    let assisted = stated(
        "assisted",
        row.latest
            .and_then(|change| change.assisted_by.as_ref())
            .map(assistance)
            .unwrap_or_default(),
    );
    let loaded = stated(
        "loaded",
        row.latest
            .and_then(|change| change.loaded_by)
            .map(|implement| implement.as_str().to_owned())
            .unwrap_or_default(),
    );
    let (sets, reps) = counts(workout, row);
    let sets = stated("sets", sets);
    let reps = stated("reps", reps);
    let rest = stated(
        "rest",
        progression::rest_text(workout.rest.ending(row.slot, row.within.as_ref())),
    );
    let hold = stated("hold", progression::seconds(workout.static_hold));
    let from = stated("from", shown.today.to_string());
    let other = match shown.intensity {
        Relative::Higher => "lower",
        Relative::Lower => "higher",
    };
    let both = refused.map_or(row.same, |refused| {
        refused.typed(row.slot, "both") == Some("on")
    });
    let (banded, loaders) = offered(shown);
    let field = |name: &'static str, label: &str, control: Markup| {
        input::field(&id(name), name, label, &control, refusal)
    };
    let text = |name: &'static str, value: &str| {
        html! { input id=(id(name)) type="text" name=(name) value=(value); }
    };
    let rest_label = if row.within.is_some() {
        "Rest after the round, seconds"
    } else {
        "Rest, seconds"
    };
    html! {
        form.input method="post" action=(HERE) {
            input type="hidden" name="slot" value=(row.slot.as_str());
            input type="hidden" name="intensity" value=(shown.intensity.as_str());
            (field("exercise", "Exercise", html! {
                select id=(id("exercise")) name="exercise" { (exercises(row.kind, &exercise)) }
            }))
            @if row.kind != Kind::Hold {
                (field("assisted", "Assisted by", html! {
                    select id=(id("assisted")) name="assisted" {
                        option value="" { "unassisted" }
                        @for specific in &banded {
                            @let key = assistance(specific);
                            option value=(key) selected[key == assisted] { (specific) }
                        }
                    }
                }))
                (field("loaded", "Loaded by", html! {
                    select id=(id("loaded")) name="loaded" {
                        option value="" { "not loaded" }
                        @for implement in &loaders {
                            option value=(implement.as_str()) selected[implement.as_str() == loaded] { (implement) }
                        }
                    }
                }))
            }
            (field("from", if row.kind == Kind::Static { "From" } else { "Exercise from" }, html! {
                input id=(id("from")) type="date" name="from" value=(from);
            }))
            @match row.kind {
                Kind::Static | Kind::Progressing => {
                    (field("sets", "Sets", text("sets", &sets)))
                    (field("reps", "Reps", text("reps", &reps)))
                }
                Kind::Hold => {
                    (field("hold", "Hold, seconds, every mobility slot", text("hold", &hold)))
                }
                Kind::Primary => {}
            }
            (field("rest", rest_label, text("rest", &rest)))
            label.check {
                input type="checkbox" name="both" checked[both];
                "The " (other) "-intensity workout too"
            }
            (input::whole(refusal))
            button type="submit" { "Change" }
        }
    }
}

/// A slot's form as it was posted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    pub slot: SlotId,
    pub intensity: Relative,
    pub both: bool,
    pub exercise: Exercise,
    pub assisted_by: Option<Specific>,
    pub loaded_by: Option<Implement>,
    pub from: Date,
    pub sets: Option<RepCount>,
    pub reps: Option<Target<RepCount>>,
    pub rest: Option<Target<Duration>>,
    pub hold: Option<Duration>,
}

/// Read a posted slot's form.
///
/// # Errors
///
/// What is missing or does not read, and the field it is about.
pub fn asked(form: &[(String, String)]) -> Result<Asked, Refusal> {
    let get = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
    };
    let needed = |name: &'static str| {
        get(name).ok_or_else(|| Refusal::of(name, format!("{name} is needed")))
    };
    let optional = |name: &str| get(name).filter(|value| !value.is_empty());
    let slot = SlotId::try_from(needed("slot")?.to_owned())
        .map_err(|error| Refusal::whole(error.to_string()))?;
    let intensity = Relative::try_from(needed("intensity")?.to_owned())
        .map_err(|error| Refusal::whole(error.to_string()))?;
    let exercise = Exercise::named(needed("exercise")?)
        .ok_or_else(|| Refusal::of("exercise", "not an exercise the page offered".to_owned()))?;
    let assisted_by = match optional("assisted") {
        None => None,
        Some(both) => {
            let refused = |error: String| Refusal::of("assisted", error);
            let (maker, model) = both
                .split_once('\u{1f}')
                .ok_or_else(|| refused("the assistance is not one the page offered".to_owned()))?;
            Some(Specific {
                maker: Maker::try_from(maker).map_err(|error| refused(error.to_string()))?,
                model: Model::try_from(model).map_err(|error| refused(error.to_string()))?,
            })
        }
    };
    let loaded_by = optional("loaded")
        .map(|kind| {
            Implement::try_from(kind.to_owned())
                .map_err(|error| Refusal::of("loaded", error.to_string()))
        })
        .transpose()?;
    let typed = needed("from")?;
    let from = typed
        .parse()
        .map_err(|_| Refusal::of("from", format!("{typed:?} is not a date")))?;
    Ok(Asked {
        slot,
        intensity,
        both: get("both") == Some("on"),
        exercise,
        assisted_by,
        loaded_by,
        from,
        sets: optional("sets")
            .map(|typed| {
                progression::reps(typed, "sets").map_err(|error| Refusal::of("sets", error))
            })
            .transpose()?,
        reps: optional("reps")
            .map(|typed| {
                progression::target(typed, "reps", progression::reps)
                    .map_err(|error| Refusal::of("reps", error))
            })
            .transpose()?,
        rest: optional("rest")
            .map(|typed| {
                progression::rest(typed, "rest").map_err(|error| Refusal::of("rest", error))
            })
            .transpose()?,
        hold: optional("hold")
            .map(|typed| {
                progression::duration(typed, "the hold").map_err(|error| Refusal::of("hold", error))
            })
            .transpose()?,
    })
}

/// What a slot's form comes to: a fill change, if the fill differs from what
/// the slot is to hold, and a new version of each workout it reaches that
/// differs from the version in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Writes {
    pub change: Option<FillChange>,
    pub workouts: Vec<(Relative, Workout)>,
}

/// Work out what a posted form changes, against the mesocycle and the
/// workouts in force.
///
/// # Errors
///
/// A static slot's reps typed as a range, since a static slot's reps are
/// one number.
pub fn writes(
    asked: &Asked,
    found: &MesocycleInForce,
    in_force: &InForce,
) -> Result<Writes, Refusal> {
    let primary = found.mesocycle.primary();
    let kind = kind(asked.slot, primary);
    let within = within(primary, asked.slot);
    let intensities: Vec<Relative> = if asked.both {
        vec![Relative::Higher, Relative::Lower]
    } else {
        vec![asked.intensity]
    };
    let statics = match (kind, asked.reps) {
        (Kind::Static, Some(Target::Exactly(reps))) => asked.sets.map(|sets| (sets, reps)),
        (Kind::Static, Some(Target::Range { .. })) => {
            return Err(Refusal::of(
                "reps",
                "a static slot's reps are one number".to_owned(),
            ));
        }
        _ => None,
    };
    // What each intensity is to hold: its latest change, or the mesocycle's
    // own fill where it has none.
    let differs = |intensity: Relative| {
        let latest = found.changes.latest(asked.slot, intensity);
        let (held, fill) = match found.mesocycle.fills().content(asked.slot, role(intensity)) {
            SlotContent::Single(exercise) => (*exercise, None),
            SlotContent::Static(fill) => (fill.exercise, Some((fill.sets, fill.reps))),
        };
        let exercise = latest.map_or(held, |change| change.exercise);
        let counts = latest
            .and_then(|change| change.sets.zip(change.reps))
            .or(fill);
        exercise != asked.exercise
            || latest.and_then(|change| change.assisted_by.as_ref()) != asked.assisted_by.as_ref()
            || latest.and_then(|change| change.loaded_by) != asked.loaded_by
            || (kind == Kind::Static && statics.is_some() && counts != statics)
    };
    let change = intensities
        .iter()
        .any(|intensity| differs(*intensity))
        .then(|| FillChange {
            slot: asked.slot,
            intensity: (!asked.both).then_some(asked.intensity),
            from: asked.from,
            recorded_at: jiff::Timestamp::now(),
            exercise: asked.exercise,
            assisted_by: asked.assisted_by.clone(),
            loaded_by: asked.loaded_by,
            sets: statics.map(|(sets, _)| sets),
            reps: statics.map(|(_, reps)| reps),
        });
    let scheme = match (kind, asked.sets, asked.reps) {
        (Kind::Progressing, Some(sets), Some(reps)) => Some(AccessoryScheme { reps, sets }),
        _ => None,
    };
    let workouts = intensities
        .iter()
        .filter_map(|intensity| {
            let (_, workout) = in_force.workouts.of(*intensity).as_ref()?;
            let mut restated =
                workout
                    .clone()
                    .restated(asked.slot, within.as_ref(), scheme, asked.rest);
            if let (Kind::Hold, Some(hold)) = (kind, asked.hold) {
                restated.static_hold = hold;
            }
            (restated != *workout).then_some((*intensity, restated))
        })
        .collect();
    Ok(Writes { change, workouts })
}

/// A change the fill use case refused, beside the field it is about.
pub fn refusal(error: &FillChangeError) -> Refusal {
    match error {
        FillChangeError::UnknownSpecific { .. } => Refusal::of("assisted", error.to_string()),
        FillChangeError::NotBodyweight { .. } => Refusal::of("loaded", error.to_string()),
        FillChangeError::NoMesocycle { .. } => Refusal::of("from", error.to_string()),
        FillChangeError::NotChangeable { .. }
        | FillChangeError::NotStatic { .. }
        | FillChangeError::Store(_) => Refusal::whole(error.to_string()),
    }
}
