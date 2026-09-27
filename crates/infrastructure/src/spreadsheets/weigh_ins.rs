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

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{Cell, CellRef, ManualWeighIn, SheetName},
    landing::{FileProvenance, LandedRecord},
    measure::Kg,
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
};
use jiff::civil::Date;

use super::sheet::{Sheet, Value};
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
        super::translate_with(record, "weigh-ins", |file, sheets, scribe| {
            Ok(read_weigh_ins(record, file, sheets, scribe))
        })
    }
}

/// Every weigh-in in a workbook's sheets.
pub(super) fn read_weigh_ins(
    record: &LandedRecord,
    file: &FileProvenance,
    sheets: &[Sheet],
    scribe: &mut Scribe,
) -> Vec<ManualWeighIn> {
    let mut weigh_ins = Vec::new();
    for sheet in sheets {
        read_sheet(record, file, sheet, scribe, &mut weigh_ins);
    }
    weigh_ins
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
        Value::Day(_) | Value::Time(_) | Value::Other(_) => {
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
        Value::Empty | Value::Number(_) | Value::Time(_) | Value::Other(_) => {
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
