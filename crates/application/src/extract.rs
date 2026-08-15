//! Collecting everything the source has served since the resumption point.

use domain::landing::{
    EventCount, EventTime, LandingRecord, LandingStream, RecordCount, RunId, RunOutcome, Watermark,
};

use crate::{
    error::ExtractionError,
    ports::{
        Clock, EventBatch, ExtractionRunLog, LandingStore, ResumptionPointStore, RunLock,
        RunSummary, SourceEvent, WorkoutEventSource, WorkoutExtractor,
    },
};

/// The extraction use case.
///
/// Generic over every port it uses, so the composition root decides what it
/// talks to and a test can drive the whole of it with fakes and no I/O.
pub struct Extraction<S, L, R, G, K, C> {
    stream: LandingStream,
    source: S,
    landing: L,
    resumption: R,
    runs: G,
    lock: K,
    clock: C,
}

/// The adapters an extraction run needs.
///
/// A struct rather than six positional arguments: at the composition root the
/// names are what make the wiring readable, and two ports of the same shape
/// cannot be swapped by accident.
pub struct ExtractionPorts<S, L, R, G, K, C> {
    pub source: S,
    pub landing: L,
    pub resumption: R,
    pub runs: G,
    pub lock: K,
    pub clock: C,
}

impl<S, L, R, G, K, C> Extraction<S, L, R, G, K, C>
where
    L: LandingStore,
{
    /// No stream argument: the landing store is bound to one table and is
    /// asked which, so the run cannot be built holding a stream its ports
    /// disagree with. See [`LandingStore::stream`].
    pub fn new(ports: ExtractionPorts<S, L, R, G, K, C>) -> Self {
        Self {
            stream: ports.landing.stream().clone(),
            source: ports.source,
            landing: ports.landing,
            resumption: ports.resumption,
            runs: ports.runs,
            lock: ports.lock,
            clock: ports.clock,
        }
    }
}

/// What one walk of the feed collected, before it is recorded.
struct Collected {
    events_seen: EventCount,
    records_landed: RecordCount,
    /// The newest event time this run actually **saw**.
    ///
    /// Not the clock, and not the newest that exists. See
    /// [`Extraction::collect`].
    newest_event: Option<EventTime>,
}

impl<S, L, R, G, K, C> Extraction<S, L, R, G, K, C>
where
    S: WorkoutEventSource + Sync,
    L: LandingStore + Sync,
    R: ResumptionPointStore + Sync,
    G: ExtractionRunLog + Sync,
    K: RunLock + Sync,
    C: Clock + Sync,
{
    /// Walk the feed from `since`, landing what has changed.
    ///
    /// Batches commit as they are read, so a run that fails on the last one
    /// keeps everything before it. Those records are durable and stay durable:
    /// deleting them on failure would be a mutation of raw, and the retry
    /// deduplicates them anyway.
    async fn collect(
        &self,
        run: RunId,
        since: Option<Watermark>,
    ) -> Result<Collected, ExtractionError> {
        let mut resume = None;
        let mut events_seen = 0_usize;
        let mut records_landed = 0_usize;
        let mut newest_event: Option<EventTime> = None;

        loop {
            let EventBatch {
                events,
                resume: next,
            } = self.source.fetch(since, resume).await?;
            let fetched_at = self.clock.now();

            let mut to_land = Vec::with_capacity(events.len());
            for event in events {
                events_seen = events_seen.saturating_add(1);

                // The watermark is built only from event times this run was
                // actually served. An event the source gave no time for
                // contributes nothing rather than borrowing the clock.
                if let Some(at) = event.provenance.occurred_at() {
                    newest_event = Some(newest_event.map_or(at, |seen| seen.max(at)));
                }

                if let Some(record) = self.landed_if_changed(&event, fetched_at).await? {
                    to_land.push(record);
                }
            }

            let landed = self.landing.append(run, to_land).await?;
            records_landed = records_landed.saturating_add(landed.as_usize());

            // The source says when it has finished. Asking it again after that
            // is a question it has already answered.
            match next {
                Some(next) => resume = Some(next),
                None => break,
            }
        }

        Ok(Collected {
            events_seen: EventCount::from(events_seen),
            records_landed: RecordCount::from(records_landed),
            newest_event,
        })
    }

    /// A record, unless the source re-served a payload we already hold.
    ///
    /// The comparison is against the most recent record for that source
    /// record, not against any of them: a workout edited to X, then Y, then
    /// back to X is the source serving three payloads, and the third differs
    /// from what is current even though it matches what came first.
    async fn landed_if_changed(
        &self,
        event: &SourceEvent,
        fetched_at: domain::landing::FetchedAt,
    ) -> Result<Option<LandingRecord>, ExtractionError> {
        let digest = event.payload.digest();
        let held = self.landing.latest_digest(&event.source_record_id).await?;
        if held == Some(digest) {
            return Ok(None);
        }

        Ok(Some(LandingRecord::land(
            self.stream.clone(),
            fetched_at,
            event.source_record_id.clone(),
            event.provenance.clone(),
            event.payload.clone(),
        )))
    }

    /// Record why a run stopped, then hand the failure back.
    ///
    /// Written in its own call rather than alongside the success path so that
    /// a failed run is always visible: the difference between a run that broke
    /// and one that quietly found nothing is the whole of § 38.
    async fn record_failure(&self, run: RunId, error: ExtractionError) -> ExtractionError {
        let outcome = RunOutcome::Failed {
            finished_at: self.clock.now(),
            reason: error.as_failure_reason(),
        };
        // If recording the failure also fails, the original failure is the one
        // worth reporting — the store being unreachable is why we are here.
        let _ = self.runs.finish(run, outcome).await;
        error
    }
}

impl<S, L, R, G, K, C> WorkoutExtractor for Extraction<S, L, R, G, K, C>
where
    S: WorkoutEventSource + Sync,
    L: LandingStore + Sync,
    R: ResumptionPointStore + Sync,
    G: ExtractionRunLog + Sync,
    K: RunLock + Sync,
    C: Clock + Sync,
{
    async fn extract(&self) -> Result<RunSummary, ExtractionError> {
        // One stream throughout, read from the store that holds it. The lock,
        // the run log, the resumption point and every record landed name the
        // same one because there is only one to name.
        let stream = &self.stream;

        // Single-flight first, and before anything is written. A second run
        // must land nothing and move nothing, so it fails here or not at all.
        let _guard = self.lock.try_acquire(stream)?;

        let started_at = self.clock.now();
        let run = self.runs.begin(stream, started_at).await?;

        let since = match self.resumption.read(stream).await {
            Ok(since) => since,
            Err(error) => return Err(self.record_failure(run, error.into()).await),
        };

        let collected = match self.collect(run, since).await {
            Ok(collected) => collected,
            // The resumption point is untouched on this path, which is what
            // makes a retry reach the same end state as one clean run.
            Err(error) => return Err(self.record_failure(run, error).await),
        };

        let finished_at = self.clock.now();

        // The resumption point advances to the newest event this run *saw*,
        // and never to the clock. The feed serves newest first, so a workout
        // edited mid-run is promoted above batches already read and can be
        // missed by this run; because the point never passes an event we
        // observed, and that edit is by definition newer, the next run
        // collects it. Setting it from the clock would step over it for good.
        let (resumption_point, moved) = match collected.newest_event {
            None => (since, false),
            Some(newest) => {
                let advanced =
                    since.map_or_else(|| Watermark::from(newest), |at| at.advanced_to(newest));
                if Some(advanced) == since {
                    (since, false)
                } else {
                    match self.resumption.advance(stream, advanced, finished_at).await {
                        Ok(()) => (Some(advanced), true),
                        Err(error) => return Err(self.record_failure(run, error.into()).await),
                    }
                }
            }
        };

        let outcome = RunOutcome::Succeeded {
            finished_at,
            events_seen: collected.events_seen,
            records_landed: collected.records_landed,
        };
        if let Err(error) = self.runs.finish(run, outcome).await {
            return Err(error.into());
        }

        Ok(RunSummary {
            run_id: run,
            events_seen: collected.events_seen,
            records_landed: collected.records_landed,
            resumption_point,
            resumption_point_moved: moved,
        })
    }
}
