//! Drinking (#873), under Report: a day's drinking, at whatever accuracy
//! is known.
//!
//! **A screen for each way it is used** (operator, 2026-11-19: *"some things
//! are only relevant for certain use cases"*). The page opens on today,
//! where each drink is saved as it is added, with the days reported below.
//! Another day is a page of its own, for a day whose drinks are known
//! exactly or not at all, and for saying afterwards that some were not
//! counted, or removing it. A day rather than an evening: drinking is
//! usually at night but not always (operator, 2026-11-19).
//!
//! **Ease is the requirement.** The operator: *"If we make this hard to use, I
//! won't use it."*

use std::num::{NonZeroU8, NonZeroU16};

use application::{DrinkingError, drinking::Choices};
use domain::{
    drinking::{Day, Drink, DrinkName, Drinking, Drinks, Millilitres, OwnDrink, Strength},
    sequence::NonEmpty,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail,
    input::{self, Refusal},
    page,
};

pub const TITLE: &str = "Drinking";

pub const HERE: &str = "/drinking";

/// Another day.
pub const DAY: &str = "/drinking/day";

/// What the picker posts for one of his own not yet named.
const OWN: &str = "own";

/// How many days the page lists.
const LISTED: usize = 10;

/// A day as it is being counted: its date, the drinks so far, whether
/// more were drunk than counted, and the drink picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counting {
    pub day: Date,
    pub drinks: Vec<Drinks>,
    pub more: bool,
    pub picked: String,
}

impl Counting {
    /// The day as it stands: what was recorded, or nothing counted yet.
    pub fn standing(days: &[Day], date: Date) -> Self {
        let recorded = days.iter().find(|day| day.date == date);
        Self {
            day: date,
            drinks: recorded
                .map(|day| day.drinking.counted().cloned().collect())
                .unwrap_or_default(),
            more: matches!(
                recorded.map(|day| &day.drinking),
                Some(Drinking::AtLeast(_))
            ),
            picked: String::new(),
        }
    }

    /// One more of `drink`, added to its line if it has one.
    fn add(mut self, drink: OwnDrink) -> Self {
        let line = self
            .drinks
            .iter_mut()
            .find(|drinks| drinks.name == drink.name && drinks.drink == drink.drink);
        match line {
            Some(drinks) => drinks.count = drinks.count.saturating_add(1),
            None => self.drinks.push(Drinks {
                count: NonZeroU8::MIN,
                name: drink.name,
                drink: drink.drink,
            }),
        }
        self
    }

    /// One fewer on a line, which goes when it reaches none.
    fn less(mut self, line: usize) -> Self {
        if let Some(drinks) = self.drinks.get_mut(line) {
            match NonZeroU8::new(drinks.count.get().saturating_sub(1)) {
                Some(count) => drinks.count = count,
                None => {
                    self.drinks.remove(line);
                }
            }
        }
        self
    }

    /// One more on a line.
    fn more_of(mut self, line: usize) -> Self {
        if let Some(drinks) = self.drinks.get_mut(line) {
            drinks.count = drinks.count.saturating_add(1);
        }
        self
    }

    /// The day as counted, or `None` with nothing counted.
    pub fn counted(&self) -> Option<Day> {
        let drinks = NonEmpty::new(self.drinks.clone()).ok()?;
        Some(Day {
            date: self.day,
            drinking: if self.more {
                Drinking::AtLeast(drinks)
            } else {
                Drinking::Exactly(drinks)
            },
        })
    }

    /// Whether the picker is on one of his own not yet named.
    fn naming(&self) -> bool {
        self.picked == OWN
    }
}

/// What a post asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The day drawn again as posted: nothing is counted or recorded.
    Redrawn(Counting),
    /// A drink added or taken off. Today saves it; another day draws it
    /// to be saved.
    Counted(Counting),
    /// The day as counted.
    Save(Day),
    /// Drinking that day, amount unknown.
    Unknown(Date),
    /// The day's entry removed.
    Remove(Date),
    /// One of his own drinks named, and one of it counted.
    Name(Counting, OwnDrink),
}

/// What a drink posts as, in the picker and on each line:
/// `568:52:pint of lager`, its millilitres, its strength in tenths and its
/// name.
fn encoded(name: &DrinkName, drink: Drink) -> String {
    format!(
        "{}:{}:{}",
        drink.volume.get(),
        drink.strength.as_tenths(),
        name
    )
}

/// The drink a posted value names.
fn decoded(value: &str) -> Option<OwnDrink> {
    let mut parts = value.splitn(3, ':');
    let volume = Millilitres::new(parts.next()?.parse::<NonZeroU16>().ok()?);
    let strength = Strength::from_tenths(parts.next()?.parse::<NonZeroU16>().ok()?).ok()?;
    let name = DrinkName::try_from(parts.next()?).ok()?;
    Some(OwnDrink {
        name,
        drink: Drink { volume, strength },
    })
}

/// One of his own, as the fields under the picker name it.
fn own(form: &[(String, String)]) -> Result<OwnDrink, Refusal> {
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
    };
    let name = DrinkName::try_from(field("own-name").unwrap_or_default())
        .map_err(|error| Refusal::of("own-name", error.to_string()))?;
    let volume: Millilitres = field("own-volume").unwrap_or_default().parse().map_err(
        |error: domain::drinking::InvalidVolume| Refusal::of("own-volume", error.to_string()),
    )?;
    let strength: Strength = field("own-strength").unwrap_or_default().parse().map_err(
        |error: domain::drinking::InvalidStrength| Refusal::of("own-strength", error.to_string()),
    )?;
    Ok(OwnDrink {
        name,
        drink: Drink { volume, strength },
    })
}

/// Read a post.
///
/// # Errors
///
/// What does not read, and the field it is about.
pub fn asked(form: &[(String, String)]) -> Result<Asked, Refusal> {
    let field = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.trim())
    };
    let typed = field("day").unwrap_or_default();
    let day: Date = typed
        .parse()
        .map_err(|_| Refusal::of("day", format!("{typed:?} is not a date")))?;
    let mut drinks = Vec::new();
    for (name, value) in form {
        if name != "line" {
            continue;
        }
        let (count, drink) = value
            .split_once('×')
            .and_then(|(count, drink)| Some((count.parse::<NonZeroU8>().ok()?, decoded(drink)?)))
            .ok_or_else(|| Refusal::whole(format!("{value:?} is not a count of a drink")))?;
        drinks.push(Drinks {
            count,
            name: drink.name,
            drink: drink.drink,
        });
    }
    let counting = Counting {
        day,
        drinks,
        more: field("more").is_some(),
        picked: field("drink").unwrap_or_default().to_owned(),
    };

    let act = field("act").unwrap_or_default();
    if let Some(line) = act.strip_prefix("less-").and_then(|line| line.parse().ok()) {
        return Ok(Asked::Counted(counting.less(line)));
    }
    if let Some(line) = act.strip_prefix("more-").and_then(|line| line.parse().ok()) {
        return Ok(Asked::Counted(counting.more_of(line)));
    }
    match act {
        "add" if counting.naming() => {
            let own = own(form)?;
            Ok(Asked::Name(counting.add(own.clone()), own))
        }
        "add" => {
            let drink = decoded(&counting.picked).ok_or_else(|| {
                Refusal::of("drink", format!("{:?} is not a drink", counting.picked))
            })?;
            Ok(Asked::Counted(counting.add(drink)))
        }
        "save" => counting.counted().map(Asked::Save).ok_or_else(|| {
            Refusal::whole(
                "nothing is counted: add a drink, or say the amount is unknown".to_owned(),
            )
        }),
        "unknown" => Ok(Asked::Unknown(day)),
        "remove" => Ok(Asked::Remove(day)),
        _ => Ok(Asked::Redrawn(counting)),
    }
}

/// A save the use case refused, where the page says it.
pub fn refusal(error: &DrinkingError) -> Refusal {
    match error {
        DrinkingError::NotYet { .. } => Refusal::of("day", error.to_string()),
        DrinkingError::Store(_) => Refusal::whole(error.to_string()),
    }
}

/// What was drunk, as the list says it.
fn amount(drinking: &Drinking) -> String {
    let units = drinking.units().unwrap_or_default();
    match drinking {
        Drinking::AmountUnknown => "amount unknown".to_owned(),
        Drinking::AtLeast(_) => format!("at least {units} units"),
        Drinking::Exactly(_) => format!("{units} units"),
    }
}

/// The page Drinking opens on: today, saved a drink at a time, and the
/// days reported.
pub fn opening(
    days: &[Day],
    choices: &Choices,
    today: Date,
    counting: &Counting,
    refused: Option<&Refusal>,
) -> Markup {
    input::page(
        HERE,
        TITLE,
        &html! {
            section.card #today {
                h2 { "Today" }
                form.input method="post" action=(HERE) hx-post=(HERE) hx-target="main" hx-select="main" hx-swap="outerHTML" {
                    input type="hidden" name="day" value=(counting.day);
                    @if counting.more { input type="hidden" name="more" value="on"; }
                    @for (line, drinks) in counting.drinks.iter().enumerate() { (line_of(line, drinks)) }
                    (total(counting))
                    (counter(choices, counting, refused))
                    (input::whole(refused))
                }
            }
            section.card {
                h2 { "Reported" }
                @let past: Vec<_> = days.iter().filter(|day| day.date != today).take(LISTED).collect();
                @if past.is_empty() {
                    p.sub { "No other day has an entry." }
                }
                @for day in past {
                    a.day href=(format!("{DAY}?day={}", day.date)) {
                        span { (detail::day(day.date, today)) }
                        span.amount { (amount(&day.drinking)) }
                    }
                }
                a.button href=(DAY) { "Add another day" }
            }
        },
    )
}

/// Another day: counted and saved, or the amount unknown; and once
/// reported, some not counted, or removed.
pub fn another(
    days: &[Day],
    choices: &Choices,
    today: Date,
    counting: &Counting,
    refused: Option<&Refusal>,
) -> Markup {
    let recorded = days.iter().find(|day| day.date == counting.day);
    let unknown = recorded.is_some_and(|day| matches!(day.drinking, Drinking::AmountUnknown));
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(TITLE, &[])) }
            body {
                main {
                    (page::bar(HERE))
                    header.detail {
                        a href=(HERE) aria-label=(TITLE) { "‹" }
                        div { h1 { "Another day" } }
                    }
                    section.card {
                        form.input method="post" action=(DAY) hx-post=(DAY) hx-target="main" hx-select="main" hx-swap="outerHTML" {
                            button.unseen type="submit" name="act" value="redraw" tabindex="-1" aria-hidden="true" {}
                            (input::field("day", "day", "Date", &html! {
                                input id="day" type="date" name="day" value=(counting.day) max=(today)
                                    hx-get=(DAY) hx-trigger="change" hx-target="main" hx-select="main" hx-swap="outerHTML" hx-include="this";
                            }, refused))
                            @if unknown {
                                p.held { "Amount unknown" }
                            }
                            @for (line, drinks) in counting.drinks.iter().enumerate() { (line_of(line, drinks)) }
                            (counter(choices, counting, refused))
                            @if recorded.is_some_and(|recorded| !matches!(recorded.drinking, Drinking::AmountUnknown)) {
                                label.check {
                                    input type="checkbox" name="more" checked[counting.more];
                                    "Some not counted"
                                }
                            } @else if counting.more {
                                input type="hidden" name="more" value="on";
                            }
                            (total(counting))
                            (input::whole(refused))
                            // **Each button only where it does something**: Save
                            // once a drink is counted, Amount unknown while none
                            // is and the day is not already unknown, Remove once
                            // the day has an entry.
                            @if counting.drinks.is_empty() {
                                @if !unknown {
                                    button type="submit" name="act" value="unknown" { "Amount unknown" }
                                }
                            } @else {
                                button type="submit" name="act" value="save" { "Save" }
                            }
                            @if recorded.is_some() {
                                button.quiet type="submit" name="act" value="remove" { "Remove" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn line_value(drinks: &Drinks) -> String {
    format!("{}×{}", drinks.count, encoded(&drinks.name, drinks.drink))
}

/// A line, with a step down and up, its count carried in the form.
fn line_of(line: usize, drinks: &Drinks) -> Markup {
    html! {
        input type="hidden" name="line" value=(line_value(drinks));
        div.row.drinks.stepped {
            span { (drinks.count) " × " (drinks.name) }
            button.remove type="submit" name="act" value=(format!("less-{line}")) aria-label=(format!("One fewer {}", drinks.name)) { "−" }
            button.remove type="submit" name="act" value=(format!("more-{line}")) aria-label=(format!("One more {}", drinks.name)) { "+" }
        }
    }
}

fn total(counting: &Counting) -> Markup {
    html! {
        @if let Some(day) = counting.counted() {
            p.foot { (amount(&day.drinking)) }
        }
    }
}

/// The picker and its button on one row, and the fields for one of his own
/// when that is picked.
fn counter(choices: &Choices, counting: &Counting, refused: Option<&Refusal>) -> Markup {
    let group = |label: &str, drinks: &[OwnDrink]| {
        html! {
            @if !drinks.is_empty() {
                optgroup label=(label) {
                    @for drink in drinks {
                        @let value = encoded(&drink.name, drink.drink);
                        option value=(value) selected[value == counting.picked] {
                            (drink.name) " (" (drink.drink.volume) ", " (drink.drink.strength) ")"
                        }
                    }
                }
            }
        }
    };
    let naming = counting.naming()
        || refused
            .is_some_and(|refusal| refusal.field.is_some_and(|field| field.starts_with("own-")));
    html! {
        div.pick {
            select name="drink" aria-label="Drink" hx-post="" hx-trigger="change" hx-target="main" hx-select="main" hx-swap="outerHTML" hx-vals=r#"{"act":"redraw"}"# {
                (group("Recent", &choices.recent))
                (group("Yours", &choices.own))
                (group("NHS", &choices.standard))
                option value=(OWN) selected[naming] { "Your own…" }
            }
            button type="submit" name="act" value="add" { "Add" }
        }
        @if naming {
            (input::field("own-name", "own-name", "Name", &html! {
                input id="own-name" type="text" name="own-name" autocomplete="off";
            }, refused))
            div.own {
                (input::field("own-volume", "own-volume", "ml", &html! {
                    input id="own-volume" type="number" name="own-volume" min="1" inputmode="numeric";
                }, refused))
                (input::field("own-strength", "own-strength", "ABV %", &html! {
                    input id="own-strength" type="text" name="own-strength" inputmode="decimal";
                }, refused))
            }
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    input::unreadable(HERE, TITLE, error)
}

#[cfg(test)]
mod tests {
    use application::drinking::Choices;
    use domain::drinking::{Day, Drinking, StandardServing};
    use jiff::civil::date;

    use super::{Asked, Counting, another, asked, encoded, opening};

    fn posted(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn pint() -> String {
        let pint = StandardServing::PintHigherStrength.as_own();
        encoded(&pint.name, pint.drink)
    }

    fn choices() -> Choices {
        Choices {
            recent: Vec::new(),
            own: Vec::new(),
            standard: StandardServing::ALL
                .into_iter()
                .map(StandardServing::as_own)
                .collect(),
        }
    }

    fn nothing(on: jiff::civil::Date) -> Counting {
        Counting {
            day: on,
            drinks: Vec::new(),
            more: false,
            picked: String::new(),
        }
    }

    /// **Adding a drink twice is two of it**, on one line.
    #[test]
    fn adding_a_drink_again_counts_it_again() {
        let line = format!("1×{}", pint());
        let twice = asked(&posted(&[
            ("day", "2026-11-14"),
            ("line", &line),
            ("drink", &pint()),
            ("act", "add"),
        ]))
        .expect("an add reads");
        let Asked::Counted(twice) = twice else {
            panic!("an add counts: {twice:?}")
        };

        assert_eq!(twice.drinks.len(), 1);
        assert_eq!(twice.drinks[0].count.get(), 2);
    }

    /// **"Some not counted" saves at least what was counted.**
    #[test]
    fn some_not_counted_saves_at_least() {
        let line = format!("3×{}", pint());
        let saved = asked(&posted(&[
            ("day", "2026-11-14"),
            ("line", &line),
            ("more", "on"),
            ("act", "save"),
        ]))
        .expect("a save reads");
        let Asked::Save(day) = saved else {
            panic!("a save with drinks saves: {saved:?}")
        };

        assert!(matches!(day.drinking, Drinking::AtLeast(_)));
        assert_eq!(
            day.drinking.units().map(|units| units.to_string()),
            Some("8.9".to_owned())
        );
    }

    /// **Saving nothing counted is refused**, rather than read as no alcohol.
    #[test]
    fn saving_nothing_is_refused() {
        let refused = asked(&posted(&[("day", "2026-11-14"), ("act", "save")]));
        assert!(refused.is_err(), "{refused:?}");
    }

    /// **The last one off a line takes the line.**
    #[test]
    fn one_fewer_of_one_removes_the_line() {
        let line = format!("1×{}", pint());
        let fewer = asked(&posted(&[
            ("day", "2026-11-14"),
            ("line", &line),
            ("act", "less-0"),
        ]));
        assert_eq!(fewer, Ok(Asked::Counted(nothing(date(2026, 11, 14)))));
    }

    /// **Adding one of his own names it and counts one**, and asks for its
    /// name when there is none.
    #[test]
    fn adding_ones_own_names_it() {
        let named = asked(&posted(&[
            ("day", "2026-11-14"),
            ("drink", "own"),
            ("own-name", "house red"),
            ("own-volume", "175"),
            ("own-strength", "13.5"),
            ("act", "add"),
        ]))
        .expect("a named drink reads");
        let Asked::Name(counting, own) = named else {
            panic!("an own drink is named: {named:?}")
        };
        assert_eq!(own.name.as_str(), "house red");
        assert_eq!(counting.drinks.len(), 1);

        let unnamed = asked(&posted(&[
            ("day", "2026-11-14"),
            ("drink", "own"),
            ("act", "add"),
        ]));
        assert!(
            matches!(&unnamed, Err(refusal) if refusal.field == Some("own-name")),
            "{unnamed:?}"
        );
    }

    /// **Today offers every NHS serving and has no Save**: each drink is
    /// saved as it is added.
    #[test]
    fn today_offers_the_nhs_servings_and_saves_as_it_goes() {
        let today = date(2026, 11, 19);
        let shown = opening(&[], &choices(), today, &nothing(today), None).into_string();

        for serving in StandardServing::ALL {
            assert!(
                shown.contains(serving.name()),
                "{} is not offered",
                serving.name()
            );
        }
        assert!(!shown.contains(r#"value="save""#), "{shown}");
        assert!(!shown.contains(r#"value="unknown""#), "{shown}");
    }

    /// **A new day is saved or unknown; one already reported can
    /// be marked as not all counted, or removed.**
    #[test]
    fn a_past_day_offers_what_its_use_needs() {
        let today = date(2026, 11, 19);
        let on = date(2026, 10, 28);
        let fresh = another(&[], &choices(), today, &nothing(on), None).into_string();
        assert!(fresh.contains(r#"value="unknown""#), "{fresh}");
        assert!(!fresh.contains(r#"value="save""#), "{fresh}");
        assert!(!fresh.contains(r#"value="remove""#), "{fresh}");
        assert!(!fresh.contains("Some not counted"), "{fresh}");

        let line = format!("2×{}", pint());
        let Ok(Asked::Save(reported)) = asked(&posted(&[
            ("day", "2026-10-28"),
            ("line", &line),
            ("act", "save"),
        ])) else {
            panic!("a save reads")
        };
        let standing = Counting::standing(std::slice::from_ref(&reported), on);
        let again = another(&[reported], &choices(), today, &standing, None).into_string();
        assert!(again.contains(r#"value="remove""#), "{again}");
        assert!(again.contains("Some not counted"), "{again}");
        assert!(!again.contains(r#"value="unknown""#), "{again}");

        let unknown = Day {
            date: on,
            drinking: Drinking::AmountUnknown,
        };
        let standing = Counting::standing(std::slice::from_ref(&unknown), on);
        let shown = another(&[unknown], &choices(), today, &standing, None).into_string();
        assert!(
            !shown.contains(r#"value="save""#),
            "nothing counted to save: {shown}"
        );
        assert!(
            !shown.contains(r#"value="unknown""#),
            "already unknown: {shown}"
        );
        assert!(shown.contains(r#"value="remove""#), "{shown}");
    }
}
