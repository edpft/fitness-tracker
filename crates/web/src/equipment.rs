//! The equipment page: what each piece is, and the forms that declare, change
//! and correct it (#450).
//!
//! **Plain forms, posted and answered with the page.** Each fact has its own
//! form, so what is corrected is the fact beside the button and never one
//! picked from a list. Reading the fields into domain values is this page's
//! job; what a declaration or a correction means is `application`'s.

use domain::gym::{
    equipment::{
        Band, BandName, BandRange, Combination, EquipmentFacts, Fact, FactId, Maker, Model,
        Specific, Stack, Statement,
    },
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
.rows .band, .rows .ranged, .rows .step { display: grid; gap: 0.3rem; align-items: center; }
.rows .band, .rows .ranged { grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr) auto; }
.rows .step { grid-template-columns: repeat(3, minmax(0, 1fr)) auto; }
.rows .ranged > label { display: contents; }
.rows .band > label { display: contents; }
.rows input { width: 100%; min-width: 0; }
.rows .remove { width: auto; padding: 6px 10px; }
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
                (crate::page::head(TITLE, &[STYLE]))
                script { (maud::PreEscaped(SCRIPT)) }
            }
            body {
                main {
                    (crate::page::menu("/equipment"))
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
                    h2 { "Machines" }
                    p.legend { "A particular machine: another of its type may not offer the same stack." }
                    @for (specific, timeline) in &facts.machines {
                        h3 { (specific) }
                        @for fact in timeline.iter() {
                            (machine_fact(specific, fact, today))
                        }
                    }
                    (new_machine(today))
                    h2 { "Bands" }
                    p.legend {
                        "A range of bands by maker and model. Each band gives a force at twice its "
                        "length, and the steps say which bands, together, come next."
                    }
                    @for (specific, timeline) in &facts.bands {
                        h3 { (specific) }
                        @for fact in timeline.iter() {
                            (bands_fact(specific, fact, today))
                        }
                    }
                    (new_bands(today))
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
                    (crate::page::menu("/equipment"))
                    p.failed { "the store could not be read: " (error) }
                }
            }
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

fn maker_and_model(specific: &Specific) -> Markup {
    html! {
        input type="hidden" name="maker" value=(specific.maker);
        input type="hidden" name="model" value=(specific.model);
    }
}

fn naming(maker: &str, model: &str) -> Markup {
    html! {
        div.row {
            label { "Maker " input name="maker" placeholder=(maker) required; }
            label { "Model " input name="model" placeholder=(model) required; }
        }
    }
}

fn machine_fact(specific: &Specific, fact: &Fact<Stack>, today: Date) -> Markup {
    html! {
        p { (fact.value) " " span.period { (period(fact)) } }
        form.fact method="post" action="/equipment" {
            input type="hidden" name="category" value="machine";
            (maker_and_model(specific))
            (steps_rows(Some(fact.value.steps())))
            (most_field(Some(fact.value.most())))
            (acts(fact.id, today))
        }
    }
}

fn new_machine(today: Date) -> Markup {
    html! {
        details {
            summary { "Another machine" }
            form.fact method="post" action="/equipment" {
                input type="hidden" name="category" value="machine";
                (naming("Technogym", "Kneeling Easy Chin Dip"))
                (steps_rows(None))
                (most_field(None))
                (declare_row(today))
            }
        }
    }
}

fn bands_fact(specific: &Specific, fact: &Fact<BandRange>, today: Date) -> Markup {
    html! {
        p { (fact.value) " " span.period { (period(fact)) } }
        form.fact method="post" action="/equipment" {
            input type="hidden" name="category" value="bands";
            (maker_and_model(specific))
            (band_rows(Some(fact.value.bands())))
            (step_rows(Some(fact.value.steps())))
            (acts(fact.id, today))
        }
    }
}

fn new_bands(today: Date) -> Markup {
    html! {
        details {
            summary { "Another range of bands" }
            form.fact method="post" action="/equipment" {
                input type="hidden" name="category" value="bands";
                (naming("Physical", "Supaflex"))
                (band_rows(None))
                (step_rows(None))
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

/// A range's bands, one per row: what the maker marks it with, and the force
/// it takes to stretch it to twice its resting length.
fn band_rows(current: Option<&NonEmpty<Band>>) -> Markup {
    let bands: Vec<(String, String)> = current.map_or_else(
        || vec![(String::new(), String::new())],
        |bands| {
            bands
                .iter()
                .map(|band| (band.name.to_string(), band.at_twice_length.to_string()))
                .collect()
        },
    );
    html! {
        fieldset {
            legend { "Bands, and the force at twice their length, in kg" }
            div.rows {
                @for (name, force) in &bands {
                    div.row.ranged {
                        label { "band " input name="band_name" value=(name) placeholder="4/5 stars"; }
                        label { "kg " input type="number" name="band_force" value=(force)
                            min="0.001" step="0.001" inputmode="decimal"; }
                        button.remove type="button" onclick="removeRow(this)" { "Remove" }
                    }
                }
            }
            button type="button" onclick="addRow(this)" { "Add a band" }
        }
    }
}

/// The steps, most assistance first: up to three bands each, a band named
/// twice where two are used.
fn step_rows(current: Option<&NonEmpty<Combination>>) -> Markup {
    let steps: Vec<Vec<String>> = current.map_or_else(
        || vec![Vec::new()],
        |steps| {
            steps
                .iter()
                .map(|step| step.iter().map(ToString::to_string).collect())
                .collect()
        },
    );
    html! {
        fieldset {
            legend { "Steps, most assistance first" }
            div.rows {
                @for step in &steps {
                    div.row.step {
                        @for part in 0..STEP_PARTS {
                            input name="step_band" value=[step.get(part)] aria-label="Band";
                        }
                        input type="hidden" name="step_end" value="";
                        button.remove type="button" onclick="removeRow(this)" { "Remove" }
                    }
                }
            }
            button type="button" onclick="addRow(this)" { "Add a step" }
        }
    }
}

/// How many bands one step can name.
const STEP_PARTS: usize = 3;

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
    let specific = Specific {
        maker: Maker::try_from(form.required("maker")?).map_err(|error| error.to_string())?,
        model: Model::try_from(form.required("model")?).map_err(|error| error.to_string())?,
    };
    match form.required("category")? {
        "machine" => Ok(Statement::Machine(
            specific,
            Stack::new(steps(form)?, kilograms(form.required("most")?)?)
                .map_err(|error| error.to_string())?,
        )),
        "bands" => Ok(Statement::Bands(
            specific,
            BandRange::new(bands(form)?, band_steps(form)?).map_err(|error| error.to_string())?,
        )),
        other => Err(format!("{other:?} is not a category of equipment")),
    }
}

/// The band rows, skipping any left wholly empty.
fn bands(form: &Fields) -> Result<Vec<Band>, String> {
    let mut bands = Vec::new();
    for (name, force) in form
        .all("band_name")
        .into_iter()
        .zip(form.all("band_force"))
    {
        match (name.is_empty(), force.is_empty()) {
            (true, true) => {}
            (false, false) => bands.push(Band {
                name: BandName::try_from(name).map_err(|error| error.to_string())?,
                at_twice_length: kilograms(force)?,
            }),
            _ => return Err("each band needs both its name and its force".to_owned()),
        }
    }
    Ok(bands)
}

/// The step rows in order, each the bands named before its end marker,
/// skipping any left empty.
fn band_steps(form: &Fields) -> Result<Vec<Combination>, String> {
    let mut steps = Vec::new();
    let mut current = Vec::new();
    for (key, value) in &form.0 {
        match key.as_str() {
            "step_band" if !value.trim().is_empty() => {
                current.push(BandName::try_from(value.trim()).map_err(|error| error.to_string())?);
            }
            "step_end" => {
                if let Ok(bands) = NonEmpty::new(std::mem::take(&mut current)) {
                    steps.push(Combination::new(bands));
                }
            }
            _ => {}
        }
    }
    Ok(steps)
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
