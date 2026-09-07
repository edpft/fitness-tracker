//! The HTTP adapter for Peloton's performance graphs.
//!
//! **A graph is one request per workout**, against
//! `/api/workout/{id}/performance_graph`. There is no list of graphs, so this
//! walk enumerates workouts the same way [`super::workouts`] does — a page of
//! the list — and then asks for each one's graph.
//!
//! **Enumerating them again rather than reading the other stream's table.** A
//! landing store is a driven port and so is a source; an adapter reaching into
//! another adapter would couple two streams that the catalogue keeps apart on
//! purpose, and would mean this one could not run until the other had. The cost
//! is one list request per page of fifty, which is under two per cent of the
//! requests a walk makes.
//!
//! **What a graph carries**, against the operator's own account: a stream per
//! metric at 1 Hz, `summaries` and `average_summaries`. Cycling has five
//! streams — output, cadence, resistance, speed, heart rate — and every other
//! discipline has heart rate alone. Nothing here reads any of it. The bytes are
//! landed as served and a later derivation decides what they mean.
//!
//! **A graph older than the resumption point is not fetched at all.** The
//! operator, on the first version of this, which re-fetched a whole page every
//! run so that the digests could discard it: *"why would a graph change for an
//! older workout? it's literally sensor data, I don't know how I would change
//! it and I wouldn't even if I could!"* He is right, and it cost thirty seconds
//! of requests on every run to defend against a case with no name. A finished
//! workout's samples are what its sensors recorded.
//!
//! The boundary is still re-fetched, and the reason is thinner than it looks.
//! `WorkoutEventSource::fetch` states that `since` is inclusive, on the grounds
//! that the boundary event "cannot skip a sibling sharing that timestamp" —
//! true of Hevy, whose feed serves *change events* and can stamp several with
//! one second during a bulk edit. **It is not true here.** This source lists
//! workouts, one person starts them one at a time on one bike, and two cannot
//! share a second. The operator, when this was claimed as a reason: *"are you
//! suggesting I might start a yoga workout while still in the middle of a
//! ride?"*
//!
//! So the boundary is kept for consistency with the port's stated contract and
//! for nothing else. It costs one request per run. An exclusive comparison here
//! would save that request and make this the one adapter that reads `since`
//! differently from the way the port defines it.

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{Endpoint, EventKind, EventProvenance, EventTime, RawPayload, Watermark};

use super::{auth::PelotonAuth, workouts::PelotonWorkouts};

/// The graph's path for one workout.
fn graph_path(workout: &str) -> String {
    format!("/api/workout/{workout}/performance_graph")
}

/// Whether this run has any reason to ask for a workout's graph.
///
/// **Strictly-older is skipped, and the boundary is kept.** Inclusive because
/// the port defines `since` that way, not because two of this source's records
/// could share a second — they cannot. See the note at the top of this module.
/// The boundary is asked for again and its digest discards the answer.
///
/// A workout the source gave no time for is always wanted. Guessing that an
/// undated record is old would be inventing the fact that decides whether it is
/// ever collected.
fn wanted(occurred_at: Option<EventTime>, since: Option<Watermark>) -> bool {
    match (occurred_at, since) {
        (Some(at), Some(mark)) => at.as_timestamp() >= mark.as_timestamp(),
        _ => true,
    }
}

/// **Every sample, not a summary of them.** § II.1 says a component observation
/// keeps the resolution the source recorded it at and is never resampled, so
/// this asks for one point per second and takes what it is given. Peloton will
/// thin a graph on request; that request is not made.
const EVERY_N: &str = "1";

/// Where a walk of the graphs has got to.
///
/// The same shape as the workout walk's, because it is the same enumeration.
pub type SamplePage = super::workouts::WorkoutPage;

/// Peloton's performance graphs.
#[derive(Debug)]
pub struct PelotonWorkoutSamples {
    /// The workout list, which is how a graph is found at all.
    ///
    /// Composed rather than reimplemented: paging, the user lookup and the
    /// watermark are that adapter's and there is no second version of them
    /// here to drift.
    workouts: PelotonWorkouts,
}

impl PelotonWorkoutSamples {
    pub fn new(api_base: impl Into<String>, auth: PelotonAuth) -> Self {
        Self {
            workouts: PelotonWorkouts::new(api_base, auth),
        }
    }
}

impl WorkoutEventSource for PelotonWorkoutSamples {
    type Resume = SamplePage;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<SamplePage>,
    ) -> Result<EventBatch<SamplePage>, SourceError> {
        // The workouts this page names, and whether the walk continues. The
        // watermark is applied there, so a graph is never fetched for a workout
        // this run had no reason to look at.
        let listed = self.workouts.fetch(since, resume).await?;

        let mut events = Vec::with_capacity(listed.events.len());
        for workout in listed.events {
            // **The request this saves is the whole point.** A page holds fifty
            // workouts and a steady-state run wants none of them; asking anyway
            // and letting the digests discard the answers cost half a minute of
            // network per run.
            if !wanted(workout.provenance.occurred_at(), since) {
                continue;
            }

            let path = graph_path(workout.source_record_id.as_str());
            let endpoint =
                Endpoint::try_from(path.clone()).map_err(|error| SourceError::Malformed {
                    detail: error.to_string(),
                })?;

            let body = self
                .workouts
                .get(&path, &[("every_n", EVERY_N.to_owned())])
                .await?;

            let payload =
                RawPayload::try_from(body.as_slice()).map_err(|error| SourceError::Malformed {
                    detail: format!("{path} answered nothing: {error}"),
                })?;

            // **The workout's time, not the graph's.** A graph states none of
            // its own, and a resumption point has to advance on something the
            // source said. Read before the id is moved out of the event.
            let occurred_at = workout.provenance.occurred_at();

            events.push(SourceEvent::new(
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
    use super::{EVERY_N, EventTime, Watermark, graph_path, wanted};
    use domain::landing::Endpoint;
    use jiff::Timestamp;

    /// The path a provenance records must be a path, whatever the workout id is.
    #[test]
    fn the_graph_path_is_an_endpoint() {
        let path = graph_path("608297b6a3684d9b873f6938a3bfa190");
        let endpoint = Endpoint::try_from(path).expect("an endpoint");
        assert_eq!(
            endpoint.as_str(),
            "/api/workout/608297b6a3684d9b873f6938a3bfa190/performance_graph"
        );
    }

    /// **The base URL carries no path segment**, so composing it with this
    /// cannot produce `/api/api/…`. A stub cannot catch a wrong default.
    #[test]
    fn the_path_composes_against_a_bare_root() {
        assert!(graph_path("w").starts_with("/api/"));
        assert!(!graph_path("w").contains("//"));
    }

    /// § II.1: a component observation is never resampled, so the walk asks for
    /// every point. A larger number here would be this adapter thinning a
    /// series the source was willing to serve in full.
    #[test]
    fn the_walk_asks_for_every_sample() {
        assert_eq!(EVERY_N, "1");
    }

    /// One second, as this source serves times.
    fn at(second: i64) -> Option<EventTime> {
        Timestamp::from_second(second).ok().map(EventTime::from)
    }

    /// **The saving.** A finished workout's samples are what its sensors
    /// recorded, so a graph from before the resumption point is never asked
    /// for — which is what makes a steady-state run one request rather than
    /// fifty-one.
    #[test]
    fn a_graph_from_before_the_resumption_point_is_not_asked_for() {
        let mark = Timestamp::from_second(1_000).ok().map(Watermark::from);
        assert!(!wanted(at(999), mark));
        assert!(wanted(at(1_001), mark));
    }

    /// **The boundary is kept** because the port defines `since` as inclusive.
    /// Not because two workouts could share a second: one person starts one
    /// workout at a time on one bike. This pins the contract, not a scenario.
    #[test]
    fn the_boundary_workout_is_asked_for_again() {
        let mark = Timestamp::from_second(1_000).ok().map(Watermark::from);
        assert!(wanted(at(1_000), mark));
    }

    /// A first run has no resumption point and wants everything.
    #[test]
    fn a_first_run_asks_for_every_graph() {
        assert!(wanted(at(1), None));
        assert!(wanted(None, None));
    }

    /// Guessing that an undated record is old would invent the fact that
    /// decides whether it is ever collected.
    #[test]
    fn a_workout_without_a_time_is_always_asked_for() {
        let mark = Timestamp::from_second(1_000).ok().map(Watermark::from);
        assert!(wanted(None, mark));
    }
}
