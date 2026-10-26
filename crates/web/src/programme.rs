//! The programme page: what fills each slot of the mesocycle in force, and a
//! form to change a fill from a date (#454).
//!
//! **A change names the exercise, what assists it and what loads it.** Only a
//! range of bands is offered as assistance, because a range of bands is the
//! only specific a prescription can step through so far; and only a kind with a
//! scale on the day is offered as what loads it (#579).

use application::MesocycleInForce;
use domain::{
    gym::{
        equipment::{EquipmentFacts, Maker, Model, Specific},
        exercise::{Exercise, Implement, RepsExercise},
    },
    prescription::{FillChange, SlotId, linear::SlotContent},
    schedule::{Relative, SessionRole},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Programme";

const STYLE: &str = "
form.fill { display: grid; gap: 0.4rem; margin: 0.25rem 0 1.25rem; }
form.fill select, form.fill input { font: inherit; padding: 6px; max-width: 100%; box-sizing: border-box; }
.row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
.row > * { flex: 1 1 10rem; min-width: 0; }
.row button { flex: 0 0 auto; padding: 8px 12px; }
.change { color: var(--muted); }
.said { color: var(--failed); }
";

/// The slots a change can reach: not the static ones, which carry their own
/// sets and reps.
const fn changeable(slot: SlotId) -> bool {
    !matches!(slot, SlotId::Plyometric | SlotId::Power)
}

/// The page for the mesocycle in force today.
pub fn page(
    found: Option<&MesocycleInForce>,
    facts: &EquipmentFacts,
    today: Date,
    said: Option<&str>,
) -> Markup {
    let banded: Vec<&Specific> = facts
        .bands
        .iter()
        .filter(|(_, timeline)| timeline.on(today).is_some())
        .map(|(specific, _)| specific)
        .collect();
    let loaders: Vec<Implement> = facts
        .kinds
        .iter()
        .filter(|(_, timeline)| timeline.on(today).is_some())
        .map(|(implement, _)| *implement)
        .collect();
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (maud::PreEscaped(STYLE)) }
            }
            body {
                main {
                    (crate::page::menu("/programme"))
                    h1 { (TITLE) }
                    @if let Some(said) = said { p.said { (said) } }
                    @match found {
                        None => p { "No mesocycle is in force today." },
                        Some(found) => {
                            p.legend {
                                (found.plan) ", mesocycle " (found.ordinal) ". "
                                "A change holds from its date; a session prescribed before it keeps what it was given."
                            }
                            (slots(found, &banded, &loaders, today))
                        }
                    }
                }
            }
        }
    }
}

/// The two intensities, with the words the page uses for them.
const SESSIONS: [(Relative, &str); 2] = [
    (Relative::Higher, "Higher-intensity session"),
    (Relative::Lower, "Lower-intensity session"),
];

fn slots(
    found: &MesocycleInForce,
    banded: &[&Specific],
    loaders: &[Implement],
    today: Date,
) -> Markup {
    let fills = found.mesocycle.clone().changed_on(&found.changes, today);
    html! {
        @for slot in SlotId::ALL.iter().copied().filter(|slot| changeable(*slot)) {
            h3 { (slot.as_str()) }
            @let held = |intensity: Relative| {
                SessionRole::ALL
                    .iter()
                    .find(|role| role.intensity() == intensity)
                    .and_then(|role| match fills.fills().content(slot, *role) {
                        SlotContent::Single(exercise) => Some(*exercise),
                        SlotContent::Static(_) => None,
                    })
            };
            @let alternates = held(Relative::Higher) != held(Relative::Lower);
            @for (intensity, words) in SESSIONS {
                @if alternates || intensity == Relative::Higher {
                    @if let Some(exercise) = held(intensity) {
                        p {
                            @if alternates { (words) ": " }
                            (exercise.name())
                            @if let Some(change) = found.changes.on(slot, intensity, today) {
                                @if let Some(by) = &change.assisted_by { " assisted by " (by) }
                                @if let Some(by) = change.loaded_by { " loaded by " (by) }
                                " " span.change { "(from " (change.from) ")" }
                            }
                        }
                        // **The form starts from the latest change**, so a slot
                        // changed from a later date offers what it is about to
                        // hold rather than what it holds today.
                        @let latest = found.changes.latest(slot, intensity);
                        @let start = latest.map_or(exercise, |change| change.exercise);
                        @if let Exercise::Reps(current) = start {
                            (form(slot, alternates.then_some(intensity), current, latest, banded, loaders, today))
                        }
                    }
                }
            }
            @for change in found.changes.iter().filter(|change| change.slot == slot && change.from > today) {
                p.change {
                    "From " (change.from) ": " (change.exercise.name())
                    @if let Some(by) = &change.assisted_by { " assisted by " (by) }
                    @if let Some(by) = change.loaded_by { " loaded by " (by) }
                    @if let Some(intensity) = change.intensity { ", " (intensity.as_str()) "-intensity session" }
                }
            }
        }
    }
}

fn form(
    slot: SlotId,
    intensity: Option<Relative>,
    current: RepsExercise,
    latest: Option<&FillChange>,
    banded: &[&Specific],
    loaders: &[Implement],
    today: Date,
) -> Markup {
    let assisted = latest.and_then(|change| change.assisted_by.as_ref());
    let loaded = latest.and_then(|change| change.loaded_by);
    html! {
        form.fill method="post" action="/programme" {
            input type="hidden" name="slot" value=(slot.as_str());
            @if let Some(intensity) = intensity {
                input type="hidden" name="intensity" value=(intensity.as_str());
            }
            div.row {
                select name="exercise" aria-label="Exercise" {
                    @for exercise in RepsExercise::ALL {
                        option value=(exercise.as_str()) selected[*exercise == current] { (exercise.name()) }
                    }
                }
                select name="assisted" aria-label="Assisted by" {
                    option value="" { "unassisted" }
                    @for specific in banded {
                        option value=(format!("{}\u{1f}{}", specific.maker, specific.model)) selected[assisted == Some(*specific)] { "assisted by " (specific) }
                    }
                }
                select name="loaded" aria-label="Loaded by" {
                    option value="" { "not loaded" }
                    @for implement in loaders {
                        option value=(implement.as_str()) selected[loaded == Some(*implement)] { "loaded by " (implement) }
                    }
                }
            }
            div.row {
                input type="date" name="from" value=(today) aria-label="From";
                button type="submit" { "Change from" }
            }
        }
    }
}

/// What a posted change asked for.
pub struct Asked {
    pub slot: SlotId,
    /// The session it changes, for a slot that alternates; both otherwise.
    pub intensity: Option<Relative>,
    pub exercise: Exercise,
    pub assisted_by: Option<Specific>,
    pub loaded_by: Option<Implement>,
    pub from: Date,
}

/// Read a posted change.
///
/// # Errors
///
/// What is missing or does not read, in words for the page.
pub fn asked(form: &[(String, String)]) -> Result<Asked, String> {
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .ok_or_else(|| format!("{name} is needed"))
    };
    let slot = SlotId::try_from(field("slot")?.to_owned()).map_err(|error| error.to_string())?;
    let intensity = field("intensity")
        .ok()
        .map(|typed| Relative::try_from(typed.to_owned()).map_err(|error| error.to_string()))
        .transpose()?;
    let exercise = RepsExercise::try_from(field("exercise")?.to_owned())
        .map(Exercise::Reps)
        .map_err(|error| error.to_string())?;
    let assisted_by = match field("assisted")? {
        "" => None,
        both => {
            let (maker, model) = both
                .split_once('\u{1f}')
                .ok_or_else(|| "the assistance is not one the page offered".to_owned())?;
            Some(Specific {
                maker: Maker::try_from(maker).map_err(|error| error.to_string())?,
                model: Model::try_from(model).map_err(|error| error.to_string())?,
            })
        }
    };
    let loaded_by = match field("loaded")? {
        "" => None,
        kind => Some(Implement::try_from(kind.to_owned()).map_err(|error| error.to_string())?),
    };
    let typed = field("from")?;
    let from = typed
        .parse()
        .map_err(|_| format!("{typed:?} is not a date"))?;
    Ok(Asked {
        slot,
        intensity,
        exercise,
        assisted_by,
        loaded_by,
        from,
    })
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) }
            }
            body {
                main {
                    (crate::page::menu("/programme"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}
