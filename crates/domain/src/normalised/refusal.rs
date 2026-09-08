//! What a landing record asserted that the domain will not express.
//!
//! § 37: partial data is recorded as partial. That is not satisfied by
//! translating what fits — the grammatical part translates, the ungrammatical
//! part does not, and translation never guesses which repair was meant.
//!
//! A refusal is a value with a place and a reason, not a formatted sentence.
//! The prose satisfies a reader and defeats every other use: what has to be
//! assertable is that the refusals are *exactly* a known set, which is a query
//! over reasons and not a grep over text.
//!
//! The three kinds are the point of recording them at all. Each refusal is data
//! to fix at source, a limitation to declare, or a gap in the model — a model
//! that cannot hold a genuine case needs refining, whereas a model that rejects
//! a wrong record is working, and telling them apart is unavailable if the
//! refusal is a stack trace or a dropped row.

use std::fmt;

use crate::gym::exercise::Exercise;
use crate::landing::{LandingRecordId, SourceRecordId};

/// Where in a record the refused thing sat.
///
/// Positional, because Hevy publishes no identity below the workout — sets and
/// exercises carry only an index. That index moves under insertion or
/// reordering, which is a real limitation and is why an overlay anchored below
/// the workout is an open question; for a refusal it is enough, because a
/// refusal is read against the derivation that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefusalLocus {
    /// The record as a whole.
    Record,
    /// One exercise entry within it.
    Entry { entry: u32 },
    /// One set of one entry.
    Set { entry: u32, set: u32 },
    /// A grouping, named by what the source called it.
    Grouping { group: u32 },
}

impl fmt::Display for RefusalLocus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Record => f.write_str("the record"),
            Self::Entry { entry } => write!(f, "exercise {entry}"),
            Self::Set { entry, set } => write!(f, "exercise {entry}, set {set}"),
            Self::Grouping { group } => write!(f, "superset {group}"),
        }
    }
}

/// What sort of problem a refusal reports, and therefore what to do about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefusalKind {
    /// The event is genuine and the recording is wrong. Fix it at source, or in
    /// the edit overlay when there is one.
    WrongData,
    /// The domain has declined to model this, knowingly. Nothing to fix.
    DeclaredLimitation,
    /// A real case the model does not hold yet. Evidence for a later feature.
    Unmodelled,
}

impl RefusalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WrongData => "wrong data",
            Self::DeclaredLimitation => "declared limitation",
            Self::Unmodelled => "unmodelled",
        }
    }
}

impl fmt::Display for RefusalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Why something did not translate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefusalReason {
    /// Members either side of a non-member. "Back to back" is the definition, so
    /// this fails it rather than testing it.
    NonContiguousGrouping,
    /// One member, where the partner was never added.
    SingleMemberGrouping,
    /// An exercise entry carrying no sets at all.
    NoSetsInEntry,
    /// A set kind the domain does not recognise. Kept verbatim rather than
    /// normalised, because comparing it against a list we control would make the
    /// source's vocabulary ours.
    UnknownSetKind { kind: String },
    /// An intensity outside the eight positions.
    UnrecognisedIntensity { value: String },
    /// A quantity that would not parse, or that the type rejects.
    UnreadableValue { field: &'static str, detail: String },
    /// Every item refused, so the record yields no entity. An entity holds a
    /// non-empty sequence by construction.
    NothingTranslatable,
    /// The payload itself could not be read.
    UnreadablePayload { detail: String },
    /// A real case, correctly recorded, that this model has no entity for.
    ///
    /// The operator's Peloton account holds 141 workouts that are not Bike+
    /// rides — stretching, yoga, strength, cardio — and *"they are separate
    /// entities, we're not modelling them right now"*. Forcing them into an
    /// existing type would be worse than refusing them and dropping them would
    /// be worse still (§ 37), so they are refused, counted, and listed as
    /// evidence for a later feature.
    ///
    /// The detail is the source's own words for what it was, kept verbatim:
    /// comparing it against a list we control would make the source's
    /// vocabulary ours.
    Unmodelled { detail: String },
    /// The entity composes more than one of the source's responses (§ 3.1) and
    /// one of them has not been collected.
    ///
    /// Not a defect in the data and not a limitation of the model: the two
    /// streams resume, run and lock independently, so a ride collected since
    /// the last walk of the graphs genuinely has no samples here yet. What
    /// fixes it is collecting the other stream, which is why it reads as
    /// something to put right at the source.
    CompanionNotLanded { stream: String },
    /// The source served a series and every reading in it was a sensor saying
    /// nothing.
    ///
    /// Distinct from the series being absent, which is not a refusal at all —
    /// nothing was worn, and § 37 wants that visible as an absence rather than
    /// as an empty series.
    NoReadingsInSeries { series: &'static str },
    /// The source served an entity missing a series it cannot be built without.
    MissingSeries { series: &'static str },
}

impl RefusalReason {
    /// Which of the three this is, and therefore what an operator does with it.
    pub const fn kind(&self) -> RefusalKind {
        match self {
            Self::NonContiguousGrouping
            | Self::SingleMemberGrouping
            | Self::NoSetsInEntry
            | Self::UnknownSetKind { .. }
            | Self::UnrecognisedIntensity { .. }
            | Self::UnreadableValue { .. }
            | Self::UnreadablePayload { .. }
            | Self::NoReadingsInSeries { .. }
            | Self::MissingSeries { .. }
            // Fixed by collecting the other stream, which is a thing to do at
            // the source rather than a gap in the model.
            | Self::CompanionNotLanded { .. }
            // Not a problem in itself — it is the consequence of the others,
            // recorded so the record is still accounted for. It sits with wrong
            // data because that is what an operator does about it.
            | Self::NothingTranslatable => RefusalKind::WrongData,
            Self::Unmodelled { .. } => RefusalKind::Unmodelled,
        }
    }

    /// The stable key. Persisted and queried, so "the refusals are exactly
    /// these" is a `WHERE` clause rather than a grep over prose.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NonContiguousGrouping => "non-contiguous-grouping",
            Self::SingleMemberGrouping => "single-member-grouping",
            Self::NoSetsInEntry => "no-sets-in-entry",
            Self::UnknownSetKind { .. } => "unknown-set-kind",
            Self::UnrecognisedIntensity { .. } => "unrecognised-intensity",
            Self::UnreadableValue { .. } => "unreadable-value",
            Self::NothingTranslatable => "nothing-translatable",
            Self::UnreadablePayload { .. } => "unreadable-payload",
            Self::Unmodelled { .. } => "unmodelled",
            Self::CompanionNotLanded { .. } => "companion-not-landed",
            Self::NoReadingsInSeries { .. } => "no-readings-in-series",
            Self::MissingSeries { .. } => "missing-series",
        }
    }

    /// Whatever the source said, where the reason keeps it. `None` where the
    /// reason is complete without it.
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::UnknownSetKind { kind } => Some(kind.clone()),
            Self::UnrecognisedIntensity { value } => Some(value.clone()),
            Self::UnreadablePayload { detail } | Self::Unmodelled { detail } => {
                Some(detail.clone())
            }
            Self::UnreadableValue { field, detail } => Some(format!("{field}: {detail}")),
            Self::CompanionNotLanded { stream } => Some(stream.clone()),
            Self::NoReadingsInSeries { series } | Self::MissingSeries { series } => {
                Some((*series).to_owned())
            }
            _ => None,
        }
    }
}

impl fmt::Display for RefusalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonContiguousGrouping => {
                f.write_str("superset members either side of a non-member")
            }
            Self::SingleMemberGrouping => f.write_str("a superset with a single member"),
            Self::NoSetsInEntry => f.write_str("an exercise entry with no sets"),
            Self::UnknownSetKind { kind } => write!(f, "unrecognised set kind {kind:?}"),
            Self::UnrecognisedIntensity { value } => write!(f, "unrecognised intensity {value:?}"),
            Self::UnreadableValue { field, detail } => write!(f, "unreadable {field}: {detail}"),
            Self::NothingTranslatable => f.write_str("nothing in the record translated"),
            Self::UnreadablePayload { detail } => write!(f, "unreadable payload: {detail}"),
            Self::Unmodelled { detail } => write!(f, "{detail} is not something we model yet"),
            Self::CompanionNotLanded { stream } => {
                write!(f, "nothing landed for it in {stream}")
            }
            Self::NoReadingsInSeries { series } => {
                write!(f, "every {series} reading was a sensor saying nothing")
            }
            Self::MissingSeries { series } => write!(f, "no {series} series was served"),
        }
    }
}

/// One thing the domain would not accept, and enough to act on it without
/// re-reading the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub locus: RefusalLocus,
    /// Which of our exercises the refused thing belonged to, where that was
    /// known by the time it was refused.
    ///
    /// A position alone is not enough to act on: "exercise 4, set 2" sends the
    /// operator back to the payload to find out what exercise 4 was, which is
    /// the trip a refusal exists to save them.
    ///
    /// `None` where the record failed before any exercise was resolved, and
    /// `None` throughout for an entity that has no vocabulary below itself: a
    /// Bike+ ride is refused whole or not at all, because a series with a hole
    /// we invented would be worse than no ride.
    pub exercise: Option<Exercise>,
    pub reason: RefusalReason,
}

impl Refusal {
    pub const fn kind(&self) -> RefusalKind {
        self.reason.kind()
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — {}", self.source_record_id, self.locus)?;
        if let Some(exercise) = self.exercise {
            write!(f, " ({exercise})")?;
        }
        write!(f, ": {}", self.reason)
    }
}
