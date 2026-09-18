//! The HTTP adapter for Garmin's per-activity exercise sets.
//!
//! **The list summarises a gym session per movement, not per set.** An
//! activity's `summarizedExerciseSets` states `sets`, `reps`, `volume` and
//! `maxWeight` for each category across the whole session, and turning
//! `sets: 3, reps: 24` into three sets of eight would be inventing observations
//! the source never made. `/activity-service/activity/{id}/exerciseSets` is the
//! per-activity sub-resource that may hold the sets themselves; whether it does
//! is read off what lands here (#173, #172).
//!
//! **Enumerating the list again rather than reading the other stream's table**,
//! for the reason [`crate::peloton::samples`] gives: an adapter reaching into
//! another adapter's table would couple two streams and mean this one could not
//! run until the other had. The list walk is composed, not reimplemented, so its
//! paging and watermark are the list's own.
//!
//! **Only where the list says there is something to fetch.** An activity whose
//! `summarizedExerciseSets` is absent or empty has told us it recorded none, and
//! asking anyway would be 2,000 requests to hear it again. The test is the
//! source's own statement, not the activity's type: on the operator's account
//! it selects the 299 strength sessions with data *and* two indoor-cardio ones,
//! and a filter on `strength_training` would have been this adapter cutting the
//! source along a category of its own choosing (§ II.3).
//!
//! **A set list older than the resumption point is not fetched**, as a
//! Peloton graph is not: the walk asks about activities the list serves on or
//! after it, and the boundary again because the port defines `since` as
//! inclusive.

use std::{sync::Arc, time::Duration};

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{Endpoint, EventKind, EventProvenance, RawPayload, Watermark};
use serde::Deserialize;
use serde_json::value::RawValue;

use super::{
    activities::{ActivityPage, GarminActivities, revision_of},
    auth::GarminAuth,
};

/// The sets' path for one activity.
fn sets_path(activity: &str) -> String {
    format!("/activity-service/activity/{activity}/exerciseSets")
}

/// How long to wait between one activity's sets and the next.
///
/// Three hundred requests in a row against a source that rate-limits with 429s;
/// politeness rather than a documented requirement, as on the list.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// What this walk reads of an activity on the list. Nothing else is parsed.
#[derive(Debug, Deserialize)]
struct Summary {
    #[serde(default, rename = "summarizedExerciseSets")]
    summarized_exercise_sets: Option<Vec<Box<RawValue>>>,
}

/// Whether the list says this activity has exercise sets to ask for.
///
/// An activity whose payload will not parse is asked about: guessing that it
/// has nothing would decide, on no evidence, that it is never collected.
fn has_exercise_data(listed: &RawPayload) -> bool {
    serde_json::from_slice::<Summary>(listed.as_bytes()).map_or(true, |summary| {
        summary
            .summarized_exercise_sets
            .is_some_and(|sets| !sets.is_empty())
    })
}

/// Where a walk of the sets has got to.
///
/// The list's own position, because it is the list's enumeration.
pub type ExerciseSetPage = ActivityPage;

/// Garmin's exercise sets, one answer per activity that has any.
#[derive(Debug)]
pub struct GarminExerciseSets {
    /// The activity list, which is how an activity's sets are found at all.
    activities: GarminActivities,
}

impl GarminExerciseSets {
    pub fn new(api_base: impl Into<String>, auth: impl Into<Arc<GarminAuth>>) -> Self {
        Self {
            activities: GarminActivities::new(api_base, auth),
        }
    }
}

impl WorkoutEventSource for GarminExerciseSets {
    type Resume = ExerciseSetPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<ExerciseSetPage>,
    ) -> Result<EventBatch<ExerciseSetPage>, SourceError> {
        // The activities this page names, and whether the walk continues. The
        // list stops at the page that crosses the watermark and serves the rest
        // of that page; the older ones on it are skipped below.
        let listed = self.activities.fetch(since, resume).await?;

        let mut events = Vec::new();
        for activity in listed.events {
            let occurred_at = activity.provenance.occurred_at();
            if let (Some(at), Some(mark)) = (occurred_at, since)
                && at.as_timestamp() < mark.as_timestamp()
            {
                continue;
            }
            if !has_exercise_data(&activity.payload) {
                continue;
            }

            if !events.is_empty() {
                tokio::time::sleep(BETWEEN_REQUESTS).await;
            }

            let path = sets_path(activity.source_record_id.as_str());
            let endpoint =
                Endpoint::try_from(path.clone()).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;

            let body = self.activities.get(&path, &[]).await?;
            let payload =
                RawPayload::try_from(body.as_bytes()).map_err(|error| SourceError::Malformed {
                    detail: format!("{path} answered nothing: {error}"),
                })?;

            events.push(SourceEvent {
                source_record_id: activity.source_record_id,
                // **The activity's time, not the answer's**, as a Peloton graph
                // takes its workout's: the resumption point has to advance on
                // something the source said.
                provenance: EventProvenance::new(endpoint, EventKind::Updated, occurred_at).into(),
                // The same source, so the same unstable key order: compared on
                // the canonical form, landed as served.
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
    use super::{RawPayload, has_exercise_data, sets_path};
    use domain::landing::Endpoint;

    fn payload(json: &str) -> RawPayload {
        RawPayload::try_from(json.as_bytes()).expect("a payload")
    }

    #[test]
    fn the_sets_path_is_an_endpoint() {
        let endpoint = Endpoint::try_from(sets_path("21234567890")).expect("an endpoint");
        assert_eq!(
            endpoint.as_str(),
            "/activity-service/activity/21234567890/exerciseSets"
        );
    }

    /// **The base URL carries no path segment**, so composing it with this
    /// cannot produce a doubled prefix. A stub cannot catch a wrong default.
    #[test]
    fn the_path_composes_against_a_bare_root() {
        assert!(sets_path("1").starts_with('/'));
        assert!(!sets_path("1").contains("//"));
    }

    /// The shape the operator's account serves for a gym session.
    #[test]
    fn a_summarised_session_is_asked_about() {
        assert!(has_exercise_data(&payload(
            r#"{"activityId":1,"summarizedExerciseSets":[{"category":"SQUAT","sets":3,"reps":24}]}"#
        )));
    }

    /// 1,037 activities on the operator's account carry an empty list; asking
    /// about them is a request to hear "nothing" again.
    #[test]
    fn an_empty_summary_is_not_asked_about() {
        assert!(!has_exercise_data(&payload(
            r#"{"activityId":1,"summarizedExerciseSets":[]}"#
        )));
    }

    #[test]
    fn an_activity_without_a_summary_is_not_asked_about() {
        assert!(!has_exercise_data(&payload(r#"{"activityId":1}"#)));
    }

    /// **The source's statement, not the type.** Indoor cardio with a skipping
    /// set is asked about as a strength session is.
    #[test]
    fn the_type_does_not_decide() {
        assert!(has_exercise_data(&payload(
            r#"{"activityId":1,"activityType":{"typeKey":"indoor_cardio"},"summarizedExerciseSets":[{"category":"CARDIO"}]}"#
        )));
    }

    /// Guessing that an unreadable activity has nothing would decide, on no
    /// evidence, that it is never collected.
    #[test]
    fn an_unreadable_activity_is_asked_about() {
        assert!(has_exercise_data(&payload("not json")));
    }
}
