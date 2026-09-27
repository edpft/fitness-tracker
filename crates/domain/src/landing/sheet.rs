//! Where in a landed spreadsheet something was written: a sheet, and a cell
//! within it.
//!
//! Landing's rather than any one entity's, because it addresses the file that
//! landed and says nothing about what the cell means. A manual weigh-in names
//! the cell its mass was typed into, and a manual gym session names the sheet
//! it was logged on and the cell of every set.

use std::fmt;

use crate::newtype::string_name;

use super::{FilePath, LandingRecordId, SourceRecordId};

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

/// One cell of one landed file: the file, the sheet and the cell.
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

/// A cell named with its sheet, within a file known from elsewhere:
/// `Push!N3`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SheetCell {
    pub sheet: SheetName,
    pub cell: CellRef,
}

impl fmt::Display for SheetCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}!{}", self.sheet, self.cell)
    }
}
