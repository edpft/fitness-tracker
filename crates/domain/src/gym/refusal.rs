//! What a landing record asserted that the domain will not express.
//!
//! § 37: partial data is recorded as partial. That is not satisfied by
//! translating what fits — the grammatical part translates, the ungrammatical
//! part does not, and translation never guesses which repair was meant.
//!
//! A refusal is a value with a place and a reason, not a formatted sentence.
//! The prose satisfies a reader and defeats every other use: the assertion that
//! matters is that the refusals are *exactly* a known set, which is a query over
//! reasons and not a grep over text.
//!
//! The three kinds are the point of recording them at all. 24 sets and 2
//! groupings in the corpus refuse, and each is either data to fix at source, a
//! limitation to declare, or a gap in the model — a model that cannot hold a
//! genuine case needs refining, whereas a model that rejects a wrong record is
//! working, and telling them apart is unavailable if the refusal is a stack
//! trace or a dropped row.

use std::fmt;

use super::exercise::Exercise;
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
    /// No bar mass is assumed and no default applied: 10, 15 and 20 kg bars are
    /// all in use, so every repair is a guess.
    ZeroOnAbsoluteLoad,
    /// Band tension varies through the range of motion, so no scalar is honest,
    /// and nothing available records the mechanism.
    BandResistance,
    /// A real event that is not a set. It needs an *attempt*, which belongs with
    /// prescribed-versus-performed.
    ZeroReps,
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
    /// Every item refused, so the record yields no workout. A workout holds a
    /// non-empty sequence of items by construction.
    NothingTranslatable,
    /// The payload itself could not be read as a workout.
    UnreadablePayload { detail: String },
}

impl RefusalReason {
    /// Which of the three this is, and therefore what an operator does with it.
    pub const fn kind(&self) -> RefusalKind {
        match self {
            Self::ZeroOnAbsoluteLoad
            | Self::NonContiguousGrouping
            | Self::SingleMemberGrouping
            | Self::NoSetsInEntry
            | Self::UnknownSetKind { .. }
            | Self::UnrecognisedIntensity { .. }
            | Self::UnreadableValue { .. }
            | Self::UnreadablePayload { .. }
            // Not a problem in itself — it is the consequence of the others,
            // recorded so the record is still accounted for. It sits with wrong
            // data because that is what an operator does about it.
            | Self::NothingTranslatable => RefusalKind::WrongData,
            Self::BandResistance => RefusalKind::DeclaredLimitation,
            Self::ZeroReps => RefusalKind::Unmodelled,
        }
    }

    /// The stable key. Persisted and queried, so SC-002 is a `WHERE` clause.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ZeroOnAbsoluteLoad => "zero-on-absolute-load",
            Self::BandResistance => "band-resistance",
            Self::ZeroReps => "zero-reps",
            Self::NonContiguousGrouping => "non-contiguous-grouping",
            Self::SingleMemberGrouping => "single-member-grouping",
            Self::NoSetsInEntry => "no-sets-in-entry",
            Self::UnknownSetKind { .. } => "unknown-set-kind",
            Self::UnrecognisedIntensity { .. } => "unrecognised-intensity",
            Self::UnreadableValue { .. } => "unreadable-value",
            Self::NothingTranslatable => "nothing-translatable",
            Self::UnreadablePayload { .. } => "unreadable-payload",
        }
    }

    /// Whatever the source said, where the reason keeps it. `None` where the
    /// reason is complete without it.
    pub fn detail(&self) -> Option<String> {
        match self {
            Self::UnknownSetKind { kind } => Some(kind.clone()),
            Self::UnrecognisedIntensity { value } => Some(value.clone()),
            Self::UnreadablePayload { detail } => Some(detail.clone()),
            Self::UnreadableValue { field, detail } => Some(format!("{field}: {detail}")),
            _ => None,
        }
    }
}

impl fmt::Display for RefusalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroOnAbsoluteLoad => {
                f.write_str("zero load on an exercise whose implement has mass")
            }
            Self::BandResistance => f.write_str("band resistance is not modelled"),
            Self::ZeroReps => f.write_str("a set of zero reps is an attempt, not a set"),
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
    /// A position alone is not enough to act on. "Exercise 4, set 2" sends the
    /// operator back to the payload to find out what exercise 4 was, which is
    /// exactly what FR-022 says a refusal must save them. `None` only where the
    /// record failed before any exercise was resolved.
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
