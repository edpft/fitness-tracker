//! The equipment page: what each piece is, and the forms that declare, change
//! and correct it (#450).
//!
//! **Plain forms, posted and answered with the page.** Each fact has its own
//! form, so what is corrected is the fact beside the button and never one
//! picked from a list. Reading the fields into domain values is this page's
//! job; what a declaration or a correction means is `application`'s.

use domain::gym::{
    equipment::{EquipmentFacts, Fact, FactId, LevelName, Settings, SpecificName, Statement},
    exercise::Implement,
};
use domain::measure::Kg;
use domain::prescription::{LoadSteps, Step};
use domain::sequence::NonEmpty;
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Equipment";

const STYLE: &str = "
form.fact { display: grid; gap: 0.4rem; margin: 0.5rem 0 1.25rem; }
form.fact input, form.fact select { font: inherit; padding: 6px; max-width: 100%; box-sizing: border-box; }
form.fact fieldset { min-width: 0; }
.row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
.row > * { flex: 1 1 8rem; min-width: 0; }
.row button { width: auto; padding: 8px 12px; }
.period { color: var(--muted); }
.said { color: var(--failed); }
.rows { display: grid; gap: 0.4rem; }
.rows + button { margin-top: 0.4rem; }
.rows > .row:first-child .remove { visibility: hidden; }
.rows .band, .rows .level { display: grid; gap: 0.3rem; align-items: center; }
.rows .band { grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr) auto; }
.rows .level { grid-template-columns: minmax(0, 1fr) auto; }
.rows .band > label { display: contents; }
.rows input { width: 100%; min-width: 0; }
.rows .remove { width: auto; padding: 6px 10px; }
form:has(input[name=shape][value=levels]:checked) .stack,
form:has(input[name=shape][value=stack]:checked) .levels { display: none; }
";

/// Add a row by copying the last one emptied, remove one by taking it out.
/// The first row of steps starts at nothing and is never removed, so the
/// rule that a scale starts at 0kg cannot be broken from the page.
const SCRIPT: &str = "
function addRow(button) {
  const rows = button.previousElementSibling;
  const row = rows.lastElementChild.cloneNode(true);
  row.querySelectorAll('input').forEach((input) => { input.value = ''; input.readOnly = false; });
  rows.appendChild(row);
}
function removeRow(button) { button.closest('.row').remove(); }
";

/// What one posted form asked for.
pub enum Asked {
    Declare(Statement, Option<Date>),
    Correct(FactId, Statement),
}

/// The page, with what the last post came to if there was one.
pub fn page(facts: &EquipmentFacts, today: Date, said: Option<&str>) -> Markup {
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
                    p { a href="/" { "Run next" } }
                    h1 { (TITLE) }
                    @if let Some(said) = said { p.said { (said) } }
                    p.legend {
                        "Change: true from a date, and what came before stays true before it. "
                        "Correct: it was always this, and what was said is no longer believed."
                    }
                    h2 { "Kinds" }
                    p.legend { "Interchangeable: one barbell is another." }
                    @for (implement, timeline) in &facts.kinds {
                        h3 { (implement) }
                        @for fact in timeline.iter() {
                            (kind_fact(*implement, fact, today))
                        }
                    }
                    (new_kind(facts, today))
                    h2 { "Specific equipment" }
                    p.legend { "A particular product: another of its type may not offer the same settings." }
                    @for (name, timeline) in &facts.specifics {
                        h3 { (name) }
                        @for fact in timeline.iter() {
                            (specific_fact(name, fact, today))
                        }
                    }
                    (new_specific(today))
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
            body { main { p.failed { "the store could not be read: " (error) } } }
        }
    }
}

fn period<T>(fact: &Fact<T>) -> String {
    fact.from
        .map_or_else(|| "always".to_owned(), |date| format!("from {date}"))
}

/// The two buttons every fact carries, and the date a change starts from.
fn acts(fact: FactId, today: Date) -> Markup {
    html! {
        input type="hidden" name="fact" value=(fact.get());
        div.row {
            button type="submit" name="act" value="correct" { "Correct" }
            input type="date" name="from" value=(today) aria-label="Changed from";
            button type="submit" name="act" value="change" { "Change from" }
        }
    }
}

fn kind_fact(implement: Implement, fact: &Fact<LoadSteps>, today: Date) -> Markup {
    html! {
        p { (fact.value) " " span.period { (period(fact)) } }
        form.fact method="post" action="/equipment" {
            input type="hidden" name="kind" value=(implement.as_str());
            (steps_rows(Some(&fact.value)))
            (acts(fact.id, today))
        }
    }
}

fn new_kind(facts: &EquipmentFacts, today: Date) -> Markup {
    let unstated: Vec<Implement> = Implement::ALL
        .iter()
        .copied()
        .filter(|implement| !facts.kinds.contains_key(implement))
        .collect();
    html! {
        @if !unstated.is_empty() {
            details {
                summary { "Another kind" }
                form.fact method="post" action="/equipment" {
                    select name="kind" {
                        @for implement in &unstated {
                            option value=(implement.as_str()) { (implement) }
                        }
                    }
                    (steps_rows(None))
                    (declare_row(today))
                }
            }
        }
    }
}

fn specific_fact(name: &SpecificName, fact: &Fact<Settings>, today: Date) -> Markup {
    html! {
        p { (fact.value) " " span.period { (period(fact)) } }
        form.fact method="post" action="/equipment" {
            input type="hidden" name="specific" value=(name);
            (settings_fields(Some(&fact.value)))
            (acts(fact.id, today))
        }
    }
}

fn new_specific(today: Date) -> Markup {
    html! {
        details {
            summary { "Another specific piece of equipment" }
            form.fact method="post" action="/equipment" {
                label { "Name " input name="specific" placeholder="Technogym Kneeling Easy Chin Dip" required; }
                (settings_fields(None))
                (declare_row(today))
            }
        }
    }
}

/// A kind's increments, or a stack's: one row per band, each a load it starts
/// at and the step from there, in kilograms. Typed as numbers, so nothing is
/// parsed out of text.
fn steps_rows(current: Option<&LoadSteps>) -> Markup {
    let bands: Vec<(String, String)> = current.map_or_else(
        || vec![("0".to_owned(), String::new())],
        |steps| {
            steps
                .bands()
                .iter()
                .map(|band| (band.from.to_string(), band.size.to_string()))
                .collect()
        },
    );
    html! {
        fieldset {
            legend { "Increments, in kg" }
            div.rows {
                @for (index, (from, size)) in bands.iter().enumerate() {
                    div.row.band {
                        label { "from " input type="number" name="band_from" value=(from)
                            min="0" step="0.001" inputmode="decimal" readonly[index == 0] required; }
                        label { "step " input type="number" name="band_step" value=(size)
                            min="0.001" step="0.001" inputmode="decimal" required; }
                        button.remove type="button" onclick="removeRow(this)" { "Remove" }
                    }
                }
            }
            button type="button" onclick="addRow(this)" { "Add a band" }
        }
    }
}

/// A range's levels, one per row, in the maker's order.
fn level_rows(current: Option<&NonEmpty<LevelName>>) -> Markup {
    let levels: Vec<String> = current.map_or_else(
        || vec![String::new()],
        |levels| {
            levels
                .iter()
                .map(|level| level.as_str().to_owned())
                .collect()
        },
    );
    html! {
        fieldset {
            legend { "Levels, in order" }
            div.rows {
                @for level in &levels {
                    div.row.level {
                        input name="level" value=(level) aria-label="Level";
                        button.remove type="button" onclick="removeRow(this)" { "Remove" }
                    }
                }
            }
            button type="button" onclick="addRow(this)" { "Add a level" }
        }
    }
}

/// A specific's settings. A known one keeps its shape, so only its own fields
/// are offered; a new one chooses, and only the chosen fields are shown and
/// read.
fn settings_fields(current: Option<&Settings>) -> Markup {
    match current {
        Some(Settings::Stack { steps, most }) => html! {
            input type="hidden" name="shape" value="stack";
            (steps_rows(Some(steps)))
            (most_field(Some(*most)))
        },
        Some(Settings::Levels(levels)) => html! {
            input type="hidden" name="shape" value="levels";
            (level_rows(Some(levels)))
        },
        None => html! {
            div.row {
                label { input type="radio" name="shape" value="stack" checked; " a stack" }
                label { input type="radio" name="shape" value="levels"; " levels" }
            }
            div.stack { (steps_rows(None)) (most_field(None)) }
            div.levels { (level_rows(None)) }
        },
    }
}

fn most_field(current: Option<Kg>) -> Markup {
    html! {
        label { "The most it holds, in kg "
            input type="number" name="most" value=[current.map(|most| most.to_string())]
                min="0.001" step="0.001" inputmode="decimal";
        }
    }
}

fn declare_row(today: Date) -> Markup {
    html! {
        div.row {
            input type="date" name="from" value=(today) aria-label="True from";
            label { input type="checkbox" name="always" value="yes"; " always" }
            button type="submit" name="act" value="declare" { "Declare" }
        }
    }
}

/// A posted form, in the order its fields came: repeated names are the rows.
pub struct Fields(Vec<(String, String)>);

impl Fields {
    pub const fn new(fields: Vec<(String, String)>) -> Self {
        Self(fields)
    }

    /// The first non-empty value under a name.
    fn field(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .find(|value| !value.is_empty())
    }

    fn required(&self, name: &str) -> Result<&str, String> {
        self.field(name).ok_or_else(|| format!("{name} is needed"))
    }

    /// Every value under a name, in order, empty ones included so that rows
    /// line up.
    fn all(&self, name: &str) -> Vec<&str> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .collect()
    }
}

fn kilograms(typed: &str) -> Result<Kg, String> {
    Kg::try_from(typed).map_err(|error| error.to_string())
}

/// The band rows, skipping any left wholly empty.
fn steps(form: &Fields) -> Result<LoadSteps, String> {
    let mut bands = Vec::new();
    for (from, size) in form.all("band_from").into_iter().zip(form.all("band_step")) {
        match (from.is_empty(), size.is_empty()) {
            (true, true) => {}
            (false, false) => bands.push(Step {
                from: kilograms(from)?,
                size: kilograms(size)?,
            }),
            _ => return Err("each band needs both where it starts and its step".to_owned()),
        }
    }
    LoadSteps::new(bands).map_err(|error| error.to_string())
}

fn statement(form: &Fields) -> Result<Statement, String> {
    if let Some(kind) = form.field("kind") {
        let implement = Implement::try_from(kind).map_err(|error| error.to_string())?;
        return Ok(Statement::Kind(implement, steps(form)?));
    }
    let name =
        SpecificName::try_from(form.required("specific")?).map_err(|error| error.to_string())?;
    let settings = if form.field("shape") == Some("levels") {
        let levels = form
            .all("level")
            .into_iter()
            .filter(|level| !level.is_empty())
            .map(LevelName::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Settings::levels(levels)
    } else {
        Settings::stack(steps(form)?, kilograms(form.required("most")?)?)
    }
    .map_err(|error| error.to_string())?;
    Ok(Statement::Specific(name, settings))
}

fn date(form: &Fields) -> Result<Date, String> {
    let typed = form.required("from")?;
    typed
        .parse()
        .map_err(|_| format!("{typed:?} is not a date"))
}

/// Read a posted form into what it asks for.
///
/// # Errors
///
/// What is missing or does not read, in words for the page.
pub fn asked(form: &Fields) -> Result<Asked, String> {
    let statement = statement(form)?;
    match form.field("act") {
        Some("correct") => {
            let id = form
                .required("fact")?
                .parse::<i64>()
                .map_err(|_| "the fact to correct is not a number".to_owned())?;
            Ok(Asked::Correct(FactId::new(id), statement))
        }
        Some("declare") if form.field("always").is_some() => Ok(Asked::Declare(statement, None)),
        Some("change" | "declare") => Ok(Asked::Declare(statement, Some(date(form)?))),
        _ => Err("nothing was asked".to_owned()),
    }
}
