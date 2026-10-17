//! A weigh-in the operator read off the scale and typed in himself (#273).
//!
//! **A day, a mass and where it was written.** The operator stood on the scale,
//! read the display and typed the figure into a spreadsheet on his phone. After
//! that the value existed only in his memory, so the cell it was typed into is
//! the nearest thing to the observation there is, and it is the weigh-in's
//! source.
//!
//! **Each copy is its own weigh-in.** The same day sits in up to six files,
//! because the sheets were copied forward from year to year, and a copy can
//! differ from the original: a row slipped in one of them. Whether two copies
//! are one weigh-in, and which to believe, is the canonical layer's question
//! ([`super::canonical`]). So the identity here is the cell, never the day.
//!
//! **Some places say when he typed it** (#242). MyFitnessPal records when each
//! weight was entered; a spreadsheet cell does not, and its document records
//! only when it was last saved (#409). Either is the evidence § 10 settles a
//! disagreement between accounts by, so it is carried as an [`Entry`].

use jiff::{Timestamp, civil::Date};

use super::{composition::Component, quantity::Percentage};
pub use crate::landing::{Cell, CellRef, InvalidCellRef, InvalidSheetName, SheetName};
use crate::{landing::SourceRecordId, measure::Kg, normalised::NormalisedEntity};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidWeighIn {
    #[error("a weigh-in of no mass is not a weigh-in")]
    NoMass,
}

/// What says when a manual weigh-in was typed.
///
/// **§ 10 settles a disagreement by this**, so it is carried on every one: an
/// entry stamped when it was typed beats one that is not, and of copies of one
/// entry, the copy in the document saved earliest stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// The place it was typed into stamped when. MyFitnessPal does (#242).
    Stamped(Timestamp),
    /// A spreadsheet stamps no cell, so all there is is when the document
    /// holding this copy was last saved.
    Saved(LastSaved),
}

/// When a document was last saved, and what says so.
///
/// **The document's own record first** (`docProps/core.xml`), because the file
/// system's date is when Dropbox copied the file: `Body Weight 2018 (2).xlsx`
/// is dated 2019-04-03 and records its last save as 2017-12-31. Four workbooks
/// record none, and for those the file system's is all there is (the operator,
/// 2026-10-13, #409).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastSaved {
    ByTheDocument(Timestamp),
    ByTheFileSystem(Timestamp),
}

impl LastSaved {
    pub const fn at(self) -> Timestamp {
        match self {
            Self::ByTheDocument(at) | Self::ByTheFileSystem(at) => at,
        }
    }
}

/// What the scale said the body was made of, written beside the mass.
///
/// **Its own cell**, because the figure was typed into one of its own: the
/// cell is its method, as the mass's cell is the weigh-in's (#275).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualComposition {
    pub component: Component,
    pub percentage: Percentage,
    pub written_in: Cell,
}

/// A body mass the operator read off the scale and wrote down.
///
/// **A day and never a time**: no sheet records when in the day he stepped on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualWeighIn {
    on: Date,
    mass: Kg,
    written_in: Cell,
    entry: Entry,
    composition: Vec<ManualComposition>,
}

impl ManualWeighIn {
    /// # Errors
    ///
    /// [`InvalidWeighIn::NoMass`] for a mass of zero.
    pub fn new(on: Date, mass: Kg, written_in: Cell, entry: Entry) -> Result<Self, InvalidWeighIn> {
        if mass.is_none() {
            return Err(InvalidWeighIn::NoMass);
        }
        Ok(Self {
            on,
            mass,
            written_in,
            entry,
            composition: Vec::new(),
        })
    }

    /// The composition written beside the mass, read against it and never
    /// without it.
    #[must_use]
    pub fn with_composition(mut self, composition: Vec<ManualComposition>) -> Self {
        self.composition = composition;
        self
    }

    pub fn composition(&self) -> &[ManualComposition] {
        &self.composition
    }

    pub const fn on(&self) -> Date {
        self.on
    }

    pub const fn mass(&self) -> Kg {
        self.mass
    }

    pub const fn written_in(&self) -> &Cell {
        &self.written_in
    }

    pub const fn entry(&self) -> Entry {
        self.entry
    }

    /// When it was typed, where the place it was typed into stamped that.
    pub const fn entered_at(&self) -> Option<Timestamp> {
        match self.entry {
            Entry::Stamped(at) => Some(at),
            Entry::Saved(_) => None,
        }
    }

    /// When the document holding it was last saved, where it was typed into
    /// one.
    pub const fn last_saved(&self) -> Option<LastSaved> {
        match self.entry {
            Entry::Saved(saved) => Some(saved),
            Entry::Stamped(_) => None,
        }
    }
}

impl NormalisedEntity for ManualWeighIn {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.written_in.source_record_id]
    }
}
