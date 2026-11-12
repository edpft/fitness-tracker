//! The equipment page: what each piece is, and the forms that declare, change
//! and correct it (#450), on the input template (#667).
//!
//! **Plain forms, posted and answered with the page.** Each fact has its own
//! form, so what is corrected is the fact beside the button and never one
//! picked from a list. Reading the fields into domain values is this page's
//! job; what a declaration or a correction means is `application`'s.

use application::EquipmentError;
use domain::gym::{
    equipment::{
        Band, BandLadder, BandName, BandRange, Combination, EquipmentFacts, Fact, FactId,
        InvalidSettings, Maker, Model, Specific, Stack, Statement,
    },
    exercise::Implement,
};
use domain::measure::Kg;
use domain::prescription::{LoadSteps, Step};
use domain::sequence::NonEmpty;
use jiff::civil::Date;
use maud::{Markup, PreEscaped, html};

use crate::input::{self, Refusal};

const TITLE: &str = "Equipment";

const HERE: &str = "/equipment";

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

/// A post that was refused: the form as it was posted, and why.
pub struct Refused<'a> {
    pub typed: &'a [(String, String)],
    pub refusal: &'a Refusal,
}

/// Which form on the page: the fact it changes or corrects, or the category a
/// new one is declared in. Posted as `form`, so a refusal finds its way back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    Fact(FactId),
    Kind,
    Machine,
    Bands,
}

impl Which {
    fn key(self) -> String {
        match self {
            Self::Fact(fact) => format!("fact-{}", fact.get()),
            Self::Kind => "new-kind".to_owned(),
            Self::Machine => "new-machine".to_owned(),
            Self::Bands => "new-bands".to_owned(),
        }
    }

    fn id(self, name: &str) -> String {
        format!("{}-{name}", self.key())
    }

    /// The last post's refusal, if it was this form's.
    fn refused<'a>(self, refused: Option<&'a Refused<'_>>) -> Option<&'a Refused<'a>> {
        refused
            .filter(|refused| posted(refused.typed, "form").is_some_and(|form| form == self.key()))
    }
}

fn posted<'a>(typed: &'a [(String, String)], name: &str) -> Option<&'a str> {
    typed
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn every<'a>(typed: &'a [(String, String)], name: &str) -> Vec<&'a str> {
    typed
        .iter()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
        .collect()
}

/// What a form shows in each field: the fact it starts from, or what was
/// typed when the last post was this form's and was refused.
#[derive(Debug, Default)]
struct Shown {
    kind: String,
    maker: String,
    model: String,
    increments: Vec<(String, String)>,
    most: String,
    bands: Vec<(String, String)>,
    steps: Vec<Vec<String>>,
    from: String,
    always: bool,
}

impl Shown {
    fn typed(typed: &[(String, String)]) -> Self {
        let pairs = |first: &str, second: &str| -> Vec<(String, String)> {
            every(typed, first)
                .into_iter()
                .zip(every(typed, second))
                .map(|(one, two)| (one.to_owned(), two.to_owned()))
                .collect()
        };
        let mut steps = Vec::new();
        let mut current = Vec::new();
        for (key, value) in typed {
            match key.as_str() {
                "step_band" => current.push(value.clone()),
                "step_end" => steps.push(std::mem::take(&mut current)),
                _ => {}
            }
        }
        let field = |name: &str| posted(typed, name).unwrap_or_default().to_owned();
        Self {
            kind: field("kind"),
            maker: field("maker"),
            model: field("model"),
            increments: pairs("band_from", "band_step"),
            most: field("most"),
            bands: pairs("band_name", "band_force"),
            steps,
            from: field("from"),
            always: posted(typed, "always").is_some(),
        }
    }

    fn today(today: Date) -> Self {
        Self {
            from: today.to_string(),
            ..Self::default()
        }
    }

    fn increments(steps: &LoadSteps) -> Vec<(String, String)> {
        steps
            .bands()
            .iter()
            .map(|band| (band.from.to_string(), band.size.to_string()))
            .collect()
    }

    fn bands(range: &BandRange) -> Vec<(String, String)> {
        range
            .bands()
            .iter()
            .map(|band| (band.name.to_string(), band.at_twice_length.to_string()))
            .collect()
    }

    fn steps(range: &BandRange) -> Vec<Vec<String>> {
        range
            .steps()
            .iter()
            .map(|step| step.iter().map(ToString::to_string).collect())
            .collect()
    }
}

/// The page, with the last post if it was refused.
pub fn page(facts: &EquipmentFacts, today: Date, refused: Option<&Refused<'_>>) -> Markup {
    input::page(
        HERE,
        TITLE,
        &html! {
            script { (PreEscaped(SCRIPT)) }
            h2 { "Kinds" }
            @for (implement, timeline) in &facts.kinds {
                section.card {
                    h2.small { (input::title(implement.as_str())) }
                    @for fact in timeline.iter() {
                        @let which = Which::Fact(fact.id);
                        (held(fact))
                        (change(fact.id, &shown(which, refused, || Shown {
                            kind: implement.as_str().to_owned(),
                            increments: Shown::increments(&fact.value),
                            ..Shown::today(today)
                        }), refused, &|shown, refusal| html! {
                            input type="hidden" name="category" value="kind";
                            input type="hidden" name="kind" value=(implement.as_str());
                            (increments(shown, refusal))
                        }))
                    }
                }
            }
            (new_kind(facts, today, refused))
            h2 { "Machines" }
            @for (specific, timeline) in &facts.machines {
                section.card {
                    h2.small { (specific) }
                    @for fact in timeline.iter() {
                        @let which = Which::Fact(fact.id);
                        (held(fact))
                        (change(fact.id, &shown(which, refused, || Shown {
                            maker: specific.maker.to_string(),
                            model: specific.model.to_string(),
                            increments: Shown::increments(fact.value.steps()),
                            most: fact.value.most().to_string(),
                            ..Shown::today(today)
                        }), refused, &|shown, refusal| machine(which, shown, refusal)))
                    }
                }
            }
            (new_machine(today, refused))
            h2 { "Bands" }
            @for (specific, timeline) in &facts.bands {
                @let single = timeline.iter().count() == 1;
                section.card {
                    h2.small {
                        (specific)
                        @if single {
                            @for fact in timeline.iter() { " " span.period { "· " (period(fact.from)) } }
                        }
                    }
                    @for fact in timeline.iter() {
                        @let which = Which::Fact(fact.id);
                        @if !single { p.held.from { (period(fact.from)) } }
                        div.ladder {
                            @for (load, step) in BandLadder::of(specific, &fact.value).rungs() {
                                span { (step) } span.kg { (load) "kg" }
                            }
                        }
                        (change(fact.id, &shown(which, refused, || Shown {
                            maker: specific.maker.to_string(),
                            model: specific.model.to_string(),
                            bands: Shown::bands(&fact.value),
                            steps: Shown::steps(&fact.value),
                            ..Shown::today(today)
                        }), refused, &|shown, refusal| bands(which, shown, refusal)))
                    }
                }
            }
            (new_bands(today, refused))
        },
    )
}

/// What a form shows: what was typed if the last post was its own and was
/// refused, otherwise what `stored` says.
fn shown(which: Which, refused: Option<&Refused<'_>>, stored: impl FnOnce() -> Shown) -> Shown {
    which
        .refused(refused)
        .map_or_else(stored, |refused| Shown::typed(refused.typed))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    input::unreadable(HERE, TITLE, error)
}

/// From when a fact is true.
fn period(from: Option<Date>) -> String {
    from.map_or_else(|| "always".to_owned(), |date| format!("from {date}"))
}

/// What one fact says, and from when.
fn held<T: std::fmt::Display>(fact: &Fact<T>) -> Markup {
    html! {
        p.held { (fact.value) " " span.from { (period(fact.from)) } }
    }
}

/// The fields of one form, as `which` names them.
type Fields<'f> = dyn Fn(&Shown, Option<&Refusal>) -> Markup + 'f;

/// A fact's form: its fields, the date a change starts from, and the two
/// ways to post it.
fn change(
    fact: FactId,
    shown: &Shown,
    refused: Option<&Refused<'_>>,
    fields: &Fields<'_>,
) -> Markup {
    let which = Which::Fact(fact);
    let refusal = which.refused(refused).map(|refused| refused.refusal);
    input::folded(
        "Change",
        refusal,
        &html! {
            form.input method="post" action=(HERE) {
                input type="hidden" name="form" value=(which.key());
                input type="hidden" name="fact" value=(fact.get());
                (fields(shown, refusal))
                (input::field(&which.id("from"), "from", "From", &html! {
                    input id=(which.id("from")) type="date" name="from" value=(shown.from);
                }, refusal))
                (input::whole(refusal))
                div.actions {
                    button.quiet type="submit" name="act" value="correct" { "Correct" }
                    button type="submit" name="act" value="change" { "Change from" }
                }
            }
        },
    )
}

/// A form declaring something new, folded behind what it declares.
fn declare(
    which: Which,
    label: &str,
    shown: &Shown,
    refused: Option<&Refused<'_>>,
    fields: &Markup,
) -> Markup {
    let refusal = which.refused(refused).map(|refused| refused.refusal);
    html! {
        section.card {
            (input::folded(label, refusal, &html! {
                form.input method="post" action=(HERE) {
                    input type="hidden" name="form" value=(which.key());
                    (fields)
                    (input::field(&which.id("from"), "from", "From", &html! {
                        input id=(which.id("from")) type="date" name="from" value=(shown.from);
                    }, refusal))
                    label.check {
                        input type="checkbox" name="always" value="yes" checked[shown.always];
                        " Always"
                    }
                    (input::whole(refusal))
                    button type="submit" name="act" value="declare" { "Declare" }
                }
            }))
        }
    }
}

fn new_kind(facts: &EquipmentFacts, today: Date, refused: Option<&Refused<'_>>) -> Markup {
    let unstated: Vec<Implement> = Implement::ALL
        .iter()
        .copied()
        .filter(|implement| !facts.kinds.contains_key(implement))
        .collect();
    if unstated.is_empty() {
        return html! {};
    }
    let which = Which::Kind;
    let shown = shown(which, refused, || Shown::today(today));
    let refusal = which.refused(refused).map(|refused| refused.refusal);
    declare(
        which,
        "Another kind",
        &shown,
        refused,
        &html! {
            input type="hidden" name="category" value="kind";
            (input::field(&which.id("kind"), "kind", "Kind", &html! {
                select id=(which.id("kind")) name="kind" {
                    @for implement in &unstated {
                        option value=(implement.as_str()) selected[implement.as_str() == shown.kind] { (implement) }
                    }
                }
            }, refusal))
            (increments(&shown, refusal))
        },
    )
}

fn new_machine(today: Date, refused: Option<&Refused<'_>>) -> Markup {
    let which = Which::Machine;
    let shown = shown(which, refused, || Shown::today(today));
    let refusal = which.refused(refused).map(|refused| refused.refusal);
    declare(
        which,
        "Another machine",
        &shown,
        refused,
        &html! {
            (naming(which, &shown, refusal, "Technogym", "Kneeling Easy Chin Dip"))
            (machine(which, &shown, refusal))
        },
    )
}

fn new_bands(today: Date, refused: Option<&Refused<'_>>) -> Markup {
    let which = Which::Bands;
    let shown = shown(which, refused, || Shown::today(today));
    let refusal = which.refused(refused).map(|refused| refused.refusal);
    declare(
        which,
        "Another range of bands",
        &shown,
        refused,
        &html! {
            (naming(which, &shown, refusal, "Physical", "Supaflex"))
            (bands(which, &shown, refusal))
        },
    )
}

/// A new machine's or range's maker and model. A fact's are fixed, so they
/// are carried rather than asked.
fn naming(
    which: Which,
    shown: &Shown,
    refusal: Option<&Refusal>,
    maker: &str,
    model: &str,
) -> Markup {
    html! {
        (input::field(&which.id("maker"), "maker", "Maker", &html! {
            input id=(which.id("maker")) name="maker" value=(shown.maker) placeholder=(maker) autocomplete="off";
        }, refusal))
        (input::field(&which.id("model"), "model", "Model", &html! {
            input id=(which.id("model")) name="model" value=(shown.model) placeholder=(model) autocomplete="off";
        }, refusal))
    }
}

fn carried(which: Which, shown: &Shown) -> Markup {
    html! {
        @if matches!(which, Which::Fact(_)) {
            input type="hidden" name="maker" value=(shown.maker);
            input type="hidden" name="model" value=(shown.model);
        }
    }
}

fn machine(which: Which, shown: &Shown, refusal: Option<&Refusal>) -> Markup {
    html! {
        input type="hidden" name="category" value="machine";
        (carried(which, shown))
        (increments(shown, refusal))
        (input::field(&which.id("most"), "most", "The most it holds, in kg", &html! {
            input id=(which.id("most")) type="number" name="most" value=(shown.most)
                min="0.001" step="0.001" inputmode="decimal";
        }, refusal))
    }
}

fn bands(which: Which, shown: &Shown, refusal: Option<&Refusal>) -> Markup {
    html! {
        input type="hidden" name="category" value="bands";
        (carried(which, shown))
        (band_rows(shown, refusal))
        (step_rows(shown, refusal))
    }
}

/// A kind's increments, or a stack's: one row per band, each a load it starts
/// at and the step from there, in kilograms. Typed as numbers, so nothing is
/// parsed out of text.
fn increments(shown: &Shown, refusal: Option<&Refusal>) -> Markup {
    let rows = if shown.increments.is_empty() {
        vec![("0".to_owned(), String::new())]
    } else {
        shown.increments.clone()
    };
    html! {
        (input::group("increments", "Increments, in kg", &html! {
            div.row.band.head { span { "From" } span { "Step" } span {} }
            div.rows {
                @for (index, (from, size)) in rows.iter().enumerate() {
                    div.row.band {
                        input type="number" name="band_from" value=(from) aria-label="From, in kg"
                            min="0" step="0.001" inputmode="decimal" readonly[index == 0];
                        input type="number" name="band_step" value=(size) aria-label="Step, in kg"
                            min="0.001" step="0.001" inputmode="decimal";
                        button.remove type="button" onclick="removeRow(this)" aria-label="Remove" { "×" }
                    }
                }
            }
            button.quiet type="button" onclick="addRow(this)" { "Add a band" }
        }, refusal))
    }
}

/// A range's bands, one per row: what the maker marks it with, and the force
/// it takes to stretch it to twice its resting length.
fn band_rows(shown: &Shown, refusal: Option<&Refusal>) -> Markup {
    let rows = if shown.bands.is_empty() {
        vec![(String::new(), String::new())]
    } else {
        shown.bands.clone()
    };
    html! {
        (input::group("bands", "Bands, and the force at twice their length, in kg", &html! {
            div.row.band.head { span { "Band" } span { "kg" } span {} }
            div.rows {
                @for (name, force) in &rows {
                    div.row.band {
                        input name="band_name" value=(name) placeholder="4/5 stars" aria-label="Band";
                        input type="number" name="band_force" value=(force) aria-label="Force at twice its length, in kg"
                            min="0.001" step="0.001" inputmode="decimal";
                        button.remove type="button" onclick="removeRow(this)" aria-label="Remove" { "×" }
                    }
                }
            }
            button.quiet type="button" onclick="addRow(this)" { "Add a band" }
        }, refusal))
    }
}

/// The steps, most assistance first: up to three bands each, a band named
/// twice where two are used.
fn step_rows(shown: &Shown, refusal: Option<&Refusal>) -> Markup {
    let rows = if shown.steps.is_empty() {
        vec![Vec::new()]
    } else {
        shown.steps.clone()
    };
    html! {
        (input::group("steps", "Steps, most assistance first", &html! {
            div.rows {
                @for step in &rows {
                    div.row.step {
                        @for part in 0..STEP_PARTS {
                            input name="step_band" value=[step.get(part)] aria-label="Band";
                        }
                        input type="hidden" name="step_end" value="";
                        button.remove type="button" onclick="removeRow(this)" aria-label="Remove" { "×" }
                    }
                }
            }
            button.quiet type="button" onclick="addRow(this)" { "Add a step" }
        }, refusal))
    }
}

/// How many bands one step can name.
const STEP_PARTS: usize = 3;

/// A posted form, in the order its fields came: repeated names are the rows.
struct Posted<'a>(&'a [(String, String)]);

impl Posted<'_> {
    /// The first non-empty value under a name.
    fn field(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
            .find(|value| !value.is_empty())
    }

    fn required(&self, name: &'static str) -> Result<&str, Refusal> {
        self.field(name)
            .ok_or_else(|| Refusal::of(name, format!("{name} is needed")))
    }

    /// Every value under a name, in order, empty ones included so that rows
    /// line up.
    fn all(&self, name: &str) -> Vec<&str> {
        every(self.0, name).into_iter().map(str::trim).collect()
    }
}

fn kilograms(field: &'static str, typed: &str) -> Result<Kg, Refusal> {
    Kg::try_from(typed).map_err(|error| Refusal::of(field, error.to_string()))
}

/// The band rows, skipping any left wholly empty.
fn steps(form: &Posted<'_>) -> Result<LoadSteps, Refusal> {
    let refused = |reason: String| Refusal::of("increments", reason);
    let mut bands = Vec::new();
    for (from, size) in form.all("band_from").into_iter().zip(form.all("band_step")) {
        match (from.is_empty(), size.is_empty()) {
            (true, true) => {}
            (false, false) => bands.push(Step {
                from: kilograms("increments", from)?,
                size: kilograms("increments", size)?,
            }),
            _ => {
                return Err(refused(
                    "each band needs both where it starts and its step".to_owned(),
                ));
            }
        }
    }
    LoadSteps::new(bands).map_err(|error| refused(error.to_string()))
}

/// The field a range or a stack was refused over.
const fn settings(error: InvalidSettings) -> &'static str {
    match error {
        InvalidSettings::EmptyStack | InvalidSettings::MostOffTheStack => "most",
        InvalidSettings::NoBands | InvalidSettings::RepeatedBand | InvalidSettings::SlackBand => {
            "bands"
        }
        InvalidSettings::NoSteps
        | InvalidSettings::UnknownBand
        | InvalidSettings::StepsOutOfOrder => "steps",
    }
}

fn statement(form: &Posted<'_>) -> Result<Statement, Refusal> {
    let category = form
        .required("category")
        .map_err(|refusal| Refusal::whole(refusal.reason))?;
    if category == "kind" {
        let kind = form.required("kind")?;
        let implement =
            Implement::try_from(kind).map_err(|error| Refusal::of("kind", error.to_string()))?;
        return Ok(Statement::Kind(implement, steps(form)?));
    }
    let specific = Specific {
        maker: Maker::try_from(form.required("maker")?)
            .map_err(|error| Refusal::of("maker", error.to_string()))?,
        model: Model::try_from(form.required("model")?)
            .map_err(|error| Refusal::of("model", error.to_string()))?,
    };
    let invalid = |error: InvalidSettings| Refusal::of(settings(error), error.to_string());
    match category {
        "machine" => Ok(Statement::Machine(
            specific,
            Stack::new(steps(form)?, kilograms("most", form.required("most")?)?)
                .map_err(invalid)?,
        )),
        "bands" => Ok(Statement::Bands(
            specific,
            BandRange::new(bands_posted(form)?, band_steps(form)?).map_err(invalid)?,
        )),
        other => Err(Refusal::whole(format!(
            "{other:?} is not a category of equipment"
        ))),
    }
}

/// The band rows, skipping any left wholly empty.
fn bands_posted(form: &Posted<'_>) -> Result<Vec<Band>, Refusal> {
    let refused = |reason: String| Refusal::of("bands", reason);
    let mut bands = Vec::new();
    for (name, force) in form
        .all("band_name")
        .into_iter()
        .zip(form.all("band_force"))
    {
        match (name.is_empty(), force.is_empty()) {
            (true, true) => {}
            (false, false) => bands.push(Band {
                name: BandName::try_from(name).map_err(|error| refused(error.to_string()))?,
                at_twice_length: kilograms("bands", force)?,
            }),
            _ => {
                return Err(refused(
                    "each band needs both its name and its force".to_owned(),
                ));
            }
        }
    }
    Ok(bands)
}

/// The step rows in order, each the bands named before its end marker,
/// skipping any left empty.
fn band_steps(form: &Posted<'_>) -> Result<Vec<Combination>, Refusal> {
    let mut steps = Vec::new();
    let mut current = Vec::new();
    for (key, value) in form.0 {
        match key.as_str() {
            "step_band" if !value.trim().is_empty() => {
                current.push(
                    BandName::try_from(value.trim())
                        .map_err(|error| Refusal::of("steps", error.to_string()))?,
                );
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

fn date(form: &Posted<'_>) -> Result<Date, Refusal> {
    let typed = form.required("from")?;
    typed
        .parse()
        .map_err(|_| Refusal::of("from", format!("{typed:?} is not a date")))
}

/// Read a posted form into what it asks for.
///
/// # Errors
///
/// What is missing or does not read, and the field it is about.
pub fn asked(form: &[(String, String)]) -> Result<Asked, Refusal> {
    let form = Posted(form);
    let statement = statement(&form)?;
    match form.field("act") {
        Some("correct") => {
            let id = form
                .field("fact")
                .and_then(|id| id.parse::<i64>().ok())
                .ok_or_else(|| Refusal::whole("the fact to correct is not a number".to_owned()))?;
            Ok(Asked::Correct(FactId::new(id), statement))
        }
        Some("declare") if form.field("always").is_some() => Ok(Asked::Declare(statement, None)),
        Some("change" | "declare") => Ok(Asked::Declare(statement, Some(date(&form)?))),
        _ => Err(Refusal::whole("nothing was asked".to_owned())),
    }
}

/// A declaration or correction the use case refused, beside the field it is
/// about.
pub fn refusal(error: &EquipmentError) -> Refusal {
    match error {
        EquipmentError::AlreadyStated { .. } => Refusal::of("from", error.to_string()),
        EquipmentError::NoSuchFact { .. }
        | EquipmentError::NotTheSameEquipment { .. }
        | EquipmentError::Store(_) => Refusal::whole(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use domain::gym::equipment::EquipmentFacts;
    use jiff::civil::date;

    use super::{Asked, Refused, asked, page};

    fn posted(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    /// **Every post the page made before the template it still makes**: a
    /// change, a correction and a declaration from always each read.
    #[test]
    fn each_act_still_reads() {
        let machine = |act: &str| {
            posted(&[
                ("form", "fact-5"),
                ("fact", "5"),
                ("category", "machine"),
                ("maker", "Technogym"),
                ("model", "Kneeling Easy Chin Dip"),
                ("band_from", "0"),
                ("band_step", "7"),
                ("most", "84"),
                ("from", "2026-11-12"),
                ("act", act),
            ])
        };
        assert!(matches!(
            asked(&machine("change")),
            Ok(Asked::Declare(_, Some(from))) if from == date(2026, 11, 12)
        ));
        assert!(matches!(asked(&machine("correct")), Ok(Asked::Correct(..))));
        let kind = posted(&[
            ("form", "new-kind"),
            ("category", "kind"),
            ("kind", "kettlebell"),
            ("band_from", "0"),
            ("band_step", "4"),
            ("from", "2026-11-12"),
            ("always", "yes"),
            ("act", "declare"),
        ]);
        assert!(matches!(asked(&kind), Ok(Asked::Declare(_, None))));
    }

    /// **A refused declaration comes back open as it was typed**, every row
    /// of it, with the reason under the rows it is about.
    #[test]
    fn a_refused_declaration_keeps_what_was_typed() {
        let typed = posted(&[
            ("form", "new-bands"),
            ("category", "bands"),
            ("maker", "Physical"),
            ("model", "Minis"),
            ("band_name", "Red"),
            ("band_force", "10"),
            ("band_name", "Green"),
            ("band_force", "5"),
            ("step_band", "Green"),
            ("step_band", ""),
            ("step_band", ""),
            ("step_end", ""),
            ("step_band", "Red"),
            ("step_band", ""),
            ("step_band", ""),
            ("step_end", ""),
            ("from", "2026-11-12"),
            ("act", "declare"),
        ]);
        let Err(refusal) = asked(&typed) else {
            panic!("steps rising in assistance are refused");
        };
        assert_eq!(refusal.field, Some("steps"));
        let refused = Refused {
            typed: &typed,
            refusal: &refusal,
        };
        let shown = page(
            &EquipmentFacts::default(),
            date(2026, 11, 1),
            Some(&refused),
        )
        .into_string();

        assert!(shown.contains(r#"value="Minis""#), "{shown}");
        assert!(shown.contains(r#"value="Green""#), "{shown}");
        assert!(shown.contains(r#"value="2026-11-12""#), "{shown}");
        let steps = shown
            .find("Steps, most assistance first")
            .expect("the steps");
        let after = &shown[steps..];
        let reason = after.find("less assistance").expect("the reason");
        let from = after.find(">From<").expect("the from field");
        assert!(reason < from, "{after}");
        assert_eq!(
            shown.matches("<details class=\"folded\" open").count(),
            1,
            "{shown}"
        );
    }
}
