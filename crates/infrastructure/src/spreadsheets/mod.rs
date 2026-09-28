//! The operator's historical spreadsheets (#263), landed from a folder by
//! [`crate::folder::FolderFiles`] and derived here.

use application::{
    NormalisationError, Translation,
    ports::{SourceAccount, Translator},
};
use domain::{
    body::ManualWeighIn,
    gym::{Logged, ManualGymSession},
    landing::{Cell, FileProvenance, LandedRecord, SheetCell, SourceRecordId},
    normalised::{NormalisedEntity, OperatorZone, Refusal, RefusalLocus, RefusalReason},
    sequence::NonEmpty,
};
use jiff::civil::Date;

use self::sheet::Sheet;
use crate::scribe::Scribe;

mod sessions;
mod sheet;
mod weigh_ins;

pub use sessions::SpreadsheetSessionTranslator;
pub use weigh_ins::SpreadsheetWeighInTranslator;

/// Everything one spreadsheet derives, each thing an entity of its own.
///
/// A sum rather than two derivations because the stream is one: one landed
/// file can hold both a year of weigh-ins and a training log, and the stream
/// has one run log and one set of refusals (#274).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpreadsheetEntity {
    WeighIn(ManualWeighIn),
    GymSession(ManualGymSession),
}

impl NormalisedEntity for SpreadsheetEntity {
    fn composes(&self) -> Vec<&SourceRecordId> {
        match self {
            Self::WeighIn(weigh_in) => weigh_in.composes(),
            Self::GymSession(session) => session.composes(),
        }
    }
}

/// Every landed copy of one workbook, oldest first.
///
/// **Copies of a workbook are one record revised, not several recordings**
/// (operator, 2026-09-28), so they are translated together: a file and its
/// conflicted copies are one account. A conflicted copy is as old as the date
/// in its name, and the file under the workbook's own name is the most recent.
/// Two landings of one path are the same file edited, the later the more
/// recent.
#[derive(Debug, Clone)]
pub struct Workbook {
    copies: NonEmpty<LandedRecord>,
}

impl Workbook {
    /// A workbook with one copy.
    pub const fn of(record: LandedRecord) -> Self {
        Self {
            copies: NonEmpty::of(record, Vec::new()),
        }
    }

    /// The landed files, gathered into workbooks, in the order each workbook
    /// was first landed.
    pub fn gather(records: Vec<LandedRecord>) -> Vec<Self> {
        let mut workbooks: Vec<(String, Vec<Dated>)> = Vec::new();
        for record in records {
            let (name, made) = record.provenance().as_file().map_or_else(
                || (format!("landing record {}", record.id()), None),
                |file| copy_of(file.path().as_str()),
            );
            match workbooks.iter_mut().find(|(kept, _)| *kept == name) {
                Some((_, copies)) => copies.push((made, record)),
                None => workbooks.push((name, vec![(made, record)])),
            }
        }
        workbooks
            .into_iter()
            .filter_map(|(_, mut copies)| {
                copies.sort_by_key(|(made, record)| (made.is_none(), *made, record.id().as_i64()));
                let copies = copies.into_iter().map(|(_, record)| record).collect();
                NonEmpty::new(copies).ok().map(|copies| Self { copies })
            })
            .collect()
    }

    /// Oldest first.
    pub const fn copies(&self) -> &NonEmpty<LandedRecord> {
        &self.copies
    }
}

impl SourceAccount for Workbook {
    fn records(&self) -> usize {
        self.copies.count()
    }
}

/// A landed copy, with the date in its name if it is a conflicted copy.
type Dated = (Option<Date>, LandedRecord);

/// The workbook a path is a copy of, and the date of the copy if it is a
/// conflicted one: `CT 2017 (Netbook's conflicted copy 2017-03-29).xlsx` is a
/// copy of `CT 2017.xlsx` made on 29 March 2017.
fn copy_of(path: &str) -> (String, Option<Date>) {
    let conflicted = || {
        let open = path.rfind(" (")?;
        let rest = path.get(open.saturating_add(2)..)?;
        let close = rest.find(')')?;
        let (_, date) = rest.get(..close)?.split_once("conflicted copy ")?;
        let date = date.trim().parse::<Date>().ok()?;
        let name = format!(
            "{}{}",
            path.get(..open)?,
            rest.get(close.saturating_add(1)..)?
        );
        Some((name, date))
    };
    conflicted().map_or_else(
        || (path.to_owned(), None),
        |(name, date)| (name, Some(date)),
    )
}

/// One copy of a workbook, opened as sheets.
struct Opened<'record> {
    record: &'record LandedRecord,
    file: &'record FileProvenance,
    sheets: Vec<Sheet>,
}

impl Opened<'_> {
    fn logged(&self) -> Logged {
        Logged {
            landed_as: self.record.id(),
            source_record_id: self.record.source_record_id().clone(),
            file: self.file.path().clone(),
        }
    }

    /// A cell of this copy.
    fn cell(&self, at: SheetCell) -> Cell {
        Cell {
            landed_as: self.record.id(),
            source_record_id: self.record.source_record_id().clone(),
            file: self.file.path().clone(),
            sheet: at.sheet,
            cell: at.cell,
        }
    }
}

/// Reads weigh-ins and gym sessions out of a landed spreadsheet, in one pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpreadsheetTranslator;

impl Translator for SpreadsheetTranslator {
    type Account = Workbook;
    type Entity = SpreadsheetEntity;

    /// The zone is not consulted: everything a sheet records is a day.
    fn translate(
        &self,
        workbook: &Workbook,
        _zone: &OperatorZone,
    ) -> Result<Translation<SpreadsheetEntity>, NormalisationError> {
        translate_with(workbook, "weigh-ins or gym sessions", |copies, scribes| {
            let mut entities = Vec::new();
            for (copy, scribe) in copies.iter().zip(scribes.iter_mut()) {
                entities.extend(
                    weigh_ins::read_weigh_ins(copy.record, copy.file, &copy.sheets, scribe)
                        .into_iter()
                        .map(SpreadsheetEntity::WeighIn),
                );
            }
            if let Some(scribe) = scribes.last_mut() {
                entities.extend(
                    sessions::read_sessions(copies, scribe)?
                        .into_iter()
                        .map(SpreadsheetEntity::GymSession),
                );
            }
            Ok(entities)
        })
    }
}

/// Open every copy of a workbook as sheets, let `read` find what they hold,
/// and say what became of them.
///
/// `read` is given the copies that opened, oldest first, and a scribe for each.
/// Weigh-ins are each copy's own; what is merged across copies is noted by
/// the most recent copy's scribe.
///
/// A workbook that yields nothing and refuses nothing is a spreadsheet of
/// something else — a food diary, a triathlon plan — and that is said once per
/// copy, as the one reason, because an account that yields nothing says why
/// (§ 37).
fn translate_with<E>(
    workbook: &Workbook,
    what: &str,
    read: impl FnOnce(&[Opened<'_>], &mut [Scribe]) -> Result<Vec<E>, NormalisationError>,
) -> Result<Translation<E>, NormalisationError> {
    let mut unopened: Vec<Refusal> = Vec::new();
    let mut copies = Vec::new();
    let mut scribes = Vec::new();
    for record in workbook.copies().iter() {
        let mut scribe = Scribe::new(record);
        match open(record, &mut scribe) {
            Some(opened) => {
                copies.push(opened);
                scribes.push(scribe);
            }
            None => unopened.extend(scribe.into_refusals()),
        }
    }

    let entities = read(&copies, &mut scribes)?;
    let mut refusals = unopened;
    for scribe in scribes {
        refusals.extend(scribe.into_refusals());
    }
    if let Ok(entities) = NonEmpty::new(entities) {
        return Ok(Translation::Entities { entities, refusals });
    }
    if let Ok(refusals) = NonEmpty::new(refusals) {
        return Ok(Translation::Refused(refusals));
    }
    let nothing: Vec<Refusal> = copies
        .iter()
        .flat_map(|copy| {
            let mut scribe = Scribe::new(copy.record);
            scribe.note(
                RefusalLocus::Record,
                RefusalReason::Unmodelled {
                    detail: format!(
                        "{}, a spreadsheet of something other than {what},",
                        copy.file.path()
                    ),
                },
            );
            scribe.into_refusals()
        })
        .collect();
    Ok(NonEmpty::new(nothing).map_or_else(
        |_| {
            Scribe::new(workbook.copies().first())
                .only(RefusalLocus::Record, RefusalReason::NothingTranslatable)
        },
        Translation::Refused,
    ))
}

/// One landed file as sheets, or a refusal saying why it is not a spreadsheet.
fn open<'record>(record: &'record LandedRecord, scribe: &mut Scribe) -> Option<Opened<'record>> {
    let Some(file) = record.provenance().as_file() else {
        scribe.note(
            RefusalLocus::Record,
            RefusalReason::UnreadablePayload {
                detail: format!("{} was served by a feed, not a folder", record.provenance()),
            },
        );
        return None;
    };

    let bytes = record.payload().as_bytes();
    let sheets = if file.path().as_str().to_lowercase().ends_with(".csv") {
        sheet::csv(bytes, file)
    } else {
        sheet::workbook(bytes)
    };
    match sheets {
        Ok(sheets) => Some(Opened {
            record,
            file,
            sheets,
        }),
        Err(detail) => {
            scribe.note(
                RefusalLocus::Record,
                RefusalReason::Unmodelled {
                    detail: format!(
                        "{}, a file that is not a spreadsheet ({detail}),",
                        file.path()
                    ),
                },
            );
            None
        }
    }
}
