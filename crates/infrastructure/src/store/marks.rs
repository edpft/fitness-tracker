//! How far a reader has read raw, in the reader's own terms (#571).
//!
//! **The last landing id of each table, keyed by its stream.** Landing ids are
//! one sequence per table, and raw is append-only, so a table read up to an id
//! is every record that table held at the time and none it has gained since.
//! The use case keeps the mark without reading it; only the readers here do.

use std::collections::BTreeMap;

use application::{ReadUpTo, Reading, StoreError};
use domain::landing::LandedRecord;

/// The last landing id read from each table, by stream.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks(BTreeMap<String, i64>);

impl Marks {
    /// The mark a reading names, or `None` for a whole reading.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] if the mark is not one a reader here wrote.
    pub fn of(reading: &Reading) -> Result<Option<Self>, StoreError> {
        let Reading::Since(up_to) = reading else {
            return Ok(None);
        };
        let mut marks = BTreeMap::new();
        for entry in up_to.as_str().split(';').filter(|entry| !entry.is_empty()) {
            let parsed = entry
                .split_once('=')
                .and_then(|(stream, id)| Some((stream.to_owned(), id.parse::<i64>().ok()?)));
            let Some((stream, id)) = parsed else {
                return Err(StoreError::Corrupt {
                    detail: format!("{entry:?} is not how far a table was read"),
                });
            };
            marks.insert(stream, id);
        }
        Ok(Some(Self(marks)))
    }

    /// Whether the record was in raw up to the mark. A table the mark does not
    /// name had nothing in it then.
    pub fn holds(&self, record: &LandedRecord) -> bool {
        self.0
            .get(&record.record().stream().to_string())
            .is_some_and(|last| record.id().as_i64() <= *last)
    }

    /// The records raw held up to the mark, oldest first as given.
    pub fn held(&self, records: &[LandedRecord]) -> Vec<LandedRecord> {
        records
            .iter()
            .filter(|record| self.holds(record))
            .cloned()
            .collect()
    }
}

/// How far these records reach: the last of each table among them.
///
/// **Every record read, not only the ones an account composes**: a companion
/// whose activity has not landed is still read, and reading it again next time
/// would find it new.
pub fn up_to<'a>(records: impl IntoIterator<Item = &'a LandedRecord>) -> ReadUpTo {
    let mut marks: BTreeMap<String, i64> = BTreeMap::new();
    for record in records {
        let last = marks
            .entry(record.record().stream().to_string())
            .or_insert(i64::MIN);
        *last = (*last).max(record.id().as_i64());
    }
    ReadUpTo::from(
        marks
            .iter()
            .map(|(stream, id)| format!("{stream}={id}"))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[cfg(test)]
mod tests {
    use application::{ReadUpTo, Reading};
    use domain::landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    };

    use super::{Marks, up_to};

    fn landed(stream: &str, id: i64) -> LandedRecord {
        let record = LandingRecord::land(
            LandingStream::try_from(stream).expect("a stream"),
            FetchedAt::try_from("2026-10-25T20:00:00Z").expect("a time"),
            SourceRecordId::try_from(format!("r{id}")).expect("an id"),
            EventProvenance::new(
                Endpoint::try_from("/v1/things").expect("an endpoint"),
                EventKind::Updated,
                None,
            )
            .into(),
            RawPayload::try_from(b"{}".to_vec()).expect("a payload"),
        );
        LandedRecord::new(LandingRecordId::try_from(id).expect("an id"), record)
    }

    #[test]
    fn a_mark_reads_back_as_the_last_record_of_each_table() {
        let records = [
            landed("garmin.activities", 3),
            landed("garmin.activities", 9),
            landed("garmin.exercise_sets", 4),
        ];
        let mark = up_to(records.iter());
        assert_eq!(mark.as_str(), "garmin.activities=9;garmin.exercise_sets=4");

        let marks = Marks::of(&Reading::Since(mark))
            .expect("a mark it wrote")
            .expect("a mark");
        assert!(marks.holds(&landed("garmin.activities", 9)));
        assert!(!marks.holds(&landed("garmin.activities", 10)));
        assert!(!marks.holds(&landed("garmin.activity_files", 1)));
    }

    #[test]
    fn a_whole_reading_has_no_mark() {
        assert_eq!(Marks::of(&Reading::Whole).expect("a reading"), None);
    }

    #[test]
    fn a_mark_no_reader_wrote_is_corrupt() {
        assert!(Marks::of(&Reading::Since(ReadUpTo::from("nonsense".to_owned()))).is_err());
    }
}
