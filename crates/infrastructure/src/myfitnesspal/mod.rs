//! MyFitnessPal's export (#242), derived as the weights the operator logged in
//! it.
//!
//! **One sheet, one row per thing logged**: foods, exercise, steps, daily
//! totals and measurements, told apart by `item_type`. Only a measurement of
//! weight is a weigh-in; the export of 2026-09-28 holds no other kind, and a
//! measurement of anything else is refused rather than guessed at.
//!
//! **A weight is the day it was logged against**, which is the diary's
//! `date`. `updated_at` is when it was typed, which is usually minutes after
//! stepping off the scale and sometimes days, so it is carried as when it was
//! entered and never as when it was weighed. `created_at` is empty on every
//! row.
//!
//! **Several exports are one account sent more than once**, as Beyond The
//! White Board's are: each day is read from the export landed last that holds
//! it, and an earlier export adds only the days the later ones lack.

use std::collections::BTreeMap;

use application::{
    NormalisationError, Translation,
    ports::{SourceAccount, Translator},
};
use domain::{
    body::{Cell, CellRef, Entry, ManualWeighIn, SheetName},
    landing::LandedRecord,
    measure::Kg,
    normalised::{Refusal, RefusalLocus, RefusalReason, StatedZones},
    sequence::NonEmpty,
};
use jiff::{Timestamp, civil::Date};

use crate::{
    scribe::Scribe,
    spreadsheets::sheet::{Sheet, Value, workbook},
};

/// The columns a weigh-in is read from, by the names the export gives them.
const ITEM_TYPE: &str = "item_type";
const DATE: &str = "date";
const DESCRIPTION: &str = "description";
const VALUE: &str = "value";
const UNIT: &str = "unit";
const UPDATED_AT: &str = "updated_at";

/// How far down the sheet the header may sit. The export's is on the second
/// row, below a title.
const HEADER_WITHIN: u32 = 4;

/// Every export landed, oldest first.
#[derive(Debug, Clone)]
pub struct MyFitnessPalExports {
    files: NonEmpty<LandedRecord>,
}

impl MyFitnessPalExports {
    /// The landed exports as one account, or none if nothing has landed.
    pub fn gather(records: Vec<LandedRecord>) -> Vec<Self> {
        NonEmpty::new(records)
            .map(|files| vec![Self { files }])
            .unwrap_or_default()
    }
}

impl SourceAccount for MyFitnessPalExports {
    fn records(&self) -> usize {
        self.files.count()
    }

    fn landed(&self) -> Vec<&LandedRecord> {
        self.files.iter().collect()
    }
}

/// Reads the weigh-ins out of MyFitnessPal's exports.
#[derive(Debug, Clone, Copy, Default)]
pub struct MyFitnessPalWeighInTranslator;

impl Translator for MyFitnessPalWeighInTranslator {
    type Account = MyFitnessPalExports;
    type Entity = ManualWeighIn;

    /// The zone is not consulted: a weight is logged against a day, and when
    /// it was typed is an instant the export states in UTC.
    fn translate(
        &self,
        exports: &MyFitnessPalExports,
        _zones: &StatedZones,
    ) -> Result<Translation<ManualWeighIn>, NormalisationError> {
        let mut scribes: Vec<Scribe> = Vec::new();
        let mut read: Vec<Vec<ManualWeighIn>> = Vec::new();
        for record in exports.files.iter() {
            let mut scribe = Scribe::new(record);
            read.push(weigh_ins(record, &mut scribe));
            scribes.push(scribe);
        }

        // Each day from the last export landed that holds it.
        let mut days: BTreeMap<Date, ManualWeighIn> = BTreeMap::new();
        for weigh_in in read.into_iter().flatten() {
            days.insert(weigh_in.on(), weigh_in);
        }

        let refusals: Vec<Refusal> = scribes
            .into_iter()
            .flat_map(Scribe::into_refusals)
            .collect();
        if let Ok(entities) = NonEmpty::new(days.into_values().collect()) {
            return Ok(Translation::Entities { entities, refusals });
        }
        if let Ok(refusals) = NonEmpty::new(refusals) {
            return Ok(Translation::Refused(refusals));
        }
        Ok(Scribe::new(exports.files.first())
            .only(RefusalLocus::Record, RefusalReason::NothingTranslatable))
    }
}

/// Every weigh-in in one export, or a refusal saying why it is not one.
fn weigh_ins(record: &LandedRecord, scribe: &mut Scribe) -> Vec<ManualWeighIn> {
    let path = record.provenance().as_file().map_or_else(
        || record.provenance().to_string(),
        |file| file.path().to_string(),
    );
    let mut refuse = |why: &str| {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::Unmodelled {
                detail: format!("{path}, a file that is not a MyFitnessPal export ({why}),"),
            },
        );
    };
    let (Some(file), Ok(sheets)) = (
        record.provenance().as_file(),
        workbook(record.payload().as_bytes()),
    ) else {
        refuse("it is not a workbook");
        return Vec::new();
    };
    let Some((sheet, columns)) = sheets
        .iter()
        .find_map(|sheet| Columns::of(sheet).map(|columns| (sheet, columns)))
    else {
        refuse("no sheet has its header");
        return Vec::new();
    };
    let Ok(sheet_name) = SheetName::try_from(sheet.name.as_str()) else {
        refuse("its sheet has no name");
        return Vec::new();
    };

    let mut weigh_ins = Vec::new();
    for row in columns.header.saturating_add(1)..sheet.height() {
        if sheet.get(row, columns.item_type).text() != Some("Measurement") {
            continue;
        }
        let cell = Cell {
            landed_as: record.id(),
            source_record_id: record.source_record_id().clone(),
            file: file.path().clone(),
            sheet: sheet_name.clone(),
            cell: CellRef::at(row, columns.value),
        };
        if let Some(weigh_in) = weigh_in(sheet, &columns, row, cell, scribe) {
            weigh_ins.push(weigh_in);
        }
    }
    weigh_ins
}

/// Where each column the weigh-ins need sits.
struct Columns {
    header: u32,
    item_type: u32,
    date: u32,
    description: u32,
    value: u32,
    unit: u32,
    updated_at: u32,
}

impl Columns {
    /// The header's columns, if the sheet has the export's header.
    fn of(sheet: &Sheet) -> Option<Self> {
        (0..HEADER_WITHIN).find_map(|header| {
            let column = |name: &str| {
                (0..sheet.width()).find(|&column| sheet.get(header, column).text() == Some(name))
            };
            Some(Self {
                header,
                item_type: column(ITEM_TYPE)?,
                date: column(DATE)?,
                description: column(DESCRIPTION)?,
                value: column(VALUE)?,
                unit: column(UNIT)?,
                updated_at: column(UPDATED_AT)?,
            })
        })
    }
}

/// One measurement row as a weigh-in, or a refusal.
fn weigh_in(
    sheet: &Sheet,
    columns: &Columns,
    row: u32,
    cell: Cell,
    scribe: &mut Scribe,
) -> Option<ManualWeighIn> {
    let at = format!("{}!{}", cell.sheet, cell.cell);
    let mut refuse = |field: &'static str, detail: String| {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::UnreadableValue {
                field,
                detail: format!("{at}: {detail}"),
            },
        );
    };

    let description = sheet.get(row, columns.description);
    if description.text() != Some("weight") {
        refuse(
            "description",
            format!("{description:?} is a measurement this build does not model"),
        );
        return None;
    }
    let unit = sheet.get(row, columns.unit);
    if unit.text() != Some("kilograms") {
        refuse("unit", format!("{unit:?} is not kilograms"));
        return None;
    }

    let mass = match sheet.get(row, columns.value) {
        Value::Number(number) => Kg::try_from(number.to_string()),
        Value::Text(text) => Kg::try_from(text.trim()),
        other => {
            refuse("value", format!("{other:?} is not a mass"));
            return None;
        }
    };
    let mass = match mass {
        Ok(mass) => mass,
        Err(error) => {
            refuse("value", error.to_string());
            return None;
        }
    };

    let on = match sheet.get(row, columns.date) {
        Value::Day(day) => *day,
        Value::Text(text) => match text.trim().parse::<Date>() {
            Ok(day) => day,
            Err(error) => {
                refuse("date", format!("{text:?}: {error}"));
                return None;
            }
        },
        other => {
            refuse("date", format!("{other:?} is not a day"));
            return None;
        }
    };

    let entered_at = match sheet.get(row, columns.updated_at) {
        Value::Text(text) => match text.trim().parse::<Timestamp>() {
            Ok(at) => at,
            Err(error) => {
                refuse("updated_at", format!("{text:?}: {error}"));
                return None;
            }
        },
        other => {
            refuse("updated_at", format!("{other:?} is not an instant"));
            return None;
        }
    };

    match ManualWeighIn::new(on, mass, cell, Entry::Stamped(entered_at)) {
        Ok(weigh_in) => Some(weigh_in),
        Err(error) => {
            refuse("value", error.to_string());
            None
        }
    }
}
