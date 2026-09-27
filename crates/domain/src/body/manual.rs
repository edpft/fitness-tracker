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
//! are one weigh-in is the canonical layer's question. Which to believe is the
//! analytical layer's, and the operator's rule is that the copy nearest the
//! original entry beats the latest one. So the identity here is the cell, never
//! the day.

use jiff::civil::Date;

pub use crate::landing::{Cell, CellRef, InvalidCellRef, InvalidSheetName, SheetName};
use crate::{landing::SourceRecordId, measure::Kg, normalised::NormalisedEntity};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidWeighIn {
    #[error("a weigh-in of no mass is not a weigh-in")]
    NoMass,
}

/// A body mass the operator read off the scale and wrote down.
///
/// **A day and never a time**: no sheet records when in the day he stepped on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualWeighIn {
    on: Date,
    mass: Kg,
    written_in: Cell,
}

impl ManualWeighIn {
    /// # Errors
    ///
    /// [`InvalidWeighIn::NoMass`] for a mass of zero.
    pub fn new(on: Date, mass: Kg, written_in: Cell) -> Result<Self, InvalidWeighIn> {
        if mass.is_none() {
            return Err(InvalidWeighIn::NoMass);
        }
        Ok(Self {
            on,
            mass,
            written_in,
        })
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
}

impl NormalisedEntity for ManualWeighIn {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.written_in.source_record_id]
    }
}
