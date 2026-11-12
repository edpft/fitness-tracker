//! The primary's progressions on the workout page (#192, #771): linear
//! progression and SBS in force, and a form to author the next version of each.
//!
//! **Each form is one whole version**, because a version is one statement and
//! the store keeps it as one. The forms start from the version in force, and a
//! refused post starts from what was typed, so a typo does not cost the rest.
//!
//! **SBS's chart is shown and not offered**: it is SBS's (§ 14.1), and what is
//! authored for SBS is only what it leaves out, its ramp and its rest.
//!
//! **One encoding, both ways.** [`linear_fields`] and [`sbs_fields`] write a
//! version as the fields its form posts, and [`asked`] reads them back. The test
//! that pins the two is that the shipped versions survive the round trip.

use domain::{
    measure::{Duration, Kg, RepCount, Spans},
    prescription::{
        BackOff, ByIntensity, LinearProgression, LinearRest, Percentage, ResetProtocol, SbsChart,
        SbsDay, SbsProgression, SbsRest, Target, TopSetReps, WarmupStep,
    },
    sequence::NonEmpty,
};
use maud::{Markup, html};

pub const STYLE: &str = "
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
pub const SCRIPT: &str = "
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

    /// Whether this is the form for `design`.
    pub fn is_for(&self, design: &str) -> bool {
        self.get("design") == design
    }
}

fn input(fields: &Fields, name: &str, label: &str, hint: &str) -> Markup {
    html! {
        label {
            span { (label) }
            input type="text" name=(name) value=(fields.get(name)) placeholder=(hint);
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

pub fn linear_form(fields: &Fields) -> Markup {
    html! {
        form.design method="post" action="/workout" {
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

pub fn chart_table(chart: &SbsChart) -> Markup {
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

pub fn sbs_form(fields: &Fields) -> Markup {
    html! {
        form.design method="post" action="/workout" {
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
pub fn target_text<M: Spans>(target: Target<M>, one: impl Fn(M) -> String) -> String {
    match target {
        Target::Exactly(measure) => one(measure),
        Target::Range { minimum, extent } => {
            format!("{}-{}", one(minimum), one(minimum.spanning(extent)))
        }
    }
}

pub fn seconds(duration: Duration) -> String {
    duration.as_seconds().to_string()
}

pub fn rest_text(rest: Target<Duration>) -> String {
    target_text(rest, seconds)
}

pub fn reps_text(reps: Target<RepCount>) -> String {
    target_text(reps, |count| count.to_string())
}

fn put_ramp(fields: &mut Fields, steps: &NonEmpty<WarmupStep>) {
    for step in steps.iter() {
        fields.put("ramp_share", &step.of_top_set);
        fields.put("ramp_reps", &step.reps);
    }
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
        "linear" => linear_asked(fields).map(Asked::Linear),
        "sbs" => sbs_asked(fields).map(Asked::Sbs),
        other => Err(format!("{other:?} is not a form this page offers")),
    }
}

pub fn reps(text: &str, what: &str) -> Result<RepCount, String> {
    RepCount::try_from(text.to_owned()).map_err(|error| format!("{what}: {error}"))
}

pub fn duration(text: &str, what: &str) -> Result<Duration, String> {
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
pub fn target<M: Spans>(
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

pub fn rest(text: &str, what: &str) -> Result<Target<Duration>, String> {
    target(text, what, duration)
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
    use domain::prescription::seed;

    use super::{Asked, Fields, asked, linear_fields, sbs_fields};

    /// **What the page writes is what it reads**: each shipped version, written
    /// as its form and read back, is the version it started as.
    #[test]
    fn the_shipped_versions_survive_their_forms() {
        let linear = seed::linear().expect("the seed builds");
        let sbs = seed::sbs().expect("the seed builds");

        assert_eq!(asked(&linear_fields(&linear)), Ok(Asked::Linear(linear)));
        assert_eq!(asked(&sbs_fields(&sbs)), Ok(Asked::Sbs(sbs)));
    }

    /// **A ramp step left blank is not a step**, so a row added and not filled
    /// does not refuse the form.
    #[test]
    fn a_blank_ramp_row_is_ignored() {
        let linear = seed::linear().expect("the seed builds");
        let Fields(mut all) = linear_fields(&linear);
        all.extend([
            ("ramp_share".to_owned(), String::new()),
            ("ramp_reps".to_owned(), String::new()),
        ]);

        assert_eq!(asked(&Fields(all)), Ok(Asked::Linear(linear)));
    }
}
