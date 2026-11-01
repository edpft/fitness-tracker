//! The fills page: what fills each slot of the mesocycle in force, and a
//! form to change a fill from a date (#454).
//!
//! **A change names the exercise, what assists it and what loads it.** Only a
//! range of bands is offered as assistance, because a range of bands is the
//! only specific a prescription can step through so far; and only a kind with a
//! scale on the day is offered as what loads it (#579).

use application::{FillChangeError, MesocycleInForce};
use domain::{
    gym::{
        equipment::{EquipmentFacts, Maker, Model, Specific},
        exercise::{Exercise, Implement, RepsExercise},
    },
    planner::{Relative, SessionRole},
    prescription::{FillChange, SlotId, linear::SlotContent},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::input::{self, Refusal};

const TITLE: &str = "Fills";

const HERE: &str = "/fills";

/// The slots a change can reach: not the static ones, which carry their own
/// sets and reps.
const fn changeable(slot: SlotId) -> bool {
    !matches!(slot, SlotId::Plyometric | SlotId::Power)
}

/// A post that was refused: the form as it was posted, and why.
pub struct Refused<'a> {
    pub typed: &'a [(String, String)],
    pub refusal: &'a Refusal,
}

impl Refused<'_> {
    /// What was typed under `name`, if this is the form for that slot and
    /// intensity.
    fn typed(&self, slot: SlotId, intensity: Option<Relative>, name: &str) -> Option<&str> {
        let posted = |key: &str| {
            self.typed
                .iter()
                .find(|(posted, _)| posted == key)
                .map(|(_, value)| value.as_str())
        };
        let same = posted("slot") == Some(slot.as_str())
            && posted("intensity") == intensity.map(Relative::as_str);
        same.then(|| posted(name).unwrap_or_default())
    }
}

/// The page for the mesocycle in force today, with the last post if it was
/// refused.
pub fn page(
    found: Option<&MesocycleInForce>,
    facts: &EquipmentFacts,
    today: Date,
    refused: Option<&Refused<'_>>,
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
    let offered = Offered {
        banded: &banded,
        loaders: &loaders,
        today,
    };
    input::page(
        HERE,
        TITLE,
        &html! {
            @match found {
                None => p { "No mesocycle is in force today." },
                Some(found) => {
                    p.foot { (found.plan) ", mesocycle " (found.ordinal) }
                    (slots(found, &offered, refused))
                }
            }
        },
    )
}

/// What the forms offer, as it stands today.
struct Offered<'a> {
    banded: &'a [&'a Specific],
    loaders: &'a [Implement],
    today: Date,
}

/// The two intensities, with the words the page uses for them.
const SESSIONS: [(Relative, &str); 2] = [
    (Relative::Higher, "Higher-intensity session"),
    (Relative::Lower, "Lower-intensity session"),
];

fn slots(found: &MesocycleInForce, offered: &Offered<'_>, refused: Option<&Refused<'_>>) -> Markup {
    let today = offered.today;
    let fills = found.mesocycle.clone().changed_on(&found.changes, today);
    html! {
        @for slot in SlotId::ALL.iter().copied().filter(|slot| changeable(*slot)) {
            section.card {
                h2 { (slot.as_str()) }
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
                            @if alternates { h3 { (words) } }
                            p.held {
                                (exercise.name())
                                @if let Some(change) = found.changes.on(slot, intensity, today) {
                                    (by(change))
                                    " " span.from { "from " (change.from) }
                                }
                            }
                            @for change in found.changes.iter().filter(|change| {
                                change.slot == slot
                                    && change.from > today
                                    && change.intensity.is_none_or(|of| of == intensity)
                            }) {
                                p.held.from {
                                    "From " (change.from) ": " (change.exercise.name()) (by(change))
                                }
                            }
                            // **The form starts from the latest change**, so a slot
                            // changed from a later date offers what it is about to
                            // hold rather than what it holds today.
                            @let latest = found.changes.latest(slot, intensity);
                            @let start = latest.map_or(exercise, |change| change.exercise);
                            @if let Exercise::Reps(current) = start {
                                @let asked = Form {
                                    slot,
                                    intensity: alternates.then_some(intensity),
                                };
                                (form(&asked, &asked.shown(current, latest, today, refused), offered, refused))
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
        @if let Some(by) = &change.assisted_by { " assisted by " (by) }
        @if let Some(by) = change.loaded_by { " loaded by " (by) }
    }
}

/// Which form: a slot, and an intensity where the slot alternates.
struct Form {
    slot: SlotId,
    intensity: Option<Relative>,
}

/// What a form shows in each field, as the keys its options post.
struct Shown {
    exercise: String,
    assisted: String,
    loaded: String,
    from: String,
}

impl Form {
    fn id(&self, name: &str) -> String {
        let slot = self.slot.as_str();
        self.intensity.map_or_else(
            || format!("{slot}-{name}"),
            |intensity| format!("{slot}-{}-{name}", intensity.as_str()),
        )
    }

    /// What was typed, if the last post was this form's and was refused;
    /// otherwise the latest change, from today.
    fn shown(
        &self,
        current: RepsExercise,
        latest: Option<&FillChange>,
        today: Date,
        refused: Option<&Refused<'_>>,
    ) -> Shown {
        let typed =
            |name: &str| refused.and_then(|refused| refused.typed(self.slot, self.intensity, name));
        if typed("from").is_some() {
            return Shown {
                exercise: typed("exercise").unwrap_or_default().to_owned(),
                assisted: typed("assisted").unwrap_or_default().to_owned(),
                loaded: typed("loaded").unwrap_or_default().to_owned(),
                from: typed("from").unwrap_or_default().to_owned(),
            };
        }
        Shown {
            exercise: current.as_str().to_owned(),
            assisted: latest
                .and_then(|change| change.assisted_by.as_ref())
                .map(assistance)
                .unwrap_or_default(),
            loaded: latest
                .and_then(|change| change.loaded_by)
                .map(|implement| implement.as_str().to_owned())
                .unwrap_or_default(),
            from: today.to_string(),
        }
    }

    /// The last post's refusal, if it was this form's.
    fn refused<'a>(&self, refused: Option<&'a Refused<'_>>) -> Option<&'a Refusal> {
        refused
            .filter(|refused| refused.typed(self.slot, self.intensity, "slot").is_some())
            .map(|refused| refused.refusal)
    }
}

/// A range of bands as its option posts it: maker and model, apart.
fn assistance(specific: &Specific) -> String {
    format!("{}\u{1f}{}", specific.maker, specific.model)
}

fn form(
    asked: &Form,
    shown: &Shown,
    offered: &Offered<'_>,
    refused: Option<&Refused<'_>>,
) -> Markup {
    let refused = asked.refused(refused);
    input::folded(
        "Change",
        refused,
        &html! {
            form.input method="post" action=(HERE) {
                input type="hidden" name="slot" value=(asked.slot.as_str());
                @if let Some(intensity) = asked.intensity {
                    input type="hidden" name="intensity" value=(intensity.as_str());
                }
                (input::field(&asked.id("exercise"), "exercise", "Exercise", &html! {
                    select id=(asked.id("exercise")) name="exercise" {
                        @for exercise in RepsExercise::ALL {
                            option value=(exercise.as_str()) selected[exercise.as_str() == shown.exercise] { (exercise.name()) }
                        }
                    }
                }, refused))
                (input::field(&asked.id("assisted"), "assisted", "Assisted by", &html! {
                    select id=(asked.id("assisted")) name="assisted" {
                        option value="" { "unassisted" }
                        @for specific in offered.banded {
                            @let key = assistance(specific);
                            option value=(key) selected[key == shown.assisted] { (specific) }
                        }
                    }
                }, refused))
                (input::field(&asked.id("loaded"), "loaded", "Loaded by", &html! {
                    select id=(asked.id("loaded")) name="loaded" {
                        option value="" { "not loaded" }
                        @for implement in offered.loaders {
                            option value=(implement.as_str()) selected[implement.as_str() == shown.loaded] { (implement) }
                        }
                    }
                }, refused))
                (input::field(&asked.id("from"), "from", "From", &html! {
                    input id=(asked.id("from")) type="date" name="from" value=(shown.from);
                }, refused))
                (input::whole(refused))
                button type="submit" { "Change from" }
            }
        },
    )
}

/// Read a posted change, recorded now.
///
/// # Errors
///
/// What is missing or does not read, and the field it is about.
pub fn asked(form: &[(String, String)]) -> Result<FillChange, Refusal> {
    let field = |name: &'static str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .ok_or_else(|| Refusal::of(name, format!("{name} is needed")))
    };
    let slot = SlotId::try_from(field("slot")?.to_owned())
        .map_err(|error| Refusal::whole(error.to_string()))?;
    let intensity = field("intensity")
        .ok()
        .map(|typed| {
            Relative::try_from(typed.to_owned()).map_err(|error| Refusal::whole(error.to_string()))
        })
        .transpose()?;
    let exercise = RepsExercise::try_from(field("exercise")?.to_owned())
        .map(Exercise::Reps)
        .map_err(|error| Refusal::of("exercise", error.to_string()))?;
    let assisted_by = match field("assisted")? {
        "" => None,
        both => {
            let refused = |error| Refusal::of("assisted", error);
            let (maker, model) = both
                .split_once('\u{1f}')
                .ok_or_else(|| refused("the assistance is not one the page offered".to_owned()))?;
            Some(Specific {
                maker: Maker::try_from(maker).map_err(|error| refused(error.to_string()))?,
                model: Model::try_from(model).map_err(|error| refused(error.to_string()))?,
            })
        }
    };
    let loaded_by = match field("loaded")? {
        "" => None,
        kind => Some(
            Implement::try_from(kind.to_owned())
                .map_err(|error| Refusal::of("loaded", error.to_string()))?,
        ),
    };
    let typed = field("from")?;
    let from = typed
        .parse()
        .map_err(|_| Refusal::of("from", format!("{typed:?} is not a date")))?;
    Ok(FillChange {
        slot,
        intensity,
        from,
        recorded_at: jiff::Timestamp::now(),
        exercise,
        assisted_by,
        loaded_by,
    })
}

/// A change the use case refused, beside the field it is about.
pub fn refusal(error: &FillChangeError) -> Refusal {
    match error {
        FillChangeError::UnknownSpecific { .. } => Refusal::of("assisted", error.to_string()),
        FillChangeError::NotBodyweight { .. } => Refusal::of("loaded", error.to_string()),
        FillChangeError::NoMesocycle { .. } => Refusal::of("from", error.to_string()),
        FillChangeError::NotChangeable { .. } | FillChangeError::Store(_) => {
            Refusal::whole(error.to_string())
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    input::unreadable(HERE, TITLE, error)
}

#[cfg(test)]
mod tests {
    use application::FillChangeError;
    use domain::{
        gym::exercise::{Implement, RepsExercise},
        prescription::SlotId,
    };
    use jiff::civil::date;

    use super::{Form, Offered, Refused, form, refusal};

    fn posted(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// **A refused change comes back as it was typed**, with the reason under
    /// the field it is about (#665), not the fill the slot holds.
    #[test]
    fn a_refused_change_keeps_what_was_typed() {
        let typed = posted(&[
            ("slot", "knee_dominant"),
            ("exercise", "front-squat-barbell"),
            ("assisted", ""),
            ("loaded", "dumbbell"),
            ("from", "2026-11-09"),
        ]);
        let refused = refusal(&FillChangeError::NotBodyweight {
            exercise: "Front Squat (Barbell)",
        });
        let refused = Refused {
            typed: &typed,
            refusal: &refused,
        };
        let offered = Offered {
            banded: &[],
            loaders: &[Implement::Barbell, Implement::Dumbbell],
            today: date(2026, 11, 1),
        };
        let asked = Form {
            slot: SlotId::KneeDominant,
            intensity: None,
        };
        let held = RepsExercise::try_from("back-squat-barbell").expect("a known exercise");

        let shown = asked.shown(held, None, offered.today, Some(&refused));
        let page = form(&asked, &shown, &offered, Some(&refused)).into_string();

        assert!(
            page.contains(r#"value="front-squat-barbell" selected"#),
            "{page}"
        );
        assert!(page.contains(r#"value="dumbbell" selected"#), "{page}");
        assert!(page.contains(r#"value="2026-11-09""#), "{page}");
        let loaded = page.find("Loaded by").expect("the loaded field");
        let reason = page
            .find("loaded on its own implement")
            .expect("the reason");
        let from = page.find(">From<").expect("the from field");
        assert!(loaded < reason && reason < from, "{page}");
    }

    /// **Another slot's form is untouched** by a refusal that was not its.
    #[test]
    fn a_refusal_stays_with_its_own_form() {
        let typed = posted(&[("slot", "hip_dominant"), ("from", "nonsense")]);
        let refused = super::Refusal::of("from", "\"nonsense\" is not a date".to_owned());
        let refused = Refused {
            typed: &typed,
            refusal: &refused,
        };
        let offered = Offered {
            banded: &[],
            loaders: &[],
            today: date(2026, 11, 1),
        };
        let asked = Form {
            slot: SlotId::KneeDominant,
            intensity: None,
        };
        let held = RepsExercise::try_from("back-squat-barbell").expect("a known exercise");

        let shown = asked.shown(held, None, offered.today, Some(&refused));
        let page = form(&asked, &shown, &offered, Some(&refused)).into_string();

        assert!(
            page.contains(r#"value="back-squat-barbell" selected"#),
            "{page}"
        );
        assert!(page.contains(r#"value="2026-11-01""#), "{page}");
        assert!(!page.contains("refused"), "{page}");
        assert!(!page.contains(" open"), "{page}");
    }
}
