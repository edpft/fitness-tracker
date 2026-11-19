//! The illness page, under Report (#873): ill from a date, or better today,
//! as `fitness schedule` records it in the terminal. Moved from the schedule
//! page, which is for availability.

use application::ScheduleError;
use domain::schedule::Diary;
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    detail,
    input::{self, Refusal},
};

pub const TITLE: &str = "Illness";

pub const HERE: &str = "/illness";

/// What a post asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked {
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
    if field("act") == Some("better") {
        return Ok(Asked::Better);
    }
    let typed = field("from").unwrap_or_default();
    typed
        .parse()
        .map(Asked::Ill)
        .map_err(|_| Refusal::of("from", format!("{typed:?} is not a date")))
}

/// A post the use case refused, where the page says it.
pub fn refusal(error: &ScheduleError) -> Refusal {
    Refusal::whole(error.to_string())
}

/// The page: the illness open, or a date to be ill from.
pub fn page(diary: &Diary, today: Date, refused: Option<&Refusal>) -> Markup {
    input::page(
        HERE,
        TITLE,
        &html! {
            section.card {
                form.input method="post" action=(HERE) {
                    @match diary.open_illness() {
                        Some(open) => {
                            p.held { "Ill from " (detail::day(open.from(), today)) }
                            (input::whole(refused))
                            button type="submit" name="act" value="better" { "Better" }
                        }
                        None => {
                            (input::field("ill-from", "from", "Ill from", &html! {
                                input id="ill-from" type="date" name="from" value=(today);
                            }, refused))
                            (input::whole(refused))
                            button type="submit" name="act" value="ill" { "Ill" }
                        }
                    }
                }
            }
        },
    )
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    input::unreadable(HERE, TITLE, error)
}
