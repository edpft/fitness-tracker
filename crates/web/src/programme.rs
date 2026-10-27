//! The programme page: the programme, linear progression and SBS in force, and
//! a form to author the next version of each (#192).
//!
//! **Each form is one whole version**, because a version is one statement and
//! the store keeps it as one. The forms start from the version in force, and a
//! refused post starts from what was typed, so a typo does not cost the rest.
//!
//! **One encoding, both ways.** [`programme_fields`] and its siblings write a
//! version as the fields its form posts, and [`asked`] reads them back. The
//! page renders from fields whichever way they came, and the test that pins the
//! two is that the shipped versions survive the round trip.

use application::design::InForce;
use domain::{
    measure::{Duration, Kg, RepCount, Spans},
    normalised::OperatorZone,
    prescription::{
        AccessoryScheme, BackOff, ByIntensity, LinearProgression, Percentage, Programme,
        ResetProtocol, RestScheme, SbsChart, SbsDay, SbsProgression, SbsSession, SbsWeek, Section,
        SectionRest, SlotId, Target, TopSetReps, WarmupStep,
    },
    sequence::NonEmpty,
};
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Programme";

const STYLE: &str = "
form.design { display: grid; gap: 0.5rem; margin: 0.5rem 0 1.5rem; }
form.design input, form.design select { font: inherit; padding: 6px; width: 100%; min-width: 0; box-sizing: border-box; }
form.design fieldset { min-width: 0; display: grid; gap: 0.4rem; }
form.design label { display: grid; gap: 0.15rem; }
.pair { display: grid; grid-template-columns: repeat(auto-fit, minmax(7rem, 1fr)); gap: 0.5rem; }
.since { color: var(--muted); }
.said { color: var(--failed); }
.rows { display: grid; gap: 0.4rem; }
.rows > .row { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)) auto; gap: 0.4rem; align-items: end; }
.rows.slots > .row { grid-template-columns: minmax(0, 2fr) repeat(2, minmax(0, 1fr)) auto; }
.rows .remove, .add { width: auto; padding: 6px 10px; justify-self: start; }
[hidden] { display: none !important; }
";

/// Add a row by copying the last one emptied, remove one by taking it out, and
/// show a chart day's fields for the kind of day it is.
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
function showKind(select) {
  select.closest('fieldset').querySelectorAll('[data-kind]').forEach((group) => {
    group.hidden = group.dataset.kind !== select.value;
  });
}
";

/// What a posted form asked to author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    Programme(Programme),
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
}

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
    let typed_as = |design: &str| typed.filter(|fields| fields.get("design") == design);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (maud::PreEscaped(STYLE)) }
                script { (maud::PreEscaped(SCRIPT)) }
            }
            body {
                main {
                    (crate::page::menu("/programme"))
                    h1 { (TITLE) }
                    @if let Some(said) = said { p.said { (said) } }
                    p.legend {
                        "Saving a form stores a new version from now. A plan already made keeps "
                        "the versions it was planned with; the next plan takes these."
                    }
                    h2 { "The programme" }
                    @match &in_force.programme {
                        None => p { "None stored. `fitness init` stores the first." },
                        Some((at, programme)) => {
                            p.since { "Since " (since(at)) }
                            (programme_form(&typed_as("programme").cloned().unwrap_or_else(|| programme_fields(programme))))
                        }
                    }
                    h2 { "Linear progression" }
                    @match &in_force.linear {
                        None => p { "None stored. `fitness init` stores the first." },
                        Some((at, linear)) => {
                            p.since { "Since " (since(at)) }
                            (linear_form(&typed_as("linear").cloned().unwrap_or_else(|| linear_fields(linear))))
                        }
                    }
                    h2 { "SBS" }
                    @match &in_force.sbs {
                        None => p { "None stored. `fitness init` stores the first." },
                        Some((at, sbs)) => {
                            p.since { "Since " (since(at)) }
                            (sbs_form(&typed_as("sbs").cloned().unwrap_or_else(|| sbs_fields(sbs))))
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

/// The sections, as the page names them and the fields are keyed.
const SECTIONS: [Section; 5] = [
    Section::Plyometric,
    Section::Power,
    Section::Strength,
    Section::Hypertrophy,
    Section::Mobility,
];

/// The slots that run double progression, and so can have a scheme of their
/// own.
fn progressing() -> impl Iterator<Item = SlotId> {
    SlotId::ALL
        .iter()
        .copied()
        .filter(|slot| matches!(slot.section(), Section::Strength | Section::Hypertrophy))
}

fn input(fields: &Fields, name: &str, label: &str, hint: &str) -> Markup {
    html! {
        label {
            span { (label) }
            input type="text" name=(name) value=(fields.get(name)) placeholder=(hint);
        }
    }
}

fn programme_form(fields: &Fields) -> Markup {
    let slots: Vec<(&str, &str, &str)> = fields
        .all("slot")
        .into_iter()
        .zip(fields.all("slot_sets"))
        .zip(fields.all("slot_reps"))
        .map(|((slot, sets), reps)| (slot, sets, reps))
        .collect();
    let slots = if slots.is_empty() {
        vec![("", "", "")]
    } else {
        slots
    };
    html! {
        form.design method="post" action="/programme" {
            input type="hidden" name="design" value="programme";
            fieldset {
                legend { "Strength slots, other than the primary" }
                div.pair {
                    (input(fields, "strength_sets", "Sets", "3"))
                    (input(fields, "strength_reps", "Reps", "4-6"))
                }
            }
            fieldset {
                legend { "Hypertrophy slots" }
                div.pair {
                    (input(fields, "hypertrophy_sets", "Sets", "3"))
                    (input(fields, "hypertrophy_reps", "Reps", "4-6"))
                }
            }
            fieldset {
                legend { "Slots with a scheme of their own" }
                div.rows.slots {
                    @for (slot, sets, reps) in slots {
                        div.row {
                            label {
                                span { "Slot" }
                                select name="slot" {
                                    option value="" { "none" }
                                    @for each in progressing() {
                                        option value=(each.as_str()) selected[each.as_str() == slot] { (each) }
                                    }
                                }
                            }
                            label { span { "Sets" } input type="text" name="slot_sets" value=(sets); }
                            label { span { "Reps" } input type="text" name="slot_reps" value=(reps); }
                            button.remove type="button" onclick="removeRow(this)" { "Remove" }
                        }
                    }
                }
                button.add type="button" onclick="addRow(this)" { "Add a slot" }
            }
            fieldset {
                legend { "Rest, in seconds" }
                @for section in SECTIONS {
                    div.pair {
                        (input(fields, &format!("rest_{}", section.as_str()), &format!("{}, between sets", section.as_str()), "90 or 120-180"))
                        (input(fields, &format!("rest_{}_superset", section.as_str()), "after a superset", "as between sets"))
                    }
                }
            }
            (input(fields, "static_hold", "A static hold, in seconds", "60"))
            button type="submit" { "Save the programme" }
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
            div.rows {
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
        form.design method="post" action="/programme" {
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
            (input(fields, "light_of_heavy", "The light session's top set, of the heavy load", "90%"))
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
            button type="submit" { "Save linear progression" }
        }
    }
}

/// Every day of the chart, with the key its fields are posted under.
fn chart_days() -> impl Iterator<Item = (u32, SbsSession, String)> {
    (1..=4).flat_map(|week| {
        [SbsSession::First, SbsSession::Second]
            .into_iter()
            .map(move |session| (week, session, format!("w{week}_{session}")))
    })
}

const fn closes(week: u32, session: SbsSession) -> bool {
    week == 4 && matches!(session, SbsSession::Second)
}

fn sbs_form(fields: &Fields) -> Markup {
    html! {
        form.design method="post" action="/programme" {
            input type="hidden" name="design" value="sbs";
            (ramp(fields))
            @for (week, session, key) in chart_days() {
                fieldset {
                    legend { "Week " (week) ", " (session) " session" }
                    @if closes(week, session) {
                        input type="hidden" name=(format!("{key}_kind")) value="test";
                        (input(fields, &format!("{key}_reps"), "Test: a repetition maximum of", "1"))
                    } @else {
                        @let kind = fields.get(&format!("{key}_kind"));
                        label {
                            span { "Kind" }
                            select name=(format!("{key}_kind")) onchange="showKind(this)" {
                                option value="percentage" selected[kind == "percentage"] { "sets across at a share of the maximum" }
                                option value="repmax" selected[kind == "repmax"] { "work up to a repetition maximum, then back off" }
                            }
                        }
                        div.pair data-kind="percentage" hidden[kind == "repmax"] {
                            (input(fields, &format!("{key}_sets"), "Sets", "5"))
                            (input(fields, &format!("{key}_reps"), "Reps", "5"))
                            (input(fields, &format!("{key}_share"), "Of the maximum", "80%"))
                        }
                        div.pair data-kind="repmax" hidden[kind != "repmax"] {
                            (input(fields, &format!("{key}_max"), "A repetition maximum of", "8"))
                            (input(fields, &format!("{key}_back_off_sets"), "Back-off sets", "3"))
                            (input(fields, &format!("{key}_back_off_reps"), "Back-off reps", "5-6"))
                        }
                    }
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

fn put_ramp(fields: &mut Fields, steps: &NonEmpty<WarmupStep>) {
    for step in steps.iter() {
        fields.put("ramp_share", &step.of_top_set);
        fields.put("ramp_reps", &step.reps);
    }
}

/// The programme, as its form posts it.
pub fn programme_fields(programme: &Programme) -> Fields {
    let mut fields = Fields::default();
    fields.put("design", &"programme");
    for (key, scheme) in [
        ("strength", programme.strength),
        ("hypertrophy", programme.hypertrophy),
    ] {
        fields.put(&format!("{key}_sets"), &scheme.sets);
        fields.put(
            &format!("{key}_reps"),
            &target_text(scheme.reps, |reps| reps.to_string()),
        );
    }
    for (slot, scheme) in &programme.slots {
        fields.put("slot", &slot.as_str());
        fields.put("slot_sets", &scheme.sets);
        fields.put(
            "slot_reps",
            &target_text(scheme.reps, |reps| reps.to_string()),
        );
    }
    for section in SECTIONS {
        let rest = programme.rest.of(section);
        fields.put(
            &format!("rest_{}", section.as_str()),
            &target_text(rest.between_sets, seconds),
        );
        fields.put(
            &format!("rest_{}_superset", section.as_str()),
            &rest
                .after_superset
                .map_or_else(String::new, |after| target_text(after, seconds)),
        );
    }
    fields.put("static_hold", &seconds(programme.static_hold));
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
    fields
}

/// SBS, as its form posts it.
pub fn sbs_fields(sbs: &SbsProgression) -> Fields {
    let mut fields = Fields::default();
    fields.put("design", &"sbs");
    put_ramp(&mut fields, &sbs.warmup);
    for (week, session, day) in sbs.chart.days() {
        let key = format!("w{week}_{session}");
        match day {
            SbsDay::Percentage { sets, reps, share } => {
                fields.put(&format!("{key}_kind"), &"percentage");
                fields.put(&format!("{key}_sets"), &sets);
                fields.put(&format!("{key}_reps"), &reps);
                fields.put(&format!("{key}_share"), &share);
            }
            SbsDay::RepMax {
                reps,
                back_off_sets,
                back_off_reps,
            } => {
                fields.put(&format!("{key}_kind"), &"repmax");
                fields.put(&format!("{key}_max"), &reps);
                fields.put(&format!("{key}_back_off_sets"), &back_off_sets);
                fields.put(
                    &format!("{key}_back_off_reps"),
                    &target_text(back_off_reps, |reps| reps.to_string()),
                );
            }
            SbsDay::Test { reps } => {
                fields.put(&format!("{key}_kind"), &"test");
                fields.put(&format!("{key}_reps"), &reps);
            }
        }
    }
    fields
}

/// Read a posted form.
///
/// # Errors
///
/// What is missing or does not read, in words for the page.
pub fn asked(fields: &Fields) -> Result<Asked, String> {
    match fields.get("design") {
        "programme" => programme_asked(fields).map(Asked::Programme),
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

fn scheme(sets: &str, typed: &str, what: &str) -> Result<AccessoryScheme, String> {
    Ok(AccessoryScheme {
        sets: reps(sets, &format!("{what} sets"))?,
        reps: target(typed, &format!("{what} reps"), reps)?,
    })
}

fn programme_asked(fields: &Fields) -> Result<Programme, String> {
    let section_scheme = |key: &str| {
        scheme(
            fields.needed(&format!("{key}_sets"), &format!("{key} sets"))?,
            fields.needed(&format!("{key}_reps"), &format!("{key} reps"))?,
            key,
        )
    };
    let mut slots = std::collections::BTreeMap::new();
    for ((slot, sets), typed) in fields
        .all("slot")
        .into_iter()
        .zip(fields.all("slot_sets"))
        .zip(fields.all("slot_reps"))
    {
        if slot.is_empty() {
            continue;
        }
        let slot = SlotId::try_from(slot.to_owned()).map_err(|error| error.to_string())?;
        if !progressing().any(|each| each == slot) {
            return Err(format!("the {slot} slot does not run double progression"));
        }
        let what = slot.as_str();
        if slots.insert(slot, scheme(sets, typed, what)?).is_some() {
            return Err(format!("the {slot} slot is given two schemes"));
        }
    }
    let rest = |section: Section| -> Result<SectionRest, String> {
        let name = section.as_str();
        let between = fields.needed(&format!("rest_{name}"), &format!("{name} rest"))?;
        let after = fields.get(&format!("rest_{name}_superset"));
        Ok(SectionRest {
            between_sets: target(between, &format!("{name} rest"), duration)?,
            after_superset: match after {
                "" => None,
                typed => Some(target(
                    typed,
                    &format!("{name} rest after a superset"),
                    duration,
                )?),
            },
        })
    };
    Ok(Programme {
        strength: section_scheme("strength")?,
        hypertrophy: section_scheme("hypertrophy")?,
        slots,
        rest: RestScheme {
            plyometric: rest(Section::Plyometric)?,
            power: rest(Section::Power)?,
            strength: rest(Section::Strength)?,
            hypertrophy: rest(Section::Hypertrophy)?,
            mobility: rest(Section::Mobility)?,
        },
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
    })
}

fn day_asked(fields: &Fields, week: u32, session: SbsSession, key: &str) -> Result<SbsDay, String> {
    let what = format!("week {week}, {session} session");
    let field = |name: &str| fields.needed(&format!("{key}_{name}"), &what);
    match fields.get(&format!("{key}_kind")) {
        "percentage" => Ok(SbsDay::Percentage {
            sets: reps(field("sets")?, &what)?,
            reps: reps(field("reps")?, &what)?,
            share: percentage(field("share")?, &what)?,
        }),
        "repmax" => Ok(SbsDay::RepMax {
            reps: reps(field("max")?, &what)?,
            back_off_sets: reps(field("back_off_sets")?, &what)?,
            back_off_reps: target(field("back_off_reps")?, &what, reps)?,
        }),
        "test" => Ok(SbsDay::Test {
            reps: reps(field("reps")?, &what)?,
        }),
        other => Err(format!("{what}: {other:?} is not a kind of day")),
    }
}

fn sbs_asked(fields: &Fields) -> Result<SbsProgression, String> {
    let mut days = Vec::new();
    for (week, session, key) in chart_days() {
        days.push(day_asked(fields, week, session, &key)?);
    }
    let [w1a, w1b, w2a, w2b, w3a, w3b, w4a, w4b] = <[SbsDay; 8]>::try_from(days)
        .map_err(|_| "the chart runs four weeks of two days".to_owned())?;
    let week = |first, second| SbsWeek { first, second };
    let chart = SbsChart::new([
        week(w1a, w1b),
        week(w2a, w2b),
        week(w3a, w3b),
        week(w4a, w4b),
    ])
    .map_err(|error| error.to_string())?;
    Ok(SbsProgression {
        warmup: ramp_asked(fields)?,
        chart,
    })
}

#[cfg(test)]
mod tests {
    use domain::prescription::seed;

    use super::{Asked, Fields, asked, linear_fields, programme_fields, sbs_fields};

    /// **What the page writes is what it reads**: each shipped version, written
    /// as its form and read back, is the version it started as.
    #[test]
    fn the_shipped_versions_survive_their_forms() {
        let programme = seed::programme().expect("the seed builds");
        let linear = seed::linear().expect("the seed builds");
        let sbs = seed::sbs().expect("the seed builds");

        assert_eq!(
            asked(&programme_fields(&programme)),
            Ok(Asked::Programme(programme))
        );
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

    /// **A range typed backwards is refused, and says which.**
    #[test]
    fn a_backwards_range_is_refused() {
        let programme = seed::programme().expect("the seed builds");
        let refused = asked(&with(
            programme_fields(&programme),
            "hypertrophy_reps",
            "6-4",
        ));

        assert_eq!(
            refused,
            Err("hypertrophy reps: a range runs low-high, and 6-4 does not".to_owned())
        );
    }

    /// **The chart closes on its test**, so a test anywhere else is SBS's
    /// own refusal rather than one the page invents.
    #[test]
    fn a_test_out_of_place_is_refused() {
        let sbs = seed::sbs().expect("the seed builds");
        let fields = with(sbs_fields(&sbs), "w2_first_kind", "test");
        let fields = with(fields, "w2_first_reps", "1");

        let refused = asked(&fields).expect_err("refused");
        assert!(refused.contains("week 2's first session"), "{refused}");
    }

    /// **A ramp step left blank is not a step**, so a row added and not filled
    /// does not refuse the form.
    #[test]
    fn a_blank_ramp_row_is_ignored() {
        let linear = seed::linear().expect("the seed builds");
        let Fields(mut all) = linear_fields(&linear);
        all.push(("ramp_share".to_owned(), String::new()));
        all.push(("ramp_reps".to_owned(), String::new()));

        assert_eq!(asked(&Fields(all)), Ok(Asked::Linear(linear)));
    }
}
