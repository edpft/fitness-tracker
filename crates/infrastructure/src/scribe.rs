//! Collecting refusals as a translation walks an account.
//!
//! A small mutable thing rather than a returned list at every level: a refusal
//! can be raised four layers down, and threading `Vec<Refusal>` through each of
//! them would put the plumbing in front of the reading.
//!
//! Shared by every translator rather than written once per adapter. What a
//! refusal is, and the rule that an account yielding no entity must yield at
//! least one reason, are the normalised layer's (§ 37) and not any one source's
//! — so a second adapter getting the rule subtly wrong is a failure mode worth
//! removing rather than reviewing for.

use application::Translation;
use domain::{
    gym::exercise::Exercise,
    landing::{LandedRecord, LandingRecordId, SourceRecordId},
    normalised::{Refusal, RefusalLocus, RefusalReason},
    sequence::NonEmpty,
};

/// Collects refusals as translation walks an account.
pub struct Scribe {
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    refusals: Vec<Refusal>,
}

impl Scribe {
    /// Anchored to the landing record the account is *identified* by. Where an
    /// account composes more than one record (§ 3.1), that is the one naming
    /// the thing — a refusal points an operator at a ride, not at a graph they
    /// cannot look up.
    pub fn new(record: &LandedRecord) -> Self {
        Self {
            landed_as: record.id(),
            source_record_id: record.source_record_id().clone(),
            refusals: Vec::new(),
        }
    }

    pub fn note(&mut self, locus: RefusalLocus, reason: RefusalReason) {
        self.note_for(locus, None, reason);
    }

    /// A refusal that knows which exercise it belonged to.
    pub fn note_for(
        &mut self,
        locus: RefusalLocus,
        exercise: Option<Exercise>,
        reason: RefusalReason,
    ) {
        self.refusals.push(Refusal {
            landed_as: self.landed_as,
            source_record_id: self.source_record_id.clone(),
            locus,
            exercise,
            reason,
        });
    }

    /// An account that produced nothing but this one reason.
    pub fn only<E>(&mut self, locus: RefusalLocus, reason: RefusalReason) -> Translation<E> {
        self.note(locus, reason);
        self.nothing_translatable()
    }

    /// Everything refused, so the account yields no entity — an entity holds a
    /// non-empty sequence by construction. Not a run failure.
    pub fn nothing_translatable<E>(&mut self) -> Translation<E> {
        if self.refusals.is_empty() {
            self.note(RefusalLocus::Record, RefusalReason::NothingTranslatable);
        }
        let refusals = std::mem::take(&mut self.refusals);
        NonEmpty::new(refusals).map_or_else(
            |_| {
                Translation::Refused(NonEmpty::of(
                    Refusal {
                        landed_as: self.landed_as,
                        source_record_id: self.source_record_id.clone(),
                        locus: RefusalLocus::Record,
                        exercise: None,
                        reason: RefusalReason::NothingTranslatable,
                    },
                    Vec::new(),
                ))
            },
            Translation::Refused,
        )
    }

    pub fn into_refusals(self) -> Vec<Refusal> {
        self.refusals
    }
}
