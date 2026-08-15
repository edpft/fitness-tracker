//! In-memory doubles for every driven port.
//!
//! These live in `application`'s tests rather than in `infrastructure`,
//! because a use-case test that needs a database has a dependency pointing the
//! wrong way. Everything below is a `Mutex` over a `Vec`; nothing here touches
//! the network, the filesystem or the clock.
//!
//! No lock is held across an `await`: every method does its work
//! synchronously, which is what keeps the resulting futures `Send`.
//!
//! The fixture builders return `Result` rather than unwrapping. Clippy's test
//! exemption for `expect` reaches a `#[test]` function and the async block
//! inside it, but not a free function alongside them — so the unwrapping
//! happens at the call site, where it is allowed and where a bad fixture
//! should fail loudly anyway.

#![allow(dead_code)]

use std::{
    error::Error,
    sync::{Arc, Mutex, PoisonError},
};

pub type Fallible<T> = Result<T, Box<dyn Error>>;

/// What a fake source was asked for: a resumption point and a page number.
type Request = (Option<Watermark>, u32);

/// A feed, as pages of events served newest-first.
type Pages = Vec<Vec<SourceEvent>>;

/// An edit staged to arrive mid-run: after this page has been served, the feed
/// becomes these pages.
type StagedEdit = Option<(u32, Pages)>;

use application::{
    Clock, EventBatch, ExtractionRunLog, LandingStore, ResumptionPointStore, RunLock, SourceError,
    SourceEvent, StoreError, WorkoutEventSource,
};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, ExtractionRun, FetchedAt, LandingRecord,
    LandingStream, PayloadDigest, Provenance, RawPayload, RecordCount, RunId, RunOutcome,
    SourceRecordId, Watermark,
};

pub fn hevy_workouts() -> Fallible<LandingStream> {
    Ok(LandingStream::try_from("hevy.workouts")?)
}

pub fn events_endpoint() -> Fallible<Endpoint> {
    Ok(Endpoint::try_from("/v1/workouts/events")?)
}

pub fn at(value: &str) -> Fallible<FetchedAt> {
    Ok(FetchedAt::try_from(value)?)
}

pub fn event_at(value: &str) -> Fallible<EventTime> {
    Ok(EventTime::try_from(value)?)
}

/// How the events feed describes what it served. Every fixture below builds
/// one, because provenance is the adapter's to supply.
fn served(kind: EventKind, when: Option<EventTime>) -> Fallible<Provenance> {
    Ok(EventProvenance::new(events_endpoint()?, kind, when).into())
}

/// An update event carrying a body, in the shape the source serves.
pub fn updated(id: &str, body: &str, when: &str) -> Fallible<SourceEvent> {
    Ok(SourceEvent {
        source_record_id: SourceRecordId::try_from(id)?,
        provenance: served(EventKind::Updated, Some(event_at(when)?))?,
        payload: RawPayload::try_from(body.as_bytes())?,
    })
}

/// A deletion, which names its workout at the top level and carries no body.
pub fn deleted(id: &str, when: &str) -> Fallible<SourceEvent> {
    Ok(SourceEvent {
        source_record_id: SourceRecordId::try_from(id)?,
        provenance: served(EventKind::Deleted, Some(event_at(when)?))?,
        payload: RawPayload::try_from(
            format!(r#"{{"type":"deleted","id":"{id}","deleted_at":"{when}"}}"#).into_bytes(),
        )?,
    })
}

/// An event the source served without a timestamp.
pub fn untimed(id: &str, body: &str) -> Fallible<SourceEvent> {
    Ok(SourceEvent {
        source_record_id: SourceRecordId::try_from(id)?,
        provenance: served(EventKind::Updated, None)?,
        payload: RawPayload::try_from(body.as_bytes())?,
    })
}

fn guard<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

// --- The source -------------------------------------------------------------

/// A stub feed: pages served newest-first, exactly as Hevy does.
#[derive(Clone)]
pub struct FakeSource {
    pages: Arc<Mutex<Pages>>,
    /// One-based page number that should fail instead of being served.
    fail_on_page: Arc<Mutex<Option<u32>>>,
    requests: Arc<Mutex<Vec<Request>>>,
    unreachable: Arc<Mutex<bool>>,
    /// What the feed becomes after a given page has been served, so a test can
    /// stage an edit that lands *during* a run.
    swap_after: Arc<Mutex<StagedEdit>>,
}

impl FakeSource {
    pub fn serving(pages: Pages) -> Self {
        Self {
            pages: Arc::new(Mutex::new(pages)),
            fail_on_page: Arc::new(Mutex::new(None)),
            requests: Arc::new(Mutex::new(Vec::new())),
            unreachable: Arc::new(Mutex::new(false)),
            swap_after: Arc::new(Mutex::new(None)),
        }
    }

    /// An account holding nothing. The source answers with one empty page,
    /// which is exactly what the live API does.
    pub fn empty() -> Self {
        Self::serving(vec![Vec::new()])
    }

    pub fn fail_on_page(&self, page: u32) {
        *guard(&self.fail_on_page) = Some(page);
    }

    pub fn heal(&self) {
        *guard(&self.fail_on_page) = None;
        *guard(&self.unreachable) = false;
    }

    pub fn go_unreachable(&self) {
        *guard(&self.unreachable) = true;
    }

    /// Replace what the source serves, as an edit in Hevy would.
    pub fn now_serving(&self, pages: Pages) {
        *guard(&self.pages) = pages;
    }

    /// Stage an edit that arrives mid-run: once `page` has been served, the
    /// feed becomes `pages`.
    ///
    /// This is how a workout edited during a run gets promoted above a page
    /// the run has already read — the race the resumption point rule exists
    /// to survive.
    pub fn swap_after_page(&self, page: u32, pages: Pages) {
        *guard(&self.swap_after) = Some((page, pages));
    }

    pub fn requests(&self) -> Vec<Request> {
        guard(&self.requests).clone()
    }

    pub fn request_count(&self) -> usize {
        guard(&self.requests).len()
    }
}

impl WorkoutEventSource for FakeSource {
    /// A page number, which is this stub's way of saying "not finished". The
    /// application never looks inside it — that is the point of the type.
    type Resume = u32;

    async fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<u32>,
    ) -> Result<EventBatch<u32>, SourceError> {
        // No resume token means the opening request of a walk.
        let page = resume.unwrap_or(1);
        guard(&self.requests).push((since, page));

        if *guard(&self.unreachable) {
            return Err(SourceError::Unavailable {
                detail: "stub is unreachable".to_owned(),
            });
        }
        if *guard(&self.fail_on_page) == Some(page) {
            return Err(SourceError::Unavailable {
                detail: format!("stub fails on page {page}"),
            });
        }

        let pages = guard(&self.pages);
        // The source filters by `since` inclusively, and serves newest first.
        let filtered: Pages = pages
            .iter()
            .map(|events| {
                events
                    .iter()
                    .filter(|event| match (since, event.provenance.occurred_at()) {
                        (Some(mark), Some(time)) => time.as_timestamp() >= mark.as_timestamp(),
                        _ => true,
                    })
                    .cloned()
                    .collect()
            })
            .filter(|events: &Vec<SourceEvent>| !events.is_empty())
            .collect();

        // An empty result is still one page, matching the live API's
        // `{"page":1,"page_count":1,"workouts":[]}`.
        let page_count = u32::try_from(filtered.len()).unwrap_or(u32::MAX).max(1);
        let events = filtered
            .get((page as usize).saturating_sub(1))
            .cloned()
            .unwrap_or_default();

        drop(pages);

        // Apply a staged mid-run edit only after this page has been decided,
        // so the run sees the feed as it was when it asked.
        let staged = guard(&self.swap_after).take();
        if let Some((after, replacement)) = staged {
            if page == after {
                *guard(&self.pages) = replacement;
            } else {
                *guard(&self.swap_after) = Some((after, replacement));
            }
        }

        // The last page says so by resuming nowhere. Asking beyond it is an
        // error at the live source, so a walk has to stop exactly here.
        let next = page.saturating_add(1);
        Ok(EventBatch {
            events,
            resume: (next <= page_count).then_some(next),
        })
    }
}

// --- The store --------------------------------------------------------------

/// Bound to one stream, like every real landing store: it is the table, and
/// the table is one stream's. There is no `Default`, because a store that does
/// not know which stream it holds is the thing the port exists to rule out.
#[derive(Clone)]
pub struct InMemoryLanding {
    stream: LandingStream,
    records: Arc<Mutex<Vec<(RunId, LandingRecord)>>>,
    failing: Arc<Mutex<bool>>,
}

impl InMemoryLanding {
    pub fn holding(stream: LandingStream) -> Self {
        Self {
            stream,
            records: Arc::new(Mutex::new(Vec::new())),
            failing: Arc::new(Mutex::new(false)),
        }
    }

    pub fn records(&self) -> Vec<LandingRecord> {
        guard(&self.records)
            .iter()
            .map(|(_, record)| record.clone())
            .collect()
    }

    pub fn for_id(&self, id: &str) -> Vec<LandingRecord> {
        self.records()
            .into_iter()
            .filter(|record| record.source_record_id().as_str() == id)
            .collect()
    }

    pub fn distinct_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .records()
            .iter()
            .map(|record| record.source_record_id().as_str().to_owned())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    pub fn fail(&self) {
        *guard(&self.failing) = true;
    }
}

impl LandingStore for InMemoryLanding {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn latest_digest(
        &self,
        id: &SourceRecordId,
    ) -> Result<Option<PayloadDigest>, StoreError> {
        if *guard(&self.failing) {
            return Err(StoreError::Unavailable {
                detail: "stub store".to_owned(),
            });
        }
        Ok(guard(&self.records)
            .iter()
            .rev()
            .find(|(_, record)| record.source_record_id() == id)
            .map(|(_, record)| record.digest()))
    }

    async fn append(
        &self,
        run: RunId,
        records: Vec<LandingRecord>,
    ) -> Result<RecordCount, StoreError> {
        if *guard(&self.failing) {
            return Err(StoreError::Unavailable {
                detail: "stub store".to_owned(),
            });
        }
        let landed = records.len();
        guard(&self.records).extend(records.into_iter().map(|record| (run, record)));
        Ok(RecordCount::from(landed))
    }

    async fn count(&self) -> Result<RecordCount, StoreError> {
        Ok(RecordCount::from(guard(&self.records).len()))
    }
}

#[derive(Clone, Default)]
pub struct InMemoryResumption {
    point: Arc<Mutex<Option<Watermark>>>,
}

impl InMemoryResumption {
    pub fn holding(mark: Watermark) -> Self {
        Self {
            point: Arc::new(Mutex::new(Some(mark))),
        }
    }

    pub fn get(&self) -> Option<Watermark> {
        *guard(&self.point)
    }
}

impl ResumptionPointStore for InMemoryResumption {
    async fn read(&self, _stream: &LandingStream) -> Result<Option<Watermark>, StoreError> {
        Ok(*guard(&self.point))
    }

    async fn advance(
        &self,
        _stream: &LandingStream,
        to: Watermark,
        _at: FetchedAt,
    ) -> Result<(), StoreError> {
        *guard(&self.point) = Some(to);
        Ok(())
    }

    async fn clear(&self, _stream: &LandingStream) -> Result<(), StoreError> {
        *guard(&self.point) = None;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub struct InMemoryRuns {
    runs: Arc<Mutex<Vec<(RunId, RunOutcome)>>>,
    started: Arc<Mutex<Vec<FetchedAt>>>,
}

impl InMemoryRuns {
    pub fn outcomes(&self) -> Vec<RunOutcome> {
        guard(&self.runs)
            .iter()
            .map(|(_, outcome)| outcome.clone())
            .collect()
    }

    pub fn last_outcome(&self) -> Option<RunOutcome> {
        guard(&self.runs).last().map(|(_, outcome)| outcome.clone())
    }
}

impl ExtractionRunLog for InMemoryRuns {
    async fn begin(&self, _stream: &LandingStream, at: FetchedAt) -> Result<RunId, StoreError> {
        // Scoped so the `runs` lock is released before `started` is taken:
        // holding two of this fake's locks at once is a deadlock waiting for a
        // test that touches it from more than one task.
        let id = {
            let mut runs = guard(&self.runs);
            let id = RunId::from(
                u64::try_from(runs.len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            );
            runs.push((id, RunOutcome::InFlight));
            id
        };
        guard(&self.started).push(at);
        Ok(id)
    }

    async fn finish(&self, run: RunId, outcome: RunOutcome) -> Result<(), StoreError> {
        if let Some(entry) = guard(&self.runs).iter_mut().find(|(id, _)| *id == run) {
            entry.1 = outcome;
        }
        Ok(())
    }

    async fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> Result<Option<ExtractionRun>, StoreError> {
        let runs = guard(&self.runs);
        let started = guard(&self.started);
        Ok(runs
            .iter()
            .rev()
            .find(|(_, outcome)| outcome.is_success())
            .map(|(id, outcome)| {
                let index = usize::try_from(id.as_u64()).unwrap_or(1).saturating_sub(1);
                ExtractionRun::new(
                    *id,
                    stream.clone(),
                    started.get(index).copied().unwrap_or(FetchedAt::EPOCH),
                    outcome.clone(),
                )
            }))
    }
}

// --- The lock ---------------------------------------------------------------

/// A lock that can be told to be already held, so FR-010 is testable without
/// spawning a second process.
#[derive(Clone)]
pub struct FakeLock {
    held: Arc<Mutex<bool>>,
}

impl FakeLock {
    pub fn free() -> Self {
        Self {
            held: Arc::new(Mutex::new(false)),
        }
    }

    pub fn already_held() -> Self {
        Self {
            held: Arc::new(Mutex::new(true)),
        }
    }
}

pub struct FakeGuard;

impl RunLock for FakeLock {
    type Guard = FakeGuard;

    fn try_acquire(
        &self,
        _stream: &LandingStream,
    ) -> Result<Self::Guard, application::RunLockError> {
        if *guard(&self.held) {
            return Err(application::RunLockError::Held);
        }
        Ok(FakeGuard)
    }
}

// --- The clock --------------------------------------------------------------

/// A clock that does not move.
///
/// Extraction must never take its resumption point from the clock, so a
/// frozen one is not a simplification — it is the assertion.
pub struct FixedClock {
    now: FetchedAt,
}

impl FixedClock {
    pub fn at(value: &str) -> Fallible<Self> {
        Ok(Self { now: at(value)? })
    }
}

impl Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        self.now
    }
}
