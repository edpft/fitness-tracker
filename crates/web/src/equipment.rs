//! The equipment page: what each piece is, and the forms that declare, change
//! and correct it (#450).
//!
//! **Plain forms, posted and answered with the page.** Each fact has its own
//! form, so what is corrected is the fact beside the button and never one
//! picked from a list. Reading the fields into domain values is this page's
//! job; what a declaration or a correction means is `application`'s.

use std::collections::HashMap;

use domain::gym::{
    equipment::{EquipmentFacts, Fact, FactId, LevelName, Settings, SpecificName, Statement},
    exercise::Implement,
};
use domain::measure::Kg;
use domain::prescription::LoadSteps;
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

const TITLE: &str = "Equipment";

const STYLE: &str = "
form.fact { display: grid; gap: 0.4rem; margin: 0.5rem 0 1.25rem; }
form.fact input, form.fact select { font: inherit; padding: 6px; }
.row { display: flex; gap: 0.5rem; flex-wrap: wrap; }
.row > * { flex: 1 1 8rem; }
.row button { width: auto; padding: 8px 12px; }
.period { color: var(--muted); }
.said { color: var(--failed); }
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
                style { (crate::page::STYLE) (STYLE) }
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
                    p.legend { "A particular product, assumed of nothing else." }
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
            label { "Increments " input name="steps" value=(fact.value) required; }
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
                    label { "Increments " input name="steps" placeholder="2.5kg, or 1kg, 2kg from 10kg" required; }
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

/// A stack or levels: both sets of fields, and a choice of which is meant.
fn settings_fields(current: Option<&Settings>) -> Markup {
    let (steps, most, levels) = match current {
        Some(Settings::Stack { steps, most }) => {
            (steps.to_string(), most.to_string(), String::new())
        }
        Some(Settings::Levels(levels)) => {
            let names: Vec<&str> = levels.iter().map(LevelName::as_str).collect();
            (String::new(), String::new(), names.join(", "))
        }
        None => (String::new(), String::new(), String::new()),
    };
    let levelled = matches!(current, Some(Settings::Levels(_)));
    html! {
        div.row {
            label { input type="radio" name="shape" value="stack" checked[!levelled]; " a stack" }
            label { input type="radio" name="shape" value="levels" checked[levelled]; " levels" }
        }
        div.row {
            label { "Steps " input name="steps" value=(steps) placeholder="7kg"; }
            label { "To " input name="most" value=(most) placeholder="84"; }
        }
        label { "Levels, in order " input name="levels" value=(levels) placeholder="1, 2, 3, 4, 5"; }
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

fn field<'a>(form: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    form.get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn required<'a>(form: &'a HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    field(form, name).ok_or_else(|| format!("{name} is needed"))
}

fn statement(form: &HashMap<String, String>) -> Result<Statement, String> {
    let steps = || LoadSteps::try_from(required(form, "steps")?).map_err(|error| error.to_string());
    if let Some(kind) = field(form, "kind") {
        let implement = Implement::try_from(kind).map_err(|error| error.to_string())?;
        return Ok(Statement::Kind(implement, steps()?));
    }
    let name =
        SpecificName::try_from(required(form, "specific")?).map_err(|error| error.to_string())?;
    let settings = if field(form, "shape") == Some("levels") {
        let levels = required(form, "levels")?
            .split(',')
            .map(LevelName::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Settings::levels(levels)
    } else {
        let typed = required(form, "most")?;
        let most = Kg::try_from(typed.strip_suffix("kg").unwrap_or(typed).trim())
            .map_err(|error| error.to_string())?;
        Settings::stack(steps()?, most)
    }
    .map_err(|error| error.to_string())?;
    Ok(Statement::Specific(name, settings))
}

fn date(form: &HashMap<String, String>) -> Result<Date, String> {
    let typed = required(form, "from")?;
    typed
        .parse()
        .map_err(|_| format!("{typed:?} is not a date"))
}

/// Read a posted form into what it asks for.
///
/// # Errors
///
/// What is missing or does not read, in words for the page.
pub fn asked(form: &HashMap<String, String>) -> Result<Asked, String> {
    let statement = statement(form)?;
    match field(form, "act") {
        Some("correct") => {
            let id = required(form, "fact")?
                .parse::<i64>()
                .map_err(|_| "the fact to correct is not a number".to_owned())?;
            Ok(Asked::Correct(FactId::new(id), statement))
        }
        Some("declare") if field(form, "always").is_some() => Ok(Asked::Declare(statement, None)),
        Some("change" | "declare") => Ok(Asked::Declare(statement, Some(date(form)?))),
        _ => Err("nothing was asked".to_owned()),
    }
}
