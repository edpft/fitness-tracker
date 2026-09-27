//! A landed spreadsheet read as sheets of cells, before anything decides what
//! the cells mean. Shared by the weigh-in and gym-session readers, which each
//! recognise their own layouts in the same sheets.

use std::io::Cursor;

use calamine::{Data, Reader as _, open_workbook_auto_from_rs};
use domain::landing::FileProvenance;
use jiff::civil::Date;

/// One cell's content, as far as a reader needs to know.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Value {
    Empty,
    Number(f64),
    Text(String),
    Day(Date),
    /// A time of day with no date, in whole seconds: what a sheet shows as
    /// `0:03:00` for a rest, or `0:00:25` for a hold.
    Time(u64),
    /// Anything else: a time of day, a boolean, an error such as `#REF!`.
    Other(String),
}

impl Value {
    pub(super) fn from_cell(cell: &Data) -> Self {
        match cell {
            Data::Empty => Self::Empty,
            Data::Float(number) => Self::Number(*number),
            Data::Int(number) => i32::try_from(*number).map_or_else(
                |_| Self::Other(number.to_string()),
                |n| Self::Number(f64::from(n)),
            ),
            Data::String(text) => Self::Text(text.clone()),
            Data::DateTime(at) if (0.0..1.0).contains(&at.as_f64()) => seconds_of_day(at.as_f64())
                .map_or_else(|| Self::Other(format!("{cell}")), Self::Time),
            Data::DateTime(at) if at.is_datetime() => {
                let (year, month, day, hour, minute, second, milli) = at.to_ymd_hms_milli();
                let midnight = hour == 0 && minute == 0 && second == 0 && milli == 0;
                let date = i16::try_from(year)
                    .ok()
                    .and_then(|year| Date::new(year, month.cast_signed(), day.cast_signed()).ok());
                match date {
                    Some(date) if midnight => Self::Day(date),
                    _ => Self::Other(format!("{cell}")),
                }
            }
            Data::DateTimeIso(text) => text
                .parse::<Date>()
                .map_or_else(|_| Self::Other(text.clone()), Self::Day),
            other => Self::Other(format!("{other}")),
        }
    }

    pub(super) fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.trim()),
            _ => None,
        }
    }
}

/// One sheet of a workbook, addressed as the workbook addresses it.
pub(super) struct Sheet {
    pub(super) name: String,
    /// Where the first cell held sits, zero-based. A sheet whose first row or
    /// column is empty starts further in.
    pub(super) origin: (u32, u32),
    pub(super) rows: Vec<Vec<Value>>,
}

impl Sheet {
    pub(super) fn get(&self, row: u32, column: u32) -> &Value {
        const EMPTY: &Value = &Value::Empty;
        let (Some(row), Some(column)) = (
            row.checked_sub(self.origin.0),
            column.checked_sub(self.origin.1),
        ) else {
            return EMPTY;
        };
        usize::try_from(row)
            .ok()
            .zip(usize::try_from(column).ok())
            .and_then(|(row, column)| self.rows.get(row)?.get(column))
            .unwrap_or(EMPTY)
    }

    pub(super) fn height(&self) -> u32 {
        u32::try_from(self.rows.len())
            .unwrap_or(u32::MAX)
            .saturating_add(self.origin.0)
    }

    pub(super) fn width(&self) -> u32 {
        let widest = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        u32::try_from(widest)
            .unwrap_or(u32::MAX)
            .saturating_add(self.origin.1)
    }
}

/// Every sheet of a workbook, or why it could not be read as one.
pub(super) fn workbook(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
    let mut workbook =
        open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|error| error.to_string())?;
    let mut sheets = Vec::new();
    for name in workbook.sheet_names() {
        let range = workbook
            .worksheet_range(&name)
            .map_err(|error| format!("sheet {name:?}: {error}"))?;
        sheets.push(Sheet {
            origin: range.start().unwrap_or((0, 0)),
            rows: range
                .rows()
                .map(|row| row.iter().map(Value::from_cell).collect())
                .collect(),
            name,
        });
    }
    Ok(sheets)
}

/// A CSV file as the one sheet a spreadsheet program shows it as, named after
/// the file. Every cell is text; the layout decides what it means.
pub(super) fn csv(bytes: &[u8], file: &FileProvenance) -> Result<Vec<Sheet>, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let name = file
        .path()
        .as_str()
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map_or_else(|| file.path().to_string(), |(stem, _)| stem.to_owned());
    let rows = text
        .lines()
        .map(|line| {
            line.split(',')
                .map(|cell| {
                    if cell.is_empty() {
                        Value::Empty
                    } else {
                        Value::Text(cell.to_owned())
                    }
                })
                .collect()
        })
        .collect();
    Ok(vec![Sheet {
        name,
        origin: (0, 0),
        rows,
    }])
}

/// A fraction of a day as whole seconds. Formatted and parsed rather than cast,
/// because a float cast would truncate silently where this cannot.
fn seconds_of_day(fraction: f64) -> Option<u64> {
    format!("{:.0}", fraction * 86_400.0).parse().ok()
}
