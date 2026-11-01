//! The schedule page (#671): weekly availability, a calendar week altered from
//! it, and illness, as `fitness schedule` records them in the terminal.
//!
//! **One grid for both.** Weekly availability is a day per row and a part of
//! the day per column, a shorter slot a circle and a longer one a lozenge. A
//! week commencing a date is the same grid, starting from the week as it
//! stands, and tapping a cell steps it free → shorter → longer → free. Whether
//! the change is for that week or from then on is chosen on saving.
//!
//! **A tap is a post that records nothing.** The server steps the cell and
//! draws the page again, so what the rules give each slot is shown for the
//! grid as tapped, and the page holds no rule of its own about either.

use std::collections::BTreeMap;

use application::{ScheduleError, schedule::days_of};
use domain::{
    planner::{Limits, allocations, commencing},
    schedule::{Diary, PartOfDay, SlotLength, TrainingSlot},
};
use jiff::civil::{Date, Weekday};
use maud::{Markup, html};

use crate::{
    detail,
    input::{self, Refusal},
};

const TITLE: &str = "Schedule";

pub const HERE: &str = "/schedule";

/// The weekdays, Monday first, with the names the grid writes them under.
const WEEKDAYS: [(Weekday, &str); 7] = [
    (Weekday::Monday, "Mon"),
    (Weekday::Tuesday, "Tue"),
    (Weekday::Wednesday, "Wed"),
    (Weekday::Thursday, "Thu"),
    (Weekday::Friday, "Fri"),
    (Weekday::Saturday, "Sat"),
    (Weekday::Sunday, "Sun"),
];

/// What a slot posts under: `friday-evening`.
fn key(slot: TrainingSlot) -> String {
    format!("{:?}-{}", slot.weekday, slot.part.as_str()).to_lowercase()
}

/// The slot a posted key names.
fn slot(key: &str) -> Option<TrainingSlot> {
    let (weekday, part) = key.split_once('-')?;
    let weekday = WEEKDAYS
        .iter()
        .map(|(weekday, _)| *weekday)
        .find(|candidate| format!("{candidate:?}").eq_ignore_ascii_case(weekday))?;
    let part = PartOfDay::try_from(part.to_owned()).ok()?;
    Some(TrainingSlot::new(weekday, part))
}

/// A week as it was tapped: its Monday, every slot and its length, and the
/// reason typed so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tapped {
    pub monday: Date,
    pub slots: BTreeMap<TrainingSlot, SlotLength>,
    pub reason: String,
}

impl Tapped {
    /// The week as it stands in the diary, with no reason yet.
    fn standing(diary: &Diary, monday: Date) -> Self {
        Self {
            monday,
            slots: days_of(monday)
                .into_iter()
                .flat_map(|date| diary.free_on(date, false))
                .collect(),
            reason: String::new(),
        }
    }

    /// The same week with one cell stepped: free → shorter → longer → free.
    fn tapped(mut self, slot: TrainingSlot) -> Self {
        match self.slots.get(&slot) {
            None => {
                self.slots.insert(slot, SlotLength::Shorter);
            }
            Some(SlotLength::Shorter) => {
                self.slots.insert(slot, SlotLength::Longer);
            }
            Some(SlotLength::Longer) => {
                self.slots.remove(&slot);
            }
        }
        self
    }
}

/// What a post asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The week drawn again as posted, with any tap or clear applied: nothing
    /// is recorded.
    Redrawn(Tapped),
    /// The week as tapped, for that week only.
    ThisWeek(Tapped),
    /// The week as tapped, as the weekly availability from its Monday.
    FromNowOn(Tapped),
    /// Ill from a date.
    Ill(Date),
    /// Better today.
    Better,
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
    let act = field("act").unwrap_or_default();
    match act {
        "ill" => {
            let typed = field("from").unwrap_or_default();
            return typed
                .parse()
                .map(Asked::Ill)
                .map_err(|_| Refusal::of("from", format!("{typed:?} is not a date")));
        }
        "better" => return Ok(Asked::Better),
        _ => {}
    }

    let typed = field("week").unwrap_or_default();
    let monday = typed
        .parse()
        .map(commencing)
        .map_err(|_| Refusal::of("week", format!("{typed:?} is not a date")))?;
    let mut slots = BTreeMap::new();
    for (name, value) in form {
        if let Some(slot) = slot(name)
            && let Ok(length) = SlotLength::try_from(value.as_str())
        {
            slots.insert(slot, length);
        }
    }
    let tapped = Tapped {
        monday,
        slots,
        reason: field("reason").unwrap_or_default().to_owned(),
    };

    if let Some(slot) = field("tap").and_then(slot) {
        return Ok(Asked::Redrawn(tapped.tapped(slot)));
    }
    Ok(match act {
        "clear" => Asked::Redrawn(Tapped {
            slots: BTreeMap::new(),
            ..tapped
        }),
        "this-week" => Asked::ThisWeek(tapped),
        "from-now-on" => Asked::FromNowOn(tapped),
        _ => Asked::Redrawn(tapped),
    })
}

/// A save the use case refused, where the page says it.
pub fn refusal(error: &ScheduleError) -> Refusal {
    match error {
        ScheduleError::NoReason => Refusal::of("reason", error.to_string()),
        ScheduleError::NoIllnessOpen | ScheduleError::Illness(_) => {
            Refusal::of("illness", error.to_string())
        }
        _ => Refusal::of("week", error.to_string()),
    }
}

/// The page: weekly availability today, the week commencing `monday` as
/// tapped or as it stands, and illness.
pub fn page(
    diary: &Diary,
    today: Date,
    monday: Date,
    tapped: Option<&Tapped>,
    refused: Option<&Refusal>,
) -> Markup {
    let tapped = tapped.map_or_else(|| Tapped::standing(diary, monday), Clone::clone);
    input::page(
        HERE,
        TITLE,
        &html! {
            script src="/htmx.min.js" {}
            (ordinarily(diary, today))
            (week(diary, today, &tapped, refused))
            (illness(diary, today, refused))
        },
    )
}

/// The key under a grid.
fn key_line() -> Markup {
    html! {
        div.key {
            span { i.shorter {} "Shorter" }
            span { i.longer {} "Longer" }
        }
    }
}

/// The class a cell's mark takes.
const fn mark(length: Option<SlotLength>) -> &'static str {
    match length {
        None => "",
        Some(SlotLength::Shorter) => "shorter",
        Some(SlotLength::Longer) => "longer",
    }
}

/// Weekly availability in force today, as it is: not tapped here.
fn ordinarily(diary: &Diary, today: Date) -> Markup {
    html! {
        section.card {
            @match diary.pattern_on(today) {
                None => {
                    h2 { "Weekly availability" }
                    p { "None is recorded yet." }
                }
                Some(pattern) => {
                    div.latest { div.head {
                        h2 { "Weekly availability" }
                        span { "from " (detail::day(pattern.from(), today)) }
                    } }
                    div.slots {
                        span {}
                        @for part in PartOfDay::ALL { span.part { (title(part.as_str())) } }
                        @for (weekday, name) in WEEKDAYS {
                            span.day { (name) }
                            @for part in PartOfDay::ALL {
                                @let length = pattern.slots().get(&TrainingSlot::new(weekday, *part)).copied();
                                span.cell.(mark(length)) {}
                            }
                        }
                    }
                    (key_line())
                }
            }
        }
    }
}

/// A word with its first letter in capitals.
fn title(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// The week commencing a date: the grid as tapped, what the rules give each
/// slot of it, and the two ways to save it.
fn week(diary: &Diary, today: Date, tapped: &Tapped, refused: Option<&Refusal>) -> Markup {
    let days = days_of(tapped.monday);
    let step = |by: i64| {
        tapped
            .monday
            .checked_add(jiff::Span::new().weeks(by))
            .unwrap_or(tapped.monday)
    };
    let changes: Vec<_> = diary
        .alterations()
        .iter()
        .filter(|alteration| {
            days.iter().any(|date| alteration.covers(*date)) && alteration.slots().is_some()
        })
        .collect();
    let given = allocations(&tapped.slots, Limits::STATED);
    let whole = refused
        .filter(|refusal| refusal.field == Some("week"))
        .map(|refusal| refusal.reason.as_str());
    html! {
        section.card #week {
            div.weekhead {
                a href=(format!("{HERE}?week={}", step(-1))) aria-label="The week before" { "‹" }
                h2 { "Week commencing " (detail::day(tapped.monday, today)) }
                a href=(format!("{HERE}?week={}", step(1))) aria-label="The week after" { "›" }
            }
            @for change in &changes {
                p.foot {
                    (detail::day(change.start(), today))
                    @if change.days().get() > 1 { " to " (detail::day(change.last(), today)) }
                    ": " (change.reason())
                }
            }
            form.input method="post" action=(HERE) hx-post=(HERE) hx-target="main" hx-select="main" hx-swap="outerHTML" {
                // **What the return key presses**: the first button in a form,
                // which would otherwise tap Monday morning.
                button.unseen type="submit" name="act" value="redraw" tabindex="-1" aria-hidden="true" {}
                input type="hidden" name="week" value=(tapped.monday);
                @for (slot, length) in &tapped.slots {
                    input type="hidden" name=(key(*slot)) value=(length.as_str());
                }
                div.slots {
                    span {}
                    @for part in PartOfDay::ALL { span.part { (title(part.as_str())) } }
                    @for (date, (weekday, name)) in days.iter().zip(WEEKDAYS) {
                        @let past = *date < today;
                        span.day.past[past] { (name) small { (date.day()) } }
                        @for part in PartOfDay::ALL {
                            @let slot = TrainingSlot::new(weekday, *part);
                            @let length = tapped.slots.get(&slot).copied();
                            @let ordinary = diary.free_on(*date, true).into_iter().find(|(free, _)| *free == slot).map(|(_, length)| length);
                            @let gone = if length.is_none() { ordinary } else { None };
                            button.cell.(mark(length)).past[past].changed[length != ordinary]
                                type="submit" name="tap" value=(key(slot))
                                aria-label=(format!("{name} {date} {}: {}", part.as_str(), length.map_or("free", SlotLength::as_str))) {
                                @if let Some(gone) = gone { i.gone.(mark(Some(gone))) {} }
                            }
                        }
                    }
                }
                (key_line())
                @if !given.is_empty() {
                    div.given {
                        @for (slot, allocation) in &given {
                            div.row {
                                span.on { (WEEKDAYS.iter().find(|(weekday, _)| *weekday == slot.weekday).map_or("", |(_, name)| name)) " " (slot.part) }
                                span {
                                    (allocation.discipline)
                                    @if let Some(role) = allocation.role { ", " (role) }
                                }
                            }
                        }
                    }
                }
                (input::field("week-reason", "reason", "Why", &html! {
                    input id="week-reason" type="text" name="reason" value=(tapped.reason) autocomplete="off";
                }, refused))
                @if let Some(reason) = whole { p.reason { (reason) } }
                div.actions {
                    button.quiet type="submit" name="act" value="clear" { "Clear" }
                    button type="submit" name="act" value="this-week" { "This week" }
                }
                button.quiet type="submit" name="act" value="from-now-on" { "From now on" }
            }
        }
    }
}

/// Ill from a date, or better today.
fn illness(diary: &Diary, today: Date, refused: Option<&Refusal>) -> Markup {
    let whole = refused
        .filter(|refusal| refusal.field == Some("illness"))
        .map(|refusal| refusal.reason.as_str());
    html! {
        section.card {
            h2 { "Illness" }
            form.input method="post" action=(HERE) {
                @match diary.open_illness() {
                    Some(open) => {
                        p.held { "Ill from " (detail::day(open.from(), today)) }
                        @if let Some(reason) = whole { p.reason { (reason) } }
                        button type="submit" name="act" value="better" { "Better" }
                    }
                    None => {
                        (input::field("ill-from", "from", "Ill from", &html! {
                            input id="ill-from" type="date" name="from" value=(today);
                        }, refused))
                        @if let Some(reason) = whole { p.reason { (reason) } }
                        button type="submit" name="act" value="ill" { "Ill" }
                    }
                }
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
    use std::collections::BTreeMap;

    use application::ScheduleError;
    use domain::{
        normalised::OperatorZone,
        schedule::{Diary, PartOfDay, SlotLength, TrainingPattern, TrainingSlot},
    };
    use jiff::civil::{Weekday, date};

    use super::{Asked, Tapped, asked, page, refusal};

    fn posted(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn evening(weekday: Weekday) -> TrainingSlot {
        TrainingSlot::new(weekday, PartOfDay::Evening)
    }

    /// **A tap steps a cell free → shorter → longer → free**, and keeps every
    /// other cell and the reason as posted.
    #[test]
    fn a_tap_steps_one_cell() {
        let step = |length: Option<&str>| {
            let mut fields = vec![
                ("week", "2026-12-16"),
                ("monday-evening", "shorter"),
                ("reason", "Christmas"),
                ("tap", "friday-evening"),
            ];
            if let Some(length) = length {
                fields.push(("friday-evening", length));
            }
            asked(&posted(&fields)).expect("a tap reads")
        };
        let after = |length: Option<SlotLength>| {
            let mut slots = BTreeMap::from([(evening(Weekday::Monday), SlotLength::Shorter)]);
            if let Some(length) = length {
                slots.insert(evening(Weekday::Friday), length);
            }
            Asked::Redrawn(Tapped {
                monday: date(2026, 12, 14),
                slots,
                reason: "Christmas".to_owned(),
            })
        };

        assert_eq!(step(None), after(Some(SlotLength::Shorter)));
        assert_eq!(step(Some("shorter")), after(Some(SlotLength::Longer)));
        assert_eq!(step(Some("longer")), after(None));
    }

    /// **Clear empties the week and records nothing**; the reason stays.
    #[test]
    fn clear_empties_the_week() {
        let cleared = asked(&posted(&[
            ("week", "2026-12-21"),
            ("monday-evening", "shorter"),
            ("reason", "Christmas"),
            ("act", "clear"),
        ]));
        assert_eq!(
            cleared,
            Ok(Asked::Redrawn(Tapped {
                monday: date(2026, 12, 21),
                slots: BTreeMap::new(),
                reason: "Christmas".to_owned(),
            }))
        );
    }

    /// **A save with no reason comes back as it was tapped**, the reason
    /// under *Why*, and the removed slot a dashed outline of its shape.
    #[test]
    fn a_refused_week_keeps_the_grid_as_tapped() {
        let ordinary = BTreeMap::from([
            (evening(Weekday::Friday), SlotLength::Shorter),
            (
                TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
                SlotLength::Longer,
            ),
        ]);
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a real zone");
        let diary = Diary::new(
            vec![TrainingPattern::new(date(2026, 8, 25), zone, ordinary)],
            Vec::new(),
        );
        let tapped = Tapped {
            monday: date(2026, 12, 14),
            slots: BTreeMap::new(),
            reason: String::new(),
        };
        let refused = refusal(&ScheduleError::NoReason);

        let shown = page(
            &diary,
            date(2026, 11, 1),
            tapped.monday,
            Some(&tapped),
            Some(&refused),
        )
        .into_string();

        assert!(shown.contains("Week commencing 14 Dec"), "{shown}");
        assert!(!shown.contains(r#"name="friday-evening""#), "{shown}");
        assert!(shown.contains(r#"<i class="gone shorter">"#), "{shown}");
        assert!(shown.contains(r#"<i class="gone longer">"#), "{shown}");
        let why = shown.find(">Why<").expect("the reason field");
        let reason = shown.find("an alteration says why").expect("the refusal");
        let save = shown.find(">This week<").expect("the save");
        assert!(why < reason && reason < save, "{shown}");
    }
}
