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

use std::{sync::Arc, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{Endpoint, EventKind, EventProvenance, RawPayload, Watermark};

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

            let body = self.activities.get_bytes(&path).await?;
            let payload =
                RawPayload::try_from(body.as_slice()).map_err(|error| SourceError::Malformed {
                    detail: format!("{path} answered nothing: {error}"),
                })?;

            // **The activity's time, not the file's**, as the sets take theirs:
            // the resumption point advances on something the list stated.
            events.push(SourceEvent::new(
                activity.source_record_id,
                EventProvenance::new(endpoint, EventKind::Updated, occurred_at).into(),
                payload,
            ));
        }

        Ok(EventBatch {
            events,
            resume: listed.resume,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::file_path;
    use domain::landing::Endpoint;

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
