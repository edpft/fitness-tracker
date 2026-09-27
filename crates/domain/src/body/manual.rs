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

use std::fmt;

use jiff::civil::Date;

use crate::{
    landing::{FilePath, LandingRecordId, SourceRecordId},
    measure::Kg,
    newtype::string_name,
    normalised::NormalisedEntity,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidSheetName {
    #[error("a sheet name must not be empty")]
    Empty,
}

/// A sheet within a workbook, as the workbook names it: `2016`, `Sheet1`,
/// `body weight`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SheetName(String);

impl TryFrom<String> for SheetName {
    type Error = InvalidSheetName;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        if name.is_empty() {
            return Err(InvalidSheetName::Empty);
        }
        Ok(Self(name))
    }
}

string_name!(SheetName, InvalidSheetName);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCellRef {
    #[error("{0:?} is not a cell reference such as C6")]
    NotACell(String),
}

/// One cell of a sheet, in the notation every spreadsheet shows: `C6` is the
/// third column of the sixth row.
///
/// A cell rather than a row, because not every sheet runs down the page: the
/// 2014 sheets put their dates across the first row, so a weigh-in there is a
/// column.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef(String);

impl CellRef {
    /// The cell at a zero-based row and column: `(5, 2)` is `C6`.
    pub fn at(row: u32, column: u32) -> Self {
        let mut letters = Vec::new();
        let mut remaining = u64::from(column).saturating_add(1);
        while let Some(below) = remaining.checked_sub(1) {
            let digit = u8::try_from(below % 26).unwrap_or(0);
            letters.push(char::from(b'A'.saturating_add(digit)));
            remaining = below / 26;
        }
        let letters: String = letters.into_iter().rev().collect();
        Self(format!("{letters}{}", u64::from(row).saturating_add(1)))
    }
}

impl TryFrom<String> for CellRef {
    type Error = InvalidCellRef;

    fn try_from(reference: String) -> Result<Self, Self::Error> {
        let letters = reference.bytes().take_while(u8::is_ascii_uppercase).count();
        let digits = reference.get(letters..).unwrap_or_default();
        let is_cell = (1..=3).contains(&letters)
            && !digits.is_empty()
            && !digits.starts_with('0')
            && digits.bytes().all(|byte| byte.is_ascii_digit());
        if !is_cell {
            return Err(InvalidCellRef::NotACell(reference));
        }
        Ok(Self(reference))
    }
}

string_name!(CellRef, InvalidCellRef);

/// Where a manual weigh-in was written: the landed file, the sheet and the
/// cell holding the mass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub landed_as: LandingRecordId,
    /// The file's identity, which is the digest of its bytes.
    pub source_record_id: SourceRecordId,
    pub file: FilePath,
    pub sheet: SheetName,
    pub cell: CellRef,
}

impl fmt::Display for Cell {
    /// `Dropbox/Random/Body Weight 2018.xlsx › 2016!C6`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} › {}!{}", self.file, self.sheet, self.cell)
    }
}

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
