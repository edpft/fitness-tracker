//! The HTTP adapter for Peloton's workout detail (#271).
//!
//! **One request per workout**, against `/api/workout/{id}`, enumerated from
//! the list the way [`super::samples`] enumerates graphs and for the same
//! reasons.
//!
//! **What the detail adds to the list entry**, against the operator's account
//! on 2026-10-21: `ftp_info` (the FTP in force for the workout, and whether a
//! test or the operator set it), `achievement_templates` (the badges it
//! earned), the leaderboard rank and its field, `ratings`, pause fields and
//! `strength_plan_id`. Nothing here reads any of it.
//!
//! **A detail older than the resumption point is not fetched**, as with a
//! graph: two fetches of the same workouts a day apart answered byte-for-byte
//! the same, rank included.

use application::{EventBatch, SourceError, WorkoutEventSource};
use domain::landing::{Endpoint, EventKind, EventProvenance, RawPayload, Watermark};

use super::{auth::PelotonAuth, samples::wanted, workouts::PelotonWorkouts};

/// The detail's path for one workout.
fn detail_path(workout: &str) -> String {
    format!("/api/workout/{workout}")
}

/// Where a walk of the details has got to: the same enumeration as the list.
pub type DetailPage = super::workouts::WorkoutPage;

/// Peloton's workout detail.
#[derive(Debug)]
pub struct PelotonWorkoutDetails {
    /// The workout list, which is how a workout is found at all.
    workouts: PelotonWorkouts,
}

impl PelotonWorkoutDetails {
    pub fn new(api_base: impl Into<String>, auth: PelotonAuth) -> Self {
        Self {
            workouts: PelotonWorkouts::new(api_base, auth),
        }
    }
}

impl WorkoutEventSource for PelotonWorkoutDetails {
    type Resume = DetailPage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<DetailPage>,
    ) -> Result<EventBatch<DetailPage>, SourceError> {
        let listed = self.workouts.fetch(since, resume).await?;

        let mut events = Vec::with_capacity(listed.events.len());
        for workout in listed.events {
            let occurred_at = workout.provenance.occurred_at();
            if !wanted(occurred_at, since) {
                continue;
            }

            let path = detail_path(workout.source_record_id.as_str());
            let endpoint =
                Endpoint::try_from(path.clone()).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;

            let body = self.workouts.get(&path, &[]).await?;
            let payload =
                RawPayload::try_from(body.as_slice()).map_err(|error| SourceError::Malformed {
                    detail: format!("{path} answered nothing: {error}"),
                })?;

            events.push(crate::revision::json_event(
                workout.source_record_id,
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
    use super::detail_path;
    use domain::landing::Endpoint;

    /// The path a provenance records must be a path, whatever the workout id is.
    #[test]
    fn the_detail_path_is_an_endpoint() {
        let endpoint = Endpoint::try_from(detail_path("608297b6a3684d9b873f6938a3bfa190"))
            .expect("an endpoint");
        assert_eq!(
            endpoint.as_str(),
            "/api/workout/608297b6a3684d9b873f6938a3bfa190"
        );
    }

    /// **The base URL carries no path segment**, so composing it with this
    /// cannot produce `/api/api/…`. A stub cannot catch a wrong default.
    #[test]
    fn the_path_composes_against_a_bare_root() {
        assert!(detail_path("w").starts_with("/api/"));
        assert!(!detail_path("w").contains("//"));
    }
}
