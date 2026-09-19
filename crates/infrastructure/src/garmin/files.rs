//! The HTTP adapter for Garmin's activity files.
//!
//! **Every activity's original recording.** The activity list carries a
//! session's `averageHR` and `maxHR`; the samples themselves — heart rate at the
//! resolution the watch recorded it, and everything else it wrote — are in the
//! FIT file, which `/download-service/files/activity/{id}` serves inside an
//! archive. The bytes land as served (§ II.1): reading the archive, and the FIT
//! inside it, is normalisation's work.
//!
//! **Every activity, no filter.** The operator, 2026-09-18, asked whether to
//! fetch these for strength sessions or for everything: *"Everything"* —
//! *"Those are my FIT files!"* They are his recordings, and collecting them is
//! the point whatever this build goes on to read from them (#175).
//!
//! **Enumerating the list again rather than reading the other stream's table**,
//! for the reason [`crate::peloton::samples`] gives, and composed from
//! [`GarminActivities`] as [`super::exercise_sets`] is.
//!
//! **A file older than the resumption point is not fetched.** A finished
//! activity's recording is what the watch wrote.
//!
//! **An activity with no file is passed over, not a failure.** Garmin answers
//! 404, `"Uploaded file not found for activity"`, for some activities — on the
//! operator's account a 2018 ride from the vívoactive 3, which the list gives no
//! sign of: not manual, a device id, a GPS track. It is the source stating there
//! is nothing to fetch, so nothing lands; any other failure still ends the walk.

use std::{sync::Arc, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{Endpoint, EventKind, EventProvenance, PayloadDigest, RawPayload, Watermark};

use super::{
    activities::{ActivityPage, GarminActivities},
    auth::GarminAuth,
};

/// The file's path for one activity.
fn file_path(activity: &str) -> String {
    format!("/download-service/files/activity/{activity}")
}

/// How long to wait between one activity's file and the next.
///
/// Two thousand downloads in a row against a source that rate-limits with 429s;
/// politeness rather than a documented requirement, as on the list.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// What the next serving of this file is compared against.
///
/// **Garmin builds the archive at download time.** A second run landed all
/// 1,450 files again, and the two servings of one differed in four bytes: the
/// modification time of the archive's one entry, in its local header and again
/// in its central directory. The FIT inside was byte-identical, CRC and all. So
/// the comparison is the archive with every entry's time and date zeroed —
/// [`super::activities`]'s remedy for its unstable key order, applied to a
/// different wrapper. The bytes still land exactly as served (§ II.1).
///
/// Falls back to the payload's own digest when the archive will not read,
/// which is the safe direction: it lands again rather than being wrongly
/// judged unchanged.
fn revision_of(payload: &RawPayload) -> PayloadDigest {
    without_timestamps(payload.as_bytes())
        .and_then(|stable| RawPayload::try_from(stable.as_slice()).ok())
        .map_or_else(|| payload.digest(), |stable| stable.digest())
}

/// A zip's signatures, little-endian as they sit in the file.
const END_OF_DIRECTORY: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const DIRECTORY_ENTRY: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const LOCAL_HEADER: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

/// The archive with each entry's modification time and date set to zero, or
/// `None` if it is not an archive this can read.
///
/// Walks the central directory from its end record, and zeroes the time and
/// date in each directory entry and in the local header it points at. Offsets
/// are the zip specification's (APPNOTE 4.3.7, 4.3.12, 4.3.16).
fn without_timestamps(archive: &[u8]) -> Option<Vec<u8>> {
    let last_start = archive.len().checked_sub(22)?;
    let end = archive
        .get(..last_start.checked_add(4)?)?
        .windows(4)
        .rposition(|window| window == END_OF_DIRECTORY)?;

    let entries = u16_at(archive, end.checked_add(10)?)?;
    let mut at = usize::try_from(u32_at(archive, end.checked_add(16)?)?).ok()?;
    let mut stable = archive.to_vec();

    for _ in 0..entries {
        if archive.get(at..at.checked_add(4)?)? != DIRECTORY_ENTRY {
            return None;
        }
        let local = usize::try_from(u32_at(archive, at.checked_add(42)?)?).ok()?;
        if archive.get(local..local.checked_add(4)?)? != LOCAL_HEADER {
            return None;
        }
        stable
            .get_mut(at.checked_add(12)?..at.checked_add(16)?)?
            .fill(0);
        stable
            .get_mut(local.checked_add(10)?..local.checked_add(14)?)?
            .fill(0);

        let name = usize::from(u16_at(archive, at.checked_add(28)?)?);
        let extra = usize::from(u16_at(archive, at.checked_add(30)?)?);
        let comment = usize::from(u16_at(archive, at.checked_add(32)?)?);
        at = at
            .checked_add(46)?
            .checked_add(name)?
            .checked_add(extra)?
            .checked_add(comment)?;
    }

    Some(stable)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(u16::from_le_bytes(pair))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let quad: [u8; 4] = bytes.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(quad))
}

/// Where a walk of the files has got to.
///
/// The list's own position, because it is the list's enumeration.
pub type ActivityFilePage = ActivityPage;

/// Garmin's activity files, one per activity.
#[derive(Debug)]
pub struct GarminActivityFiles {
    /// The activity list, which is how an activity's file is found at all.
    activities: GarminActivities,
}

impl GarminActivityFiles {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<GarminAuth>>) -> Self {
        Self {
            activities: GarminActivities::new(api_base, auth),
        }
    }
}

impl WorkoutEventSource for GarminActivityFiles {
    type Resume = ActivityFilePage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<ActivityFilePage>,
    ) -> Result<EventBatch<ActivityFilePage>, SourceError> {
        let listed = self.activities.fetch(since, resume).await?;

        let mut events = Vec::new();
        for activity in listed.events {
            let occurred_at = activity.provenance.occurred_at();
            if let (Some(at), Some(mark)) = (occurred_at, since)
                && at.as_timestamp() < mark.as_timestamp()
            {
                continue;
            }

            if !events.is_empty() {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }

            let path = file_path(activity.source_record_id.as_str());
            let endpoint =
                Endpoint::try_from(path.clone()).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;

            let Some(body) = self.activities.get_bytes_if_any(&path).await? else {
                continue;
            };
            let payload =
                RawPayload::try_from(body.as_slice()).map_err(|error| SourceError::Malformed {
                    detail: format!("{path} answered nothing: {error}"),
                })?;

            // **The activity's time, not the file's**, as the sets take theirs:
            // the resumption point advances on something the list stated.
            events.push(SourceEvent {
                source_record_id: activity.source_record_id,
                provenance: EventProvenance::new(endpoint, EventKind::Updated, occurred_at).into(),
                revision: revision_of(&payload),
                payload,
            });
        }

        Ok(EventBatch {
            events,
            resume: listed.resume,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{RawPayload, file_path, revision_of};
    use domain::landing::Endpoint;

    /// One stored entry named `a.fit` holding `FIT`, stamped with `time` and
    /// `date` — the shape Garmin serves, built by hand.
    fn archive(time: [u8; 2], date: [u8; 2]) -> Vec<u8> {
        let mut zip = Vec::new();
        // Local header, then the name and the three bytes of data.
        zip.extend([0x50, 0x4b, 0x03, 0x04, 20, 0, 0, 0, 0, 0]);
        zip.extend(time);
        zip.extend(date);
        zip.extend([0x11, 0x22, 0x33, 0x44, 3, 0, 0, 0, 3, 0, 0, 0, 5, 0, 0, 0]);
        zip.extend(b"a.fitFIT");
        let directory = zip.len();
        // The directory entry pointing back at offset 0.
        zip.extend([0x50, 0x4b, 0x01, 0x02, 20, 0, 20, 0, 0, 0, 0, 0]);
        zip.extend(time);
        zip.extend(date);
        zip.extend([0x11, 0x22, 0x33, 0x44, 3, 0, 0, 0, 3, 0, 0, 0, 5, 0, 0, 0]);
        zip.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        zip.extend(b"a.fit");
        let size = zip.len() - directory;
        // The end record: one entry, the directory's size and offset.
        zip.extend([0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 1, 0, 1, 0]);
        zip.extend(u32::try_from(size).expect("small").to_le_bytes());
        zip.extend(u32::try_from(directory).expect("small").to_le_bytes());
        zip.extend([0, 0]);
        zip
    }

    fn payload(bytes: &[u8]) -> RawPayload {
        RawPayload::try_from(bytes).expect("a payload")
    }

    /// **The live failure this exists for.** Two servings of one recording, an
    /// hour and a half apart, differed only in the entry's time.
    #[test]
    fn two_servings_of_one_recording_are_one_revision() {
        let first = payload(&archive([0x0e, 0xe7], [0x32, 0x5b]));
        let later = payload(&archive([0x48, 0xf5], [0x33, 0x5b]));

        assert_ne!(first.digest(), later.digest(), "the bytes differ");
        assert_eq!(revision_of(&first), revision_of(&later));
    }

    /// A recording that really changed is still a new serving.
    #[test]
    fn a_changed_recording_is_a_new_revision() {
        let before = archive([0, 0], [0, 0]);
        let mut after = before.clone();
        let data = before
            .windows(3)
            .position(|window| window == b"FIT")
            .expect("the data");
        after[data] = b'X';

        assert_ne!(
            revision_of(&payload(&before)),
            revision_of(&payload(&after))
        );
    }

    /// Anything that is not an archive compares on everything.
    #[test]
    fn what_will_not_read_falls_back_to_its_own_digest() {
        let unreadable = payload(b"not an archive at all, and long enough to scan");
        assert_eq!(revision_of(&unreadable), unreadable.digest());
    }

    #[test]
    fn the_file_path_is_an_endpoint() {
        let endpoint = Endpoint::try_from(file_path("21234567890")).expect("an endpoint");
        assert_eq!(
            endpoint.as_str(),
            "/download-service/files/activity/21234567890"
        );
    }

    /// **The base URL carries no path segment**, so composing it with this
    /// cannot produce a doubled prefix. A stub cannot catch a wrong default.
    #[test]
    fn the_path_composes_against_a_bare_root() {
        assert!(file_path("1").starts_with('/'));
        assert!(!file_path("1").contains("//"));
    }
}
