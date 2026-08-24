//! The document a week is authored from.
//!
//! **A format, and formats live out here (§ 21).** `domain::schedule` knows
//! nothing about TOML, and `application`'s ports take `Schedule` and `Patch` —
//! so this is the only thing that has an opinion about how a week is written
//! down, and a second way of writing one would be a second module rather than a
//! change to either of those.
//!
//! **A slot is written the way it is said.** `"monday evening"`, not a table
//! with a weekday key and a part key: the vocabulary is closed at both ends, the
//! pair is meaningless split up, and § 8's rule that the operator's words come
//! first applies to their own diary more than to anything else here.

use std::{collections::BTreeSet, num::NonZeroU8};

use domain::{
    gym::OperatorZone,
    schedule::{PartOfDay, Patch, Schedule, Slot},
};
use jiff::civil::{Date, Weekday};

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("the schedule document could not be read: {0}")]
    Unreadable(#[from] std::io::Error),
    #[error("the schedule document is not valid TOML: {0}")]
    NotToml(#[from] toml::de::Error),
    #[error("{field}: {detail}")]
    Invalid { field: String, detail: String },
}

fn invalid(field: &str, detail: impl std::fmt::Display) -> DocumentError {
    DocumentError::Invalid {
        field: field.to_owned(),
        detail: detail.to_string(),
    }
}

/// `"monday evening"` into a slot.
///
/// Both halves are required and neither has a default. A bare weekday would
/// have to mean "all day" or "some part of it", and the operator's week is
/// planned in halves of a day rather than in days.
fn slot(field: &str, written: &str) -> Result<Slot, DocumentError> {
    let lowered = written.trim().to_lowercase();
    let mut words = lowered.split_whitespace();
    let (Some(day), Some(part), None) = (words.next(), words.next(), words.next()) else {
        return Err(invalid(
            field,
            format_args!(
                "{written:?} is not a weekday and a part of a day, as in \"monday evening\""
            ),
        ));
    };

    let weekday = match day {
        "monday" => Weekday::Monday,
        "tuesday" => Weekday::Tuesday,
        "wednesday" => Weekday::Wednesday,
        "thursday" => Weekday::Thursday,
        "friday" => Weekday::Friday,
        "saturday" => Weekday::Saturday,
        "sunday" => Weekday::Sunday,
        other => return Err(invalid(field, format_args!("{other:?} is not a weekday"))),
    };

    let part = PartOfDay::try_from(part.to_owned()).map_err(|error| invalid(field, error))?;
    Ok(Slot::new(weekday, part))
}

fn slots(field: &str, written: &[String]) -> Result<BTreeSet<Slot>, DocumentError> {
    written
        .iter()
        .map(|one| slot(field, one))
        .collect::<Result<BTreeSet<_>, _>>()
}

fn date(field: &str, written: &str) -> Result<Date, DocumentError> {
    written.parse().map_err(|_| {
        invalid(
            field,
            format_args!("{written:?} is not a date, as in 2026-09-14"),
        )
    })
}

fn zone(field: &str, written: &str) -> Result<OperatorZone, DocumentError> {
    OperatorZone::try_from(written.to_owned()).map_err(|error| invalid(field, error))
}

// --- The document's shapes --------------------------------------------------

#[derive(serde::Deserialize)]
pub struct Document {
    week: WeekSection,
    /// **Optional, and a document may be nothing but departures.**
    ///
    /// A departure is a fact about dates rather than about which ordinary week
    /// was in force when it was recorded, so recording one does not restate the
    /// week. Not every one is a holiday — a course, a visitor or a late finish
    /// all change a week without being a trip.
    #[serde(default)]
    patch: Vec<PatchSection>,
}

#[derive(serde::Deserialize)]
struct WeekSection {
    /// In force from this date. No end: a week has a successor, not a finish.
    from: String,
    zone: String,
    /// **An empty list is a real week**: a period with no room to train at all
    /// is a thing that happens, and refusing to record it would mean pretending
    /// otherwise.
    slots: Vec<String>,
}

#[derive(serde::Deserialize)]
struct PatchSection {
    start: String,
    days: NonZeroU8,
    /// Absent leaves the zone alone. Present is being somewhere else.
    zone: Option<String>,
    /// **Absent and empty are different facts.** Absent means the ordinary week
    /// stands — away, training as usual. `slots = []` means no room to train at
    /// all, which is the hard case and the one 14 September needs.
    slots: Option<Vec<String>>,
    /// Why. An override nobody explained is unreadable six months later.
    reason: String,
}

impl Document {
    /// # Errors
    ///
    /// [`DocumentError`] if the file is missing, is not TOML, or holds a value
    /// the vocabulary does not recognise.
    pub fn read(path: &std::path::Path) -> Result<Self, DocumentError> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// The ordinary week, in domain terms.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] for a value the vocabulary does not recognise.
    pub fn week(&self) -> Result<Schedule, DocumentError> {
        Ok(Schedule::new(
            date("week.from", &self.week.from)?,
            zone("week.zone", &self.week.zone)?,
            slots("week.slots", &self.week.slots)?,
        ))
    }

    /// The departures from the ordinary week, in domain terms.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] for a value the vocabulary does not recognise, or a
    /// patch that explains nothing.
    pub fn patches(&self) -> Result<Vec<Patch>, DocumentError> {
        self.patch
            .iter()
            .enumerate()
            .map(|(at, patch)| {
                let field = |name: &str| format!("patch[{at}].{name}");

                if patch.reason.trim().is_empty() {
                    return Err(invalid(
                        &field("reason"),
                        "a patch with no reason is unreadable six months later",
                    ));
                }

                let stated = patch
                    .slots
                    .as_ref()
                    .map(|written| slots(&field("slots"), written))
                    .transpose()?;

                Ok(Patch::new(
                    date(&field("start"), &patch.start)?,
                    patch.days,
                    patch
                        .zone
                        .as_deref()
                        .map(|written| zone(&field("zone"), written))
                        .transpose()?,
                    stated,
                    patch.reason.trim().to_owned(),
                ))
            })
            .collect()
    }
}
