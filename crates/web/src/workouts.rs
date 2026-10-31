//! The workouts page: each workout and each primary progression in force, and a
//! form to author the next version of each (#192).
//!
//! **Each form is one whole version**, because a version is one statement and
//! the store keeps it as one. The forms start from the version in force, and a
//! refused post starts from what was typed, so a typo does not cost the rest.
//!
//! **SBS's chart is shown and not offered**: it is SBS's (§ 14.1), and what is
//! authored for SBS is only what it leaves out, its ramp and its rest.
//!
//! **One encoding, both ways.** [`workout_fields`] and its siblings write a
//! version as the fields its form posts, and [`asked`] reads them back. The page
//! renders from fields whichever way they came, and the test that pins the two
//! is that the shipped versions survive the round trip.

use std::collections::BTreeMap;

use application::design::InForce;
use domain::{
    measure::{Duration, Kg, RepCount, Spans},
    normalised::OperatorZone,
    prescription::{
        AccessoryScheme, BackOff, ByIntensity, Levels, LinearProgression, LinearRest, Percentage,
        ResetProtocol, Rest, RestScheme, SbsChart, SbsDay, SbsProgression, SbsRest, Section,
        SlotId, Superset, Target, TopSetReps, WarmupStep, Workout,
    },
    schedule::Relative,
    sequence::NonEmpty,
};
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Workouts";

const STYLE: &str = "
form.design { display: grid; gap: 0.5rem; margin: 0.5rem 0 1.5rem; }
form.design input, form.design select { font: inherit; padding: 6px; width: 100%; min-width: 0; box-sizing: border-box; }
form.design fieldset { min-width: 0; display: grid; gap: 0.4rem; }
form.design label { display: grid; gap: 0.15rem; }
.pair { display: grid; grid-template-columns: repeat(auto-fit, minmax(7rem, 1fr)); gap: 0.5rem; }
.since { color: var(--muted); }
.said { color: var(--failed); }
.rows { display: grid; gap: 0.4rem; }
.rows > .row { display: grid; grid-template-columns: minmax(0, 2fr) repeat(2, minmax(0, 1fr)) auto; gap: 0.4rem; align-items: end; }
.rows.ramp > .row { grid-template-columns: repeat(2, minmax(0, 1fr)) auto; }
.rows .remove, .add { width: auto; padding: 6px 10px; justify-self: start; }
table.chart td { padding-right: 1rem; }
";

/// Add a row by copying the last one emptied, remove one by taking it out.
const SCRIPT: &str = "
function addRow(button) {
  const rows = button.previousElementSibling;
  const row = rows.lastElementChild.cloneNode(true);
  row.querySelectorAll('input').forEach((input) => { input.value = ''; });
  row.querySelectorAll('select').forEach((select) => { select.selectedIndex = 0; });
  rows.appendChild(row);
}
function removeRow(button) {
  const rows = button.closest('.rows');
  if (rows.children.length > 1) { button.closest('.row').remove(); }
}
";

/// What a posted form asked to author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Workout(Relative, Workout),
    Linear(LinearProgression),
    Sbs(SbsProgression),
}

/// A posted form, or a version written as one.
#[derive(Debug, Clone, Default)]
pub struct Fields(Vec<(String, String)>);

impl Fields {
    pub const fn new(fields: Vec<(String, String)>) -> Self {
        Self(fields)
    }

    fn put(&mut self, name: &str, value: &impl ToString) {
        self.0.push((name.to_owned(), value.to_string()));
    }

    /// The first value under `name`, trimmed, or empty.
    fn get(&self, name: &str) -> &str {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value.trim())
    }

    /// Every value under `name`, in the order posted.
    fn all(&self, name: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .collect()
    }

    fn needed(&self, name: &str, what: &str) -> Result<&str, String> {
        match self.get(name) {
            "" => Err(format!("{what} is needed")),
            value => Ok(value),
        }
    }

    /// Whether this is the form for `design`, and for `intensity` where it
    /// names one.
    fn is_for(&self, design: &str, intensity: Option<Relative>) -> bool {
        self.get("design") == design
            && intensity.is_none_or(|intensity| self.get("intensity") == intensity.as_str())
    }
}

/// The two workouts, with the words the page uses for them.
const WORKOUTS: [(Relative, &str); 2] = [
    (Relative::Higher, "Higher-intensity workout"),
    (Relative::Lower, "Lower-intensity workout"),
];

/// The page, with what the last post came to if there was one: the reason it
/// was refused, and what was typed into the form refused.
pub fn page(
    in_force: &InForce,
    zone: &OperatorZone,
    said: Option<&str>,
    typed: Option<&Fields>,
) -> Markup {
    let since = |at: &jiff::Timestamp| {
        at.to_zoned(zone.as_time_zone())
            .strftime("%Y-%m-%d %H:%M")
            .to_string()
    };
    let typed_as = |design: &str, intensity: Option<Relative>| {
        typed
            .filter(|fields| fields.is_for(design, intensity))
            .cloned()
    };
    let chart = SbsChart::published();
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (crate::page::head(TITLE, &[STYLE]))
                script { (maud::PreEscaped(SCRIPT)) }
            }
            body {
                main {
                    (crate::page::menu("/workouts"))
                    h1 { (TITLE) }
                    @if let Some(said) = said { p.said { (said) } }
                    p.legend {
                        "Saving a form stores a new version from now. A plan already made keeps "
                        "the versions it was planned with; the next plan takes these."
                    }
                    @for (intensity, words) in WORKOUTS {
                        h2 { (words) }
                        @match in_force.workouts.of(intensity) {
                            None => p { "None stored. `fitness init` stores the first." },
                            Some((at, workout)) => {
                                p.since { "Since " (since(at)) }
                                (workout_form(intensity, &typed_as("workout", Some(intensity)).unwrap_or_else(|| workout_fields(intensity, workout))))
                            }
                        }
                    }
                    h2 { "Linear progression" }
                    @match &in_force.linear {
                        None => p { "None stored. `fitness init` stores the first." },
                        Some((at, linear)) => {
                            p.since { "Since " (since(at)) }
                            (linear_form(&typed_as("linear", None).unwrap_or_else(|| linear_fields(linear))))
                        }
                    }
                    h2 { "SBS" }
                    @match &in_force.sbs {
                        None => p { "None stored. `fitness init` stores the first." },
                        Some((at, sbs)) => {
                            p.since { "Since " (since(at)) }
                            @match &chart {
                                Ok(chart) => (chart_table(chart)),
                                Err(error) => p.failed { (error) },
                            }
                            (sbs_form(&typed_as("sbs", None).unwrap_or_else(|| sbs_fields(sbs))))
                        }
                    }
                }
            }
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (crate::page::head(TITLE, &[]))
            }
            body {
                main {
                    (crate::page::menu("/workouts"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// Where in a workout a value is stated, narrower than the workout itself.
#[derive(Debug, Clone, PartialEq, Eq)]
enum At {
    Section(Section),
    Superset(Superset),
    Exercise(SlotId),
}

impl At {
    /// The key a form posts it under: `section:strength`, `superset:biceps+triceps`,
    /// `exercise:wrist_flexion`.
    fn key(&self) -> String {
        match self {
            Self::Section(section) => format!("section:{section}"),
            Self::Superset(superset) => format!("superset:{superset}"),
            Self::Exercise(slot) => format!("exercise:{slot}"),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Section(section) => format!("the {section} section"),
            Self::Superset(superset) => format!("the {superset} superset"),
            Self::Exercise(slot) => format!("the {slot} exercise"),
        }
    }

    fn read(key: &str) -> Result<Self, String> {
        let unknown = || format!("{key:?} is not a place in a workout");
        let (level, name) = key.split_once(':').ok_or_else(unknown)?;
        match level {
            "section" => Section::try_from(name.to_owned())
                .map(Self::Section)
                .map_err(|error| error.to_string()),
            "superset" => Superset::try_from(name.to_owned())
                .map(Self::Superset)
                .map_err(|error| error.to_string()),
            "exercise" => SlotId::try_from(name.to_owned())
                .map(Self::Exercise)
                .map_err(|error| error.to_string()),
            _ => Err(unknown()),
        }
    }
}

/// Whether a slot runs double progression, and so has reps to state.
const fn progresses(slot: SlotId) -> bool {
    matches!(slot.section(), Section::Strength | Section::Hypertrophy)
}

/// Every place reps can be stated: the progressing sections, the supersets all
/// of whose members progress, and the progressing exercises.
fn rep_places() -> Vec<At> {
    let sections = [Section::Strength, Section::Hypertrophy].map(At::Section);
    let supersets = Superset::issued()
        .into_iter()
        .filter(|superset| superset.slots().iter().all(|slot| progresses(*slot)))
        .map(At::Superset);
    let exercises = SlotId::ALL
        .iter()
        .copied()
        .filter(|slot| progresses(*slot))
        .map(At::Exercise);
    sections
        .into_iter()
        .chain(supersets)
        .chain(exercises)
        .collect()
}

/// Every place rest can be stated. The primary's is its progression's, so an
/// exercise's rest reaches the lower slots only when they are not the primary.
fn rest_places() -> Vec<At> {
    let sections = Section::ALL.map(At::Section);
    let supersets = Superset::issued().into_iter().map(At::Superset);
    let exercises = SlotId::ALL.iter().copied().map(At::Exercise);
    sections
        .into_iter()
        .chain(supersets)
        .chain(exercises)
        .collect()
}

fn input(fields: &Fields, name: &str, label: &str, hint: &str) -> Markup {
    html! {
        label {
            span { (label) }
            input type="text" name=(name) value=(fields.get(name)) placeholder=(hint);
        }
    }
}

/// Rows of a value stated somewhere narrower than the workout, with one blank
/// row when there are none.
fn place_rows(
    places: &[At],
    name: &str,
    stated: &[(&str, &str, &str)],
    labels: (&str, &str),
    hints: (&str, &str),
) -> Markup {
    let blank = [("", "", "")];
    let rows: &[(&str, &str, &str)] = if stated.is_empty() { &blank } else { stated };
    html! {
        div.rows {
            @for (at, first, second) in rows {
                div.row {
                    label {
                        span { "For" }
                        select name=(name) {
                            option value="" { "nothing" }
                            @for place in places {
                                @let key = place.key();
                                option value=(key) selected[key == *at] { (place.label()) }
                            }
                        }
                    }
                    label { span { (labels.0) } input type="text" name=(format!("{name}_{}", "first")) value=(first) placeholder=(hints.0); }
                    label { span { (labels.1) } input type="text" name=(format!("{name}_{}", "second")) value=(second) placeholder=(hints.1); }
                    button.remove type="button" onclick="removeRow(this)" { "Remove" }
                }
            }
        }
    }
}

/// The rows posted under `name`: where, and the two values beside it.
fn rows_of<'a>(fields: &'a Fields, name: &str) -> Vec<(&'a str, &'a str, &'a str)> {
    fields
        .all(name)
        .into_iter()
        .zip(fields.all(&format!("{name}_first")))
        .zip(fields.all(&format!("{name}_second")))
        .map(|((at, first), second)| (at, first, second))
        .collect()
}

fn workout_form(intensity: Relative, fields: &Fields) -> Markup {
    let reps = rows_of(fields, "reps_at");
    let rest = rows_of(fields, "rest_at");
    html! {
        form.design method="post" action="/workouts" {
            input type="hidden" name="design" value="workout";
            input type="hidden" name="intensity" value=(intensity.as_str());
            fieldset {
                legend { "Reps, for every strength and hypertrophy slot but the primary" }
                div.pair {
                    (input(fields, "reps_sets", "Sets", "3"))
                    (input(fields, "reps_reps", "Reps", "4-6"))
                }
                p.legend { "Narrower, where it differs. The narrowest stated stands." }
                (place_rows(&rep_places(), "reps_at", &reps, ("Sets", "Reps"), ("3", "8-12")))
                button.add type="button" onclick="addRow(this)" { "Add" }
            }
            fieldset {
                legend { "Rest, in seconds, after every set but the primary's" }
                div.pair {
                    (input(fields, "rest_between", "Between sets", "120-180"))
                    (input(fields, "rest_superset", "After a superset", "as between sets"))
                }
                p.legend {
                    "Narrower, where it differs. A superset or an exercise states one rest: "
                    "after its round, or between its sets."
                }
                (place_rows(&rest_places(), "rest_at", &rest, ("Rest", "After a superset"), ("90", "sections only")))
                button.add type="button" onclick="addRow(this)" { "Add" }
            }
            (input(fields, "static_hold", "A static hold, in seconds", "60"))
            button type="submit" { "Save this workout" }
        }
    }
}

fn ramp(fields: &Fields) -> Markup {
    let steps: Vec<(&str, &str)> = fields
        .all("ramp_share")
        .into_iter()
        .zip(fields.all("ramp_reps"))
        .collect();
    let steps = if steps.is_empty() {
        vec![("", "")]
    } else {
        steps
    };
    html! {
        fieldset {
            legend { "Warm-up ramp, of the top set" }
            div.rows.ramp {
                @for (share, reps) in steps {
                    div.row {
                        label { span { "Of the top set" } input type="text" name="ramp_share" value=(share) placeholder="40%"; }
                        label { span { "Reps" } input type="text" name="ramp_reps" value=(reps) placeholder="4"; }
                        button.remove type="button" onclick="removeRow(this)" { "Remove" }
                    }
                }
            }
            button.add type="button" onclick="addRow(this)" { "Add a step" }
        }
    }
}

fn linear_form(fields: &Fields) -> Markup {
    html! {
        form.design method="post" action="/workouts" {
            input type="hidden" name="design" value="linear";
            (ramp(fields))
            @for (key, words) in [("heavy", "Heavy session"), ("light", "Light session")] {
                fieldset {
                    legend { (words) }
                    div.pair {
                        (input(fields, &format!("{key}_top_reps"), "Top set reps", "3"))
                        (input(fields, &format!("{key}_back_off_sets"), "Back-off sets", "2"))
                        (input(fields, &format!("{key}_back_off_reps"), "Back-off reps", "4"))
                        (input(fields, &format!("{key}_back_off_share"), "Back-off, of the top set", "85%"))
                    }
                }
            }
            (input(fields, "light_of_heavy", "The light session's top set, of the heavy load", "85%"))
            (input(fields, "climb", "Climb a week, kg", "2.5"))
            (input(fields, "entry_drop", "A derived opening drops, off a failed entry test", "-10%"))
            @for (key, words) in [("first_reset", "First stall"), ("second_reset", "Second stall")] {
                fieldset {
                    legend { (words) }
                    div.pair {
                        (input(fields, &format!("{key}_drop"), "Drop", "-10%"))
                        (input(fields, &format!("{key}_reclimb"), "Re-climb a week, kg", "2.5"))
                    }
                }
            }
            fieldset {
                legend { "Rest, in seconds" }
                div.pair {
                    (input(fields, "rest_top_set", "After a top set", "180-300"))
                    (input(fields, "rest_back_off", "After a back-off", "120-180"))
                    (input(fields, "rest_attempt", "Between attempts", "180-300"))
                }
            }
            button type="submit" { "Save linear progression" }
        }
    }
}

fn chart_table(chart: &SbsChart) -> Markup {
    html! {
        p.legend { "SBS's chart, as it publishes it, with your week 4 first session." }
        table.chart {
            @for (week, session, day) in chart.days() {
                tr {
                    td { "Week " (week) ", " (session) }
                    td {
                        @match day {
                            SbsDay::Percentage { sets, reps, share } => { (sets) " × " (reps) " @ " (share) }
                            SbsDay::RepMax { reps, back_off_sets, back_off_reps } => {
                                (reps) "RM, then " (back_off_sets) " × " (back_off_reps)
                            }
                            SbsDay::Test { reps } => { "a test at " (reps) }
                        }
                    }
                }
            }
        }
    }
}

fn sbs_form(fields: &Fields) -> Markup {
    html! {
        form.design method="post" action="/workouts" {
            input type="hidden" name="design" value="sbs";
            (ramp(fields))
            fieldset {
                legend { "Rest, in seconds" }
                div.pair {
                    (input(fields, "rest_across", "Between sets across", "120-180"))
                    (input(fields, "rest_rep_max", "After a repetition maximum", "180-300"))
                    (input(fields, "rest_back_off", "After a back-off", "120-180"))
                    (input(fields, "rest_attempt", "Between attempts", "180-300"))
                }
            }
            button type="submit" { "Save SBS" }
        }
    }
}

/// A target as typed: `5`, or `4-6` for a range.
fn target_text<M: Spans>(target: Target<M>, one: impl Fn(M) -> String) -> String {
    match target {
        Target::Exactly(measure) => one(measure),
        Target::Range { minimum, extent } => {
            format!("{}-{}", one(minimum), one(minimum.spanning(extent)))
        }
    }
}

fn seconds(duration: Duration) -> String {
    duration.as_seconds().to_string()
}

fn rest_text(rest: Target<Duration>) -> String {
    target_text(rest, seconds)
}

fn reps_text(reps: Target<RepCount>) -> String {
    target_text(reps, |count| count.to_string())
}

fn put_ramp(fields: &mut Fields, steps: &NonEmpty<WarmupStep>) {
    for step in steps.iter() {
        fields.put("ramp_share", &step.of_top_set);
        fields.put("ramp_reps", &step.reps);
    }
}

/// A workout, as its form posts it.
pub fn workout_fields(intensity: Relative, workout: &Workout) -> Fields {
    let mut fields = Fields::default();
    fields.put("design", &"workout");
    fields.put("intensity", &intensity.as_str());
    fields.put("reps_sets", &workout.reps.workout.sets);
    fields.put("reps_reps", &reps_text(workout.reps.workout.reps));
    let mut put_reps = |at: At, scheme: &AccessoryScheme| {
        fields.put("reps_at", &at.key());
        fields.put("reps_at_first", &scheme.sets);
        fields.put("reps_at_second", &reps_text(scheme.reps));
    };
    for (section, scheme) in &workout.reps.sections {
        put_reps(At::Section(*section), scheme);
    }
    for (superset, scheme) in &workout.reps.supersets {
        put_reps(At::Superset(superset.clone()), scheme);
    }
    for (slot, scheme) in &workout.reps.exercises {
        put_reps(At::Exercise(*slot), scheme);
    }
    let after = |rest: Rest| rest.after_superset.map_or_else(String::new, rest_text);
    fields.put(
        "rest_between",
        &rest_text(workout.rest.workout.between_sets),
    );
    fields.put("rest_superset", &after(workout.rest.workout));
    let mut put_rest = |at: At, between: Target<Duration>, superset: String| {
        fields.put("rest_at", &at.key());
        fields.put("rest_at_first", &rest_text(between));
        fields.put("rest_at_second", &superset);
    };
    for (section, rest) in &workout.rest.sections {
        put_rest(At::Section(*section), rest.between_sets, after(*rest));
    }
    for (superset, rest) in &workout.rest.supersets {
        put_rest(At::Superset(superset.clone()), *rest, String::new());
    }
    for (slot, rest) in &workout.rest.exercises {
        put_rest(At::Exercise(*slot), *rest, String::new());
    }
    fields.put("static_hold", &seconds(workout.static_hold));
    fields
}

/// Linear progression, as its form posts it.
pub fn linear_fields(linear: &LinearProgression) -> Fields {
    let mut fields = Fields::default();
    fields.put("design", &"linear");
    put_ramp(&mut fields, &linear.warmup);
    for (key, top, back_off) in [
        ("heavy", linear.top_set_reps.higher, linear.back_off.higher),
        ("light", linear.top_set_reps.lower, linear.back_off.lower),
    ] {
        fields.put(&format!("{key}_top_reps"), &top);
        fields.put(&format!("{key}_back_off_sets"), &back_off.sets);
        fields.put(&format!("{key}_back_off_reps"), &back_off.reps);
        fields.put(&format!("{key}_back_off_share"), &back_off.of_top_set);
    }
    fields.put("light_of_heavy", &linear.light_of_heavy);
    fields.put("climb", &linear.ladder_climb_per_week);
    fields.put("entry_drop", &linear.entry_drop);
    for (key, reset) in [
        ("first_reset", linear.first_reset),
        ("second_reset", linear.second_reset),
    ] {
        fields.put(&format!("{key}_drop"), &reset.drop);
        fields.put(&format!("{key}_reclimb"), &reset.reclimb_per_week);
    }
    fields.put("rest_top_set", &rest_text(linear.rest.top_set));
    fields.put("rest_back_off", &rest_text(linear.rest.back_off));
    fields.put("rest_attempt", &rest_text(linear.rest.attempt));
    fields
}

/// SBS, as its form posts it.
pub fn sbs_fields(sbs: &SbsProgression) -> Fields {
    let mut fields = Fields::default();
    fields.put("design", &"sbs");
    put_ramp(&mut fields, &sbs.warmup);
    fields.put("rest_across", &rest_text(sbs.rest.across));
    fields.put("rest_rep_max", &rest_text(sbs.rest.rep_max));
    fields.put("rest_back_off", &rest_text(sbs.rest.back_off));
    fields.put("rest_attempt", &rest_text(sbs.rest.attempt));
    fields
}

/// Read a posted form.
///
/// # Errors
///
/// What is missing or does not read, in words for the page.
pub fn asked(fields: &Fields) -> Result<Asked, String> {
    match fields.get("design") {
        "workout" => {
            let intensity = Relative::try_from(fields.get("intensity").to_owned())
                .map_err(|error| error.to_string())?;
            workout_asked(fields).map(|workout| Asked::Workout(intensity, workout))
        }
        "linear" => linear_asked(fields).map(Asked::Linear),
        "sbs" => sbs_asked(fields).map(Asked::Sbs),
        other => Err(format!("{other:?} is not a form this page offers")),
    }
}

fn reps(text: &str, what: &str) -> Result<RepCount, String> {
    RepCount::try_from(text.to_owned()).map_err(|error| format!("{what}: {error}"))
}

fn duration(text: &str, what: &str) -> Result<Duration, String> {
    let bare = text.strip_suffix('s').unwrap_or(text).trim();
    Duration::try_from(bare.to_owned()).map_err(|error| format!("{what}: {error}"))
}

/// `85%`, or `85` with the sign of a percentage understood.
fn percentage(text: &str, what: &str) -> Result<Percentage, String> {
    let with_sign = if text.ends_with('%') {
        text.to_owned()
    } else {
        format!("{text}%")
    };
    Percentage::try_from(with_sign).map_err(|error| format!("{what}: {error}"))
}

fn kilos(text: &str, what: &str) -> Result<Kg, String> {
    let bare = text.strip_suffix("kg").unwrap_or(text).trim();
    Kg::try_from(bare.to_owned()).map_err(|error| format!("{what}: {error}"))
}

/// `5`, or `4-6`: a range runs low to high and must span.
fn target<M: Spans>(
    text: &str,
    what: &str,
    one: impl Fn(&str, &str) -> Result<M, String>,
) -> Result<Target<M>, String> {
    match text.split_once('-') {
        None => one(text.trim(), what).map(Target::Exactly),
        Some((low, high)) => Target::between(one(low.trim(), what)?, one(high.trim(), what)?)
            .ok_or_else(|| format!("{what}: a range runs low-high, and {text} does not")),
    }
}

fn rest(text: &str, what: &str) -> Result<Target<Duration>, String> {
    target(text, what, duration)
}

fn scheme(sets: &str, typed: &str, what: &str) -> Result<AccessoryScheme, String> {
    Ok(AccessoryScheme {
        sets: reps(sets, &format!("{what}: sets"))?,
        reps: target(typed, &format!("{what}: reps"), reps)?,
    })
}

/// Where a row says it is stated, refused where nothing narrower than the
/// workout can be stated there, or where another row already said it.
fn place(key: &str, offered: &[At], seen: &mut Vec<At>) -> Result<At, String> {
    let at = At::read(key)?;
    if !offered.contains(&at) {
        return Err(format!("nothing of that can be stated for {}", at.label()));
    }
    if seen.contains(&at) {
        return Err(format!("{} is given two values", at.label()));
    }
    seen.push(at.clone());
    Ok(at)
}

fn workout_asked(fields: &Fields) -> Result<Workout, String> {
    let mut levels = Levels::only(scheme(
        fields.needed("reps_sets", "the workout's sets")?,
        fields.needed("reps_reps", "the workout's reps")?,
        "the workout",
    )?);
    let offered = rep_places();
    let mut seen = Vec::new();
    for (key, sets, typed) in rows_of(fields, "reps_at") {
        if key.is_empty() {
            continue;
        }
        let at = place(key, &offered, &mut seen)?;
        let stated = scheme(sets, typed, &at.label())?;
        match at {
            At::Section(section) => levels.sections.insert(section, stated),
            At::Superset(superset) => levels.supersets.insert(superset, stated),
            At::Exercise(slot) => levels.exercises.insert(slot, stated),
        };
    }

    let after = |typed: &str, what: &str| -> Result<Option<Target<Duration>>, String> {
        match typed {
            "" => Ok(None),
            typed => rest(typed, what).map(Some),
        }
    };
    let mut scheme = RestScheme {
        workout: Rest {
            between_sets: rest(
                fields.needed("rest_between", "the workout's rest")?,
                "the workout's rest",
            )?,
            after_superset: after(
                fields.get("rest_superset"),
                "the workout's rest after a superset",
            )?,
        },
        sections: BTreeMap::new(),
        supersets: BTreeMap::new(),
        exercises: BTreeMap::new(),
    };
    let offered = rest_places();
    let mut seen = Vec::new();
    for (key, between, superset) in rows_of(fields, "rest_at") {
        if key.is_empty() {
            continue;
        }
        let at = place(key, &offered, &mut seen)?;
        let what = format!("rest for {}", at.label());
        let stated = rest(between, &what)?;
        match at {
            At::Section(section) => {
                scheme.sections.insert(
                    section,
                    Rest {
                        between_sets: stated,
                        after_superset: after(superset, &what)?,
                    },
                );
            }
            At::Superset(_) | At::Exercise(_) if !superset.is_empty() => {
                return Err(format!(
                    "{what}: only a section rests differently after a superset"
                ));
            }
            At::Superset(grouped) => {
                scheme.supersets.insert(grouped, stated);
            }
            At::Exercise(slot) => {
                scheme.exercises.insert(slot, stated);
            }
        }
    }

    Ok(Workout {
        reps: levels,
        rest: scheme,
        static_hold: duration(
            fields.needed("static_hold", "a static hold")?,
            "a static hold",
        )?,
    })
}

fn ramp_asked(fields: &Fields) -> Result<NonEmpty<WarmupStep>, String> {
    let mut steps = Vec::new();
    for (share, count) in fields
        .all("ramp_share")
        .into_iter()
        .zip(fields.all("ramp_reps"))
    {
        if share.is_empty() && count.is_empty() {
            continue;
        }
        steps.push(WarmupStep {
            of_top_set: percentage(share, "a ramp step")?,
            reps: reps(count, "a ramp step")?,
        });
    }
    NonEmpty::new(steps).map_err(|_| "the ramp needs a step".to_owned())
}

fn linear_asked(fields: &Fields) -> Result<LinearProgression, String> {
    let back_off = |key: &str| -> Result<BackOff, String> {
        Ok(BackOff {
            sets: reps(
                fields.needed(&format!("{key}_back_off_sets"), "back-off sets")?,
                "back-off sets",
            )?,
            reps: reps(
                fields.needed(&format!("{key}_back_off_reps"), "back-off reps")?,
                "back-off reps",
            )?,
            of_top_set: percentage(
                fields.needed(&format!("{key}_back_off_share"), "a back-off's share")?,
                "a back-off's share",
            )?,
        })
    };
    let top = |key: &str| -> Result<TopSetReps, String> {
        reps(
            fields.needed(&format!("{key}_top_reps"), "top set reps")?,
            "top set reps",
        )
        .map(TopSetReps::new)
    };
    let reset = |key: &str, what: &str| -> Result<ResetProtocol, String> {
        Ok(ResetProtocol {
            drop: percentage(fields.needed(&format!("{key}_drop"), what)?, what)?,
            reclimb_per_week: kilos(fields.needed(&format!("{key}_reclimb"), what)?, what)?,
        })
    };
    let rested = |name: &str, what: &str| rest(fields.needed(name, what)?, what);
    Ok(LinearProgression {
        warmup: ramp_asked(fields)?,
        top_set_reps: ByIntensity {
            higher: top("heavy")?,
            lower: top("light")?,
        },
        back_off: ByIntensity {
            higher: back_off("heavy")?,
            lower: back_off("light")?,
        },
        light_of_heavy: percentage(
            fields.needed("light_of_heavy", "the light session's share")?,
            "the light session's share",
        )?,
        ladder_climb_per_week: kilos(fields.needed("climb", "the climb")?, "the climb")?,
        entry_drop: percentage(
            fields.needed("entry_drop", "the entry drop")?,
            "the entry drop",
        )?,
        first_reset: reset("first_reset", "the first stall")?,
        second_reset: reset("second_reset", "the second stall")?,
        rest: LinearRest {
            top_set: rested("rest_top_set", "the rest after a top set")?,
            back_off: rested("rest_back_off", "the rest after a back-off")?,
            attempt: rested("rest_attempt", "the rest between attempts")?,
        },
    })
}

fn sbs_asked(fields: &Fields) -> Result<SbsProgression, String> {
    let rested = |name: &str, what: &str| rest(fields.needed(name, what)?, what);
    Ok(SbsProgression {
        warmup: ramp_asked(fields)?,
        rest: SbsRest {
            across: rested("rest_across", "the rest between sets across")?,
            rep_max: rested("rest_rep_max", "the rest after a repetition maximum")?,
            back_off: rested("rest_back_off", "the rest after a back-off")?,
            attempt: rested("rest_attempt", "the rest between attempts")?,
        },
    })
}

#[cfg(test)]
mod tests {
    use domain::{prescription::seed, schedule::Relative};

    use super::{Asked, Fields, asked, linear_fields, sbs_fields, workout_fields};

    /// **What the page writes is what it reads**: each shipped version, written
    /// as its form and read back, is the version it started as.
    #[test]
    fn the_shipped_versions_survive_their_forms() {
        let workout = seed::workout().expect("the seed builds");
        let linear = seed::linear().expect("the seed builds");
        let sbs = seed::sbs().expect("the seed builds");

        for intensity in [Relative::Higher, Relative::Lower] {
            assert_eq!(
                asked(&workout_fields(intensity, &workout)),
                Ok(Asked::Workout(intensity, workout.clone()))
            );
        }
        assert_eq!(asked(&linear_fields(&linear)), Ok(Asked::Linear(linear)));
        assert_eq!(asked(&sbs_fields(&sbs)), Ok(Asked::Sbs(sbs)));
    }

    fn with(fields: Fields, name: &str, value: &str) -> Fields {
        let Fields(mut all) = fields;
        if let Some(found) = all.iter_mut().find(|(key, _)| key == name) {
            found.1 = value.to_owned();
        }
        Fields(all)
    }

    fn adding(fields: Fields, added: &[(&str, &str)]) -> Fields {
        let Fields(mut all) = fields;
        all.extend(
            added
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned())),
        );
        Fields(all)
    }

    /// **A range typed backwards is refused, and says which.**
    #[test]
    fn a_backwards_range_is_refused() {
        let workout = seed::workout().expect("the seed builds");
        let refused = asked(&with(
            workout_fields(Relative::Lower, &workout),
            "reps_reps",
            "6-4",
        ));

        assert_eq!(
            refused,
            Err("the workout: reps: a range runs low-high, and 6-4 does not".to_owned())
        );
    }

    /// **A superset and an exercise state one rest**, so an after-superset rest
    /// beside either is refused rather than dropped.
    #[test]
    fn only_a_section_rests_differently_after_a_superset() {
        let workout = seed::workout().expect("the seed builds");
        let fields = adding(
            workout_fields(Relative::Higher, &workout),
            &[
                ("rest_at", "superset:biceps+triceps"),
                ("rest_at_first", "60"),
                ("rest_at_second", "90"),
            ],
        );

        let refused = asked(&fields).expect_err("refused");
        assert!(refused.contains("only a section"), "{refused}");
    }

    /// **A superset's rest is read as the superset's**, so a pair can rest
    /// differently from its section.
    #[test]
    fn a_superset_states_its_own_rest() {
        let workout = seed::workout().expect("the seed builds");
        let fields = adding(
            workout_fields(Relative::Higher, &workout),
            &[
                ("rest_at", "superset:biceps+triceps"),
                ("rest_at_first", "60"),
                ("rest_at_second", ""),
            ],
        );

        let Ok(Asked::Workout(_, read)) = asked(&fields) else {
            panic!("the form reads");
        };
        assert_eq!(read.rest.supersets.len(), 1);
    }

    /// **One place, one value**: two rows for the same place are refused.
    #[test]
    fn a_place_given_twice_is_refused() {
        let workout = seed::workout().expect("the seed builds");
        let fields = adding(
            workout_fields(Relative::Higher, &workout),
            &[
                ("reps_at", "exercise:wrist_flexion"),
                ("reps_at_first", "3"),
                ("reps_at_second", "10-15"),
            ],
        );

        let refused = asked(&fields).expect_err("refused");
        assert!(refused.contains("two values"), "{refused}");
    }

    /// **A ramp step left blank is not a step**, so a row added and not filled
    /// does not refuse the form.
    #[test]
    fn a_blank_ramp_row_is_ignored() {
        let linear = seed::linear().expect("the seed builds");
        let fields = adding(
            linear_fields(&linear),
            &[("ramp_share", ""), ("ramp_reps", "")],
        );

        assert_eq!(asked(&fields), Ok(Asked::Linear(linear)));
    }
}
