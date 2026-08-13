//! Reporting where a stream stands, and discarding where it resumes from.

use domain::landing::LandingStream;

use crate::{
    error::StatusError,
    ports::{
        ExtractionRunLog, ExtractionStatusReporter, LandingStore, ResumptionPointResetter,
        ResumptionPointStore, StreamStatus,
    },
};

/// Reads the state extraction leaves behind.
///
/// Deliberately separate from the extraction use case and holding no lock:
/// asking where things stand must work *while* a run is in flight, and must
/// keep working when the source is unreachable. A staleness report that is
/// itself unavailable whenever things go wrong reports nothing worth having.
pub struct ExtractionStatus<L, R, G> {
    stream: LandingStream,
    landing: L,
    resumption: R,
    runs: G,
}

impl<L, R, G> ExtractionStatus<L, R, G>
where
    L: LandingStore,
{
    /// No stream argument, for the same reason as [`crate::extract::Extraction::new`]:
    /// the landing store is bound to one table and is asked which.
    pub fn new(landing: L, resumption: R, runs: G) -> Self {
        Self {
            stream: landing.stream().clone(),
            landing,
            resumption,
            runs,
        }
    }
}

impl<L, R, G> ExtractionStatusReporter for ExtractionStatus<L, R, G>
where
    L: LandingStore + Sync,
    R: ResumptionPointStore + Sync,
    G: ExtractionRunLog + Sync,
{
    async fn status(&self) -> Result<StreamStatus, StatusError> {
        let stream = &self.stream;

        // Never having run is a fact to report, not an error to raise. Each of
        // these is legitimately absent on a fresh store.
        let last_success = self.runs.latest_success(stream).await?;
        let resumption_point = self.resumption.read(stream).await?;
        let records_held = self.landing.count().await?;

        Ok(StreamStatus {
            stream: stream.clone(),
            last_success,
            records_held,
            resumption_point,
        })
    }
}

impl<L, R, G> ResumptionPointResetter for ExtractionStatus<L, R, G>
where
    L: LandingStore + Sync,
    R: ResumptionPointStore + Sync,
    G: ExtractionRunLog + Sync,
{
    async fn reset(&self) -> Result<(), StatusError> {
        // The whole of a reset. Nothing in raw is touched: the next run
        // re-serves every payload from the epoch, and identical payloads land
        // nothing. Losing the position costs a re-fetch, never a fact.
        self.resumption.clear(&self.stream).await?;
        Ok(())
    }
}
