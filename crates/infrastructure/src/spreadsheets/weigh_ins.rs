//! Manual weigh-ins, read out of the operator's historical spreadsheets
//! (#273).
//!
//! **Which sheets are weigh-ins is declared, not guessed.** A sheet is read as
//! weigh-ins only where its header is one of [`LAYOUTS`], word for word. A
//! looser rule, such as "a date column beside a column called weight", also
//! matches the training logs, where `Weight` and `kg` are the load on the bar:
//! `1RM`, `Strength` and `strength_training` all have one.
//!
//! **Only the typed-in mass is read.** The 7-day averages, % change, BMI, fat %
//! and water % beside it are derived, or method-dependent (§ 6), or both, and
//! none of them is a weigh-in.
//!
//! **A blank is a gap, not a zero** (§ 37), and so is `???`, which is what the
//! operator wrote when he did not know. So is a zero: `2018 bodyweight only`
//! holds `0` against every day from 14 to 30 December 2018, in a file last
//! saved that May, which is a placeholder for days not yet reached and not a
//! reading.

use std::io::Cursor;

use application::{NormalisationError, Translation, ports::Translator};
use calamine::{Data, Reader as _, open_workbook_auto_from_rs};
use domain::{
    body::{Cell, CellRef, ManualWeighIn, SheetName},
    landing::{FileProvenance, LandedRecord},
    measure::Kg,
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
    sequence::NonEmpty,
};
use jiff::civil::Date;

use crate::scribe::Scribe;

/// How a sheet lays its weigh-ins out.
#[derive(Debug, Clone, Copy)]
enum Layout {
    /// One weigh-in per row, under a header row that starts with these cells.
    /// The date is under `date`, and the mass under the last of them.
    Down {
        header: &'static [&'static str],
        date: usize,
    },
    /// One weigh-in per column: the first column labels a row of dates and a
    /// row of masses, and each column after it is a day.
    Across {
        date: &'static str,
        mass: &'static str,
    },
}

/// Every weigh-in layout in the operator's spreadsheets, read off the files on
/// 2026-09-27.
const LAYOUTS: &[Layout] = &[
    // The Body Weight 2018 family's 2015, 2016 and 2017 sheets, and one
    // version's 2018.
    Layout::Down {
        header: &["Date", "Day", "KG"],
        date: 0,
    },
    // `Body Weight 2018 (2)`'s 2018.
    Layout::Down {
        header: &["Date", "Day", "Week", "KG"],
        date: 0,
    },
    // `Body Weight 2018`'s 2018, below a summary block.
    Layout::Down {
        header: &["Date", "Week #", "KG"],
        date: 0,
    },
    // `Body Weight`.
    Layout::Down {
        header: &["Date", "Day", "WGHT"],
        date: 0,
    },
    // `CT 2017` and its conflicted copies.
    Layout::Down {
        header: &["Wk", "Date", "Day", "KG"],
        date: 1,
    },
    Layout::Down {
        header: &["Wk", "Date", "Day", "Weight"],
        date: 1,
    },
    Layout::Down {
        header: &["Date", "Day", "Weight"],
        date: 0,
    },
    // `2018 bodyweight only`, and OneDrive's `Weight`.
    Layout::Down {
        header: &["Date", "Weight"],
        date: 0,
    },
    // `weight_2020` and `Weight_2021`.
    Layout::Down {
        header: &["date", "weight"],
        date: 0,
    },
    // `Nutrition`.
    Layout::Down {
        header: &["date", "wk", "day", "weight"],
        date: 0,
    },
    // `2018 bodyweight only.csv`.
    Layout::Down {
        header: &["date", "body_weight"],
        date: 0,
    },
    // `Training 2014 - Mk II` and `Winter training 2014`.
    Layout::Across {
        date: "Date",
        mass: "Weight (kg)",
    },
    // `British Army Fitness`, 2013.
    Layout::Across {
        date: "Date",
        mass: "Weight",
    },
];

/// How far down a sheet its header may sit. `Body Weight 2018`'s is on the
/// fourth row, below a summary.
const HEADER_WITHIN: u32 = 6;

/// A date as the CSV export wrote it: `Mon 01 Jan 18`.
const CSV_DATE: &str = "%a %d %b %y";

/// One cell's content, as far as a weigh-in needs to know.
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Empty,
    Number(f64),
    Text(String),
    Day(Date),
    /// Anything else: a time of day, a boolean, an error such as `#REF!`.
    Other(String),
}

impl Value {
    fn from_cell(cell: &Data) -> Self {
        match cell {
            Data::Empty => Self::Empty,
            Data::Float(number) => Self::Number(*number),
            Data::Int(number) => i32::try_from(*number).map_or_else(
                |_| Self::Other(number.to_string()),
                |n| Self::Number(f64::from(n)),
            ),
            Data::String(text) => Self::Text(text.clone()),
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

    fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.trim()),
            _ => None,
        }
    }
}

/// One sheet of a workbook, addressed as the workbook addresses it.
struct Sheet {
    name: String,
    /// Where the first cell held sits, zero-based. A sheet whose first row or
    /// column is empty starts further in.
    origin: (u32, u32),
    rows: Vec<Vec<Value>>,
}

impl Sheet {
    fn get(&self, row: u32, column: u32) -> &Value {
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

    fn height(&self) -> u32 {
        u32::try_from(self.rows.len())
            .unwrap_or(u32::MAX)
            .saturating_add(self.origin.0)
    }

    fn width(&self) -> u32 {
        let widest = self.rows.iter().map(Vec::len).max().unwrap_or(0);
        u32::try_from(widest)
            .unwrap_or(u32::MAX)
            .saturating_add(self.origin.1)
    }
}

/// Every sheet of a workbook, or why it could not be read as one.
fn workbook(bytes: &[u8]) -> Result<Vec<Sheet>, String> {
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
fn csv(bytes: &[u8], file: &FileProvenance) -> Result<Vec<Sheet>, String> {
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

/// Reads manual weigh-ins out of a landed spreadsheet.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpreadsheetWeighInTranslator;

impl Translator for SpreadsheetWeighInTranslator {
    type Account = LandedRecord;
    type Entity = ManualWeighIn;

    /// The zone is not consulted: a manual weigh-in is a day, and a day needs
    /// no zone to be read.
    fn translate(
        &self,
        record: &LandedRecord,
        _zone: &OperatorZone,
    ) -> Result<Translation<ManualWeighIn>, NormalisationError> {
        let mut scribe = Scribe::new(record);
        let Some(file) = record.provenance().as_file() else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::UnreadablePayload {
                    detail: format!("{} was served by a feed, not a folder", record.provenance()),
                },
            ));
        };

        let bytes = record.payload().as_bytes();
        let sheets = if file.path().as_str().to_lowercase().ends_with(".csv") {
            csv(bytes, file)
        } else {
            workbook(bytes)
        };
        let sheets = match sheets {
            Ok(sheets) => sheets,
            Err(detail) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::Unmodelled {
                        detail: format!(
                            "{}, a file that is not a spreadsheet ({detail}),",
                            file.path()
                        ),
                    },
                ));
            }
        };

        let mut weigh_ins = Vec::new();
        for sheet in &sheets {
            read_sheet(record, file, sheet, &mut scribe, &mut weigh_ins);
        }

        let refusals = scribe.into_refusals();
        if let Ok(entities) = NonEmpty::new(weigh_ins) {
            return Ok(Translation::Entities { entities, refusals });
        }
        Ok(NonEmpty::new(refusals).map_or_else(
            |_| {
                Scribe::new(record).only(
                    RefusalLocus::Record,
                    RefusalReason::Unmodelled {
                        detail: format!(
                            "{}, a spreadsheet of something other than weigh-ins,",
                            file.path()
                        ),
                    },
                )
            },
            Translation::Refused,
        ))
    }
}

/// The weigh-ins in one sheet, if it is laid out as weigh-ins.
fn read_sheet(
    record: &LandedRecord,
    file: &FileProvenance,
    sheet: &Sheet,
    scribe: &mut Scribe,
    weigh_ins: &mut Vec<ManualWeighIn>,
) {
    let Ok(sheet_name) = SheetName::try_from(sheet.name.as_str()) else {
        return;
    };
    let place = |row: u32, column: u32| Cell {
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        file: file.path().clone(),
        sheet: sheet_name.clone(),
        cell: CellRef::at(row, column),
    };

    for layout in LAYOUTS {
        match *layout {
            Layout::Down { header, date } => {
                let Some(header_row) = (0..HEADER_WITHIN).find(|&row| {
                    header.iter().enumerate().all(|(column, label)| {
                        u32::try_from(column)
                            .is_ok_and(|column| sheet.get(row, column).text() == Some(*label))
                    })
                }) else {
                    continue;
                };
                let (Ok(date), Ok(mass)) = (
                    u32::try_from(date),
                    u32::try_from(header.len().saturating_sub(1)),
                ) else {
                    continue;
                };
                for row in header_row.saturating_add(1)..sheet.height() {
                    weigh_in(
                        sheet.get(row, date),
                        sheet.get(row, mass),
                        place(row, mass),
                        scribe,
                        weigh_ins,
                    );
                }
                return;
            }
            Layout::Across { date, mass } => {
                let labelled = |label: &str| {
                    (0..HEADER_WITHIN).find(|&row| sheet.get(row, 0).text() == Some(label))
                };
                let (Some(date_row), Some(mass_row)) = (labelled(date), labelled(mass)) else {
                    continue;
                };
                for column in 1..sheet.width() {
                    weigh_in(
                        sheet.get(date_row, column),
                        sheet.get(mass_row, column),
                        place(mass_row, column),
                        scribe,
                        weigh_ins,
                    );
                }
                return;
            }
        }
    }
}

/// One weigh-in, a gap, or a refusal.
fn weigh_in(
    date: &Value,
    mass: &Value,
    cell: Cell,
    scribe: &mut Scribe,
    weigh_ins: &mut Vec<ManualWeighIn>,
) {
    let at = format!("{}!{}", cell.sheet, cell.cell);
    let mass = match mass {
        Value::Empty => return,
        Value::Text(text) if matches!(text.trim(), "" | "???") => return,
        Value::Number(number) => Kg::try_from(number.to_string()),
        Value::Text(text) => Kg::try_from(text.trim()),
        Value::Day(_) | Value::Other(_) => {
            return refuse(scribe, &at, "mass", &format!("{mass:?} is not a mass"));
        }
    };
    let mass = match mass {
        Ok(mass) if mass.is_none() => return,
        Ok(mass) => mass,
        Err(error) => return refuse(scribe, &at, "mass", &error.to_string()),
    };

    let on = match date {
        Value::Day(day) => *day,
        Value::Text(text) => match Date::strptime(CSV_DATE, text.trim()) {
            Ok(day) => day,
            Err(error) => return refuse(scribe, &at, "date", &format!("{text:?}: {error}")),
        },
        Value::Empty | Value::Number(_) | Value::Other(_) => {
            return refuse(scribe, &at, "date", &format!("{date:?} is not a day"));
        }
    };

    match ManualWeighIn::new(on, mass, cell) {
        Ok(weigh_in) => weigh_ins.push(weigh_in),
        Err(error) => refuse(scribe, &at, "mass", &error.to_string()),
    }
}

/// A cell that holds something, where what it holds is not a weigh-in.
fn refuse(scribe: &mut Scribe, at: &str, field: &'static str, detail: &str) {
    scribe.note(
        RefusalLocus::Record,
        RefusalReason::UnreadableValue {
            field,
            detail: format!("{at}: {detail}"),
        },
    );
}
