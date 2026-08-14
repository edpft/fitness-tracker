//! The landed corpus, as a fixture.
//!
//! 164 records — 163 `updated`, 1 `deleted`, November 2024 to August 2026 —
//! exported verbatim from the store this account extracted into. The payloads
//! are the bytes the source served; only the envelope around each was
//! re-encoded, and it holds nothing worth preserving.
//!
//! Committed rather than read from `local.db`, so the suite runs on a machine
//! that has never talked to Hevy. It is the fixture every figure in the model
//! of record was checked against, which is what makes an assertion over it an
//! assertion about the model rather than about a sample.
//!
//! Free functions, so they return `Result` and the test unwraps at the call
//! site: the `clippy.toml` exemptions cover `#[test]` bodies, not helpers
//! defined alongside them.

use std::{collections::HashMap, sync::Arc, sync::Mutex};

use application::{
    NormalisationError, StoreError,
    ports::{
        Clock, LandingRecordReader, NormalisationRunLog, NormalisedWorkoutStore, RefusalStore,
        WorkoutNormaliser as _,
    },
};
use domain::{
    gym::{
        GymWorkout, NormalisationOutcome, NormalisationRun, NormalisationRunId, OperatorZone,
        Refusal, RefusalCount, WorkoutCount,
    },
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
};

/// Everything that can go wrong building the fixture. Not a domain error — it
/// means the test setup is broken, which is a different thing from the code
/// under test being broken, and conflating them wastes an afternoon.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("the corpus fixture could not be read: {0}")]
    Unreadable(#[from] std::io::Error),
    #[error("the corpus fixture holds a line we cannot parse: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("the corpus fixture holds an invalid value: {0}")]
    Invalid(String),
}

fn invalid(detail: impl std::fmt::Display) -> FixtureError {
    FixtureError::Invalid(detail.to_string())
}

#[derive(serde::Deserialize)]
struct Row {
    id: i64,
    endpoint: String,
    fetched_at: String,
    source_record_id: String,
    event_kind: String,
    event_time: Option<String>,
    payload: String,
}

/// The stream every record in the fixture belongs to.
///
/// # Errors
///
/// [`FixtureError`] if the name is not a stream, which would be a typo here.
pub fn stream() -> Result<LandingStream, FixtureError> {
    LandingStream::try_from("hevy.workouts").map_err(invalid)
}

/// The zone the corpus was trained in.
///
/// The account's starts cluster at 18:00 UTC through British Summer Time and
/// 19:00–20:00 through Greenwich Mean Time, which is one zone across the whole
/// history.
///
/// # Errors
///
/// [`FixtureError`] if the identifier is not one the database knows.
pub fn zone() -> Result<OperatorZone, FixtureError> {
    OperatorZone::try_from("Europe/London").map_err(invalid)
}

/// The 164 landed records, oldest first.
///
/// # Errors
///
/// [`FixtureError`] if the fixture is missing or holds something unreadable.
pub fn records() -> Result<Vec<LandedRecord>, FixtureError> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/hevy-workouts.jsonl"
    );
    let text = std::fs::read_to_string(path)?;
    let stream = stream()?;

    let mut records = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let row: Row = serde_json::from_str(line)?;

        let occurred_at = row
            .event_time
            .as_deref()
            .map(EventTime::try_from)
            .transpose()
            .map_err(invalid)?;

        let provenance = EventProvenance::new(
            Endpoint::try_from(row.endpoint.as_str()).map_err(invalid)?,
            EventKind::try_from(row.event_kind.as_str()).map_err(invalid)?,
            occurred_at,
        );

        let record = LandingRecord::land(
            stream.clone(),
            FetchedAt::try_from(row.fetched_at.as_str()).map_err(invalid)?,
            SourceRecordId::try_from(row.source_record_id.as_str()).map_err(invalid)?,
            provenance.into(),
            RawPayload::try_from(row.payload.into_bytes()).map_err(invalid)?,
        );

        records.push(LandedRecord::new(
            LandingRecordId::try_from(row.id).map_err(invalid)?,
            record,
        ));
    }

    Ok(records)
}

/// Every `exercise_template_id` the corpus holds, distinct.
///
/// Read from the payloads rather than from what translated: an entry whose sets
/// all refused still had to resolve through the mapping, so coverage is a
/// question about the landed records and not about the output.
///
/// # Errors
///
/// [`FixtureError`] if the fixture is missing or holds something unreadable.
pub fn landed_template_ids() -> Result<Vec<String>, FixtureError> {
    #[derive(serde::Deserialize)]
    struct Envelope {
        workout: Option<Workout>,
    }
    #[derive(serde::Deserialize)]
    struct Workout {
        exercises: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        exercise_template_id: String,
    }

    let mut ids = Vec::new();
    for record in records()? {
        let envelope: Envelope = match serde_json::from_slice(record.payload().as_bytes()) {
            Ok(envelope) => envelope,
            // The `deleted` record has no workout and no entries to cover.
            Err(_) => continue,
        };
        let Some(workout) = envelope.workout else {
            continue;
        };
        for entry in workout.exercises {
            if !ids.contains(&entry.exercise_template_id) {
                ids.push(entry.exercise_template_id);
            }
        }
    }
    Ok(ids)
}

/// Raw, in memory.
///
/// A fake rather than the SQLite adapter, because these are the *use case's*
/// tests: what they exercise is the derivation's behaviour through its ports,
/// and a database in the middle would only add a way for them to fail that has
/// nothing to do with what they assert. The store adapter has its own tests.
pub struct InMemoryRaw {
    stream: LandingStream,
    records: Vec<LandedRecord>,
}

impl InMemoryRaw {
    pub const fn new(stream: LandingStream, records: Vec<LandedRecord>) -> Self {
        Self { stream, records }
    }

    /// The same records, served in the opposite order.
    ///
    /// What FR-028 is tested with: a derivation whose result depends on read
    /// order is one that resolves retraction by position rather than by
    /// absorption.
    #[must_use]
    pub fn reversed(mut self) -> Self {
        self.records.reverse();
        self
    }
}

impl LandingRecordReader for InMemoryRaw {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn records(&self) -> Result<Vec<LandedRecord>, StoreError> {
        Ok(self.records.clone())
    }
}

/// The normalised layer, in memory. Replaced wholesale, exactly as the real one
/// is.
#[derive(Clone)]
pub struct InMemoryWorkouts {
    stream: LandingStream,
    written: Arc<Mutex<Vec<GymWorkout>>>,
}

impl InMemoryWorkouts {
    pub fn new(stream: LandingStream) -> Self {
        Self {
            stream,
            written: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// # Errors
    ///
    /// [`FixtureError`] if another test thread poisoned the lock.
    pub fn workouts(&self) -> Result<Vec<GymWorkout>, FixtureError> {
        self.written
            .lock()
            .map(|held| held.clone())
            .map_err(|_| FixtureError::Invalid("the fixture lock was poisoned".to_owned()))
    }
}

impl NormalisedWorkoutStore for InMemoryWorkouts {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        _run: NormalisationRunId,
        workouts: Vec<GymWorkout>,
    ) -> Result<WorkoutCount, StoreError> {
        let count = workouts.len();
        {
            let mut held = self.written.lock().map_err(|_| StoreError::Corrupt {
                detail: "the fixture lock was poisoned".to_owned(),
            })?;
            *held = workouts;
        }
        Ok(WorkoutCount::from(count))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let held = self.written.lock().map_err(|_| StoreError::Corrupt {
            detail: "the fixture lock was poisoned".to_owned(),
        })?;
        Ok(WorkoutCount::from(held.len()))
    }
}

/// Refusals, in memory.
#[derive(Clone)]
pub struct InMemoryRefusals {
    stream: LandingStream,
    written: Arc<Mutex<Vec<Refusal>>>,
}

impl InMemoryRefusals {
    pub fn new(stream: LandingStream) -> Self {
        Self {
            stream,
            written: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// # Errors
    ///
    /// [`FixtureError`] if another test thread poisoned the lock.
    pub fn refusals(&self) -> Result<Vec<Refusal>, FixtureError> {
        self.written
            .lock()
            .map(|held| held.clone())
            .map_err(|_| FixtureError::Invalid("the fixture lock was poisoned".to_owned()))
    }
}

impl RefusalStore for InMemoryRefusals {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        _run: NormalisationRunId,
        refusals: Vec<Refusal>,
    ) -> Result<RefusalCount, StoreError> {
        let count = refusals.len();
        {
            let mut held = self.written.lock().map_err(|_| StoreError::Corrupt {
                detail: "the fixture lock was poisoned".to_owned(),
            })?;
            *held = refusals;
        }
        Ok(RefusalCount::from(count))
    }

    async fn all(&self) -> Result<Vec<Refusal>, StoreError> {
        let held = self.written.lock().map_err(|_| StoreError::Corrupt {
            detail: "the fixture lock was poisoned".to_owned(),
        })?;
        Ok(held.clone())
    }
}

/// A run log that remembers, so a report can name when the derivation ran.
#[derive(Clone, Default)]
pub struct InMemoryRunLog {
    runs: Arc<Mutex<HashMap<u64, NormalisationRun>>>,
}

impl NormalisationRunLog for InMemoryRunLog {
    async fn begin(
        &self,
        stream: &LandingStream,
        at: FetchedAt,
    ) -> Result<NormalisationRunId, StoreError> {
        let id = {
            let mut runs = self.runs.lock().map_err(|_| StoreError::Corrupt {
                detail: "the fixture lock was poisoned".to_owned(),
            })?;
            let id = u64::try_from(runs.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1);
            runs.insert(
                id,
                NormalisationRun::new(
                    NormalisationRunId::from(id),
                    stream.clone(),
                    at,
                    NormalisationOutcome::InFlight,
                ),
            );
            id
        };
        Ok(NormalisationRunId::from(id))
    }

    async fn finish(
        &self,
        run: NormalisationRunId,
        outcome: NormalisationOutcome,
    ) -> Result<(), StoreError> {
        {
            let mut runs = self.runs.lock().map_err(|_| StoreError::Corrupt {
                detail: "the fixture lock was poisoned".to_owned(),
            })?;
            if let Some(held) = runs.get(&run.as_u64()) {
                let replacement = NormalisationRun::new(
                    held.id(),
                    held.stream().clone(),
                    held.started_at(),
                    outcome,
                );
                runs.insert(run.as_u64(), replacement);
            }
        }
        Ok(())
    }

    async fn latest_success(
        &self,
        _stream: &LandingStream,
    ) -> Result<Option<NormalisationRun>, StoreError> {
        let runs = self.runs.lock().map_err(|_| StoreError::Corrupt {
            detail: "the fixture lock was poisoned".to_owned(),
        })?;
        Ok(runs
            .values()
            .filter(|run| run.outcome().is_success())
            .max_by_key(|run| run.id())
            .cloned())
    }
}

/// A clock that does not move.
///
/// A derivation reads no time except to stamp its own run, so a fixed one is
/// not a simplification — it is the whole of what this port is for here.
#[derive(Clone, Copy)]
pub struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::EPOCH
    }
}

/// Everything wired together, ready to derive.
///
/// # Errors
///
/// [`FixtureError`] if the corpus cannot be loaded.
pub fn derivation() -> Result<Derivation, FixtureError> {
    Ok(Derivation {
        records: records()?,
        stream: stream()?,
        zone: zone()?,
    })
}

/// The fixture's inputs, before a derivation is built from them.
pub struct Derivation {
    pub records: Vec<LandedRecord>,
    pub stream: LandingStream,
    pub zone: OperatorZone,
}

/// What one derivation produced, for a test to assert over.
pub struct Produced {
    pub workouts: Vec<GymWorkout>,
    pub refusals: Vec<Refusal>,
    pub summary: application::NormalisationSummary,
}

impl Derivation {
    /// Derive, and hand back everything that came of it.
    ///
    /// # Errors
    ///
    /// [`NormalisationError`] if the derivation failed — which for the corpus
    /// means the mapping has a gap.
    pub async fn run(&self, reversed: bool) -> Result<Produced, NormalisationError> {
        self.run_in(self.zone.clone(), reversed).await
    }

    /// Derive under a zone other than the corpus's declared one.
    ///
    /// What scenario 7 needs: the same raw under two configurations must give
    /// the same instants and different wall clocks, which is the difference
    /// between storing a zone and storing an offset.
    ///
    /// # Errors
    ///
    /// [`NormalisationError`] if the derivation failed.
    pub async fn run_in(
        &self,
        zone: OperatorZone,
        reversed: bool,
    ) -> Result<Produced, NormalisationError> {
        let raw = {
            let raw = InMemoryRaw::new(self.stream.clone(), self.records.clone());
            if reversed { raw.reversed() } else { raw }
        };
        let workouts = InMemoryWorkouts::new(self.stream.clone());
        let refusals = InMemoryRefusals::new(self.stream.clone());

        let normalisation = application::normalise::Normalisation::new(
            application::normalise::NormalisationPorts {
                raw,
                translator: infrastructure::hevy::HevyWorkoutTranslator,
                workouts: workouts.clone(),
                refusals: refusals.clone(),
                runs: InMemoryRunLog::default(),
                clock: FixedClock,
            },
            zone,
        );

        let summary = normalisation.normalise().await?;

        let store_broke = |_| {
            NormalisationError::Store(StoreError::Corrupt {
                detail: "the fixture lock was poisoned".to_owned(),
            })
        };
        Ok(Produced {
            workouts: workouts.workouts().map_err(store_broke)?,
            refusals: refusals.refusals().map_err(store_broke)?,
            summary,
        })
    }
}

/// Run an async body on a current-thread runtime.
///
/// Built by hand rather than with `#[tokio::test]`, which generates
/// `#[allow(clippy::unwrap_used)]` — a compile error under a `forbid` lint.
///
/// # Errors
///
/// [`FixtureError`] if the runtime cannot be built.
pub fn block_on<T>(body: impl Future<Output = T>) -> Result<T, FixtureError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    Ok(runtime.block_on(body))
}

/// One derived workout, by the source's identifier for it.
///
/// Pinning a scenario to a real record rather than a synthesised one: the
/// question these ask is what the corpus does, and a fixture built to answer it
/// would be answering itself.
pub fn workout_starting<'a>(
    produced: &'a Produced,
    source_record_id: &str,
) -> Option<&'a GymWorkout> {
    produced
        .workouts
        .iter()
        .find(|workout| workout.source_record_id().as_str() == source_record_id)
}

/// The corpus plus an `updated` record for `source_record_id`.
///
/// Built from a real workout's body so the record is genuine in every respect
/// except which workout it claims to be. What it exists for is to give the
/// landed tombstone something to withdraw — the corpus offers no such pair, and
/// a retraction that removes nothing exercises only half the rule.
///
/// # Errors
///
/// [`FixtureError`] if the corpus is empty or the identifier is not a valid one.
pub fn with_synthetic_update_for(
    fixture: &Derivation,
    source_record_id: &str,
) -> Result<Derivation, FixtureError> {
    let donor = fixture
        .records
        .iter()
        .find(|record| {
            matches!(
                record.provenance(),
                domain::landing::Provenance::Event(event)
                    if *event.kind() == EventKind::Updated
            )
        })
        .ok_or_else(|| FixtureError::Invalid("the corpus holds no update".to_owned()))?;

    let id = next_landing_id(fixture);
    let record = LandingRecord::land(
        fixture.stream.clone(),
        donor.record().fetched_at(),
        SourceRecordId::try_from(source_record_id).map_err(invalid)?,
        donor.provenance().clone(),
        donor.payload().clone(),
    );

    let mut records = fixture.records.clone();
    records.push(LandedRecord::new(id, record));
    Ok(Derivation {
        records,
        stream: fixture.stream.clone(),
        zone: fixture.zone.clone(),
    })
}

/// The corpus plus a second landing record for the first workout's identifier.
///
/// § 10's supersession, which the corpus cannot exercise: 164 records carry 164
/// distinct identifiers and not one re-serve.
///
/// # Errors
///
/// [`FixtureError`] if the corpus is empty.
pub fn with_reserved_first_workout(fixture: &Derivation) -> Result<Derivation, FixtureError> {
    let first = fixture
        .records
        .first()
        .ok_or_else(|| FixtureError::Invalid("the corpus is empty".to_owned()))?;

    let id = next_landing_id(fixture);
    let mut records = fixture.records.clone();
    records.push(LandedRecord::new(id, first.record().clone()));
    Ok(Derivation {
        records,
        stream: fixture.stream.clone(),
        zone: fixture.zone.clone(),
    })
}

/// The corpus plus a record naming a template the mapping does not cover.
///
/// # Errors
///
/// [`FixtureError`] if the corpus is empty or the synthetic payload is invalid.
pub fn with_unmapped_template(
    fixture: &Derivation,
    template_id: &str,
) -> Result<Derivation, FixtureError> {
    let donor = fixture
        .records
        .first()
        .ok_or_else(|| FixtureError::Invalid("the corpus is empty".to_owned()))?;

    let payload = format!(
        r#"{{"type":"updated","workout":{{"id":"synthetic","start_time":"2026-01-01T18:00:00+00:00","exercises":[{{"index":0,"title":"Something New","exercise_template_id":"{template_id}","superset_id":null,"sets":[{{"index":0,"type":"normal","weight_kg":60,"reps":5,"distance_meters":null,"duration_seconds":null,"rpe":null}}]}}]}}}}"#
    );

    let id = next_landing_id(fixture);
    let record = LandingRecord::land(
        fixture.stream.clone(),
        donor.record().fetched_at(),
        SourceRecordId::try_from("synthetic-unmapped").map_err(invalid)?,
        donor.provenance().clone(),
        RawPayload::try_from(payload.into_bytes()).map_err(invalid)?,
    );

    let mut records = fixture.records.clone();
    records.push(LandedRecord::new(id, record));
    Ok(Derivation {
        records,
        stream: fixture.stream.clone(),
        zone: fixture.zone.clone(),
    })
}

/// One past the highest landing record id the fixture holds.
///
/// Raw is append-only, so a synthetic record joins at the end exactly as a real
/// one would.
fn next_landing_id(fixture: &Derivation) -> LandingRecordId {
    let highest = fixture
        .records
        .iter()
        .map(|record| record.id().as_i64())
        .max()
        .unwrap_or(0);
    LandingRecordId::try_from(highest.saturating_add(1)).unwrap_or(LandingRecordId::FIRST)
}

/// Derive over the corpus, flattening the runtime and the derivation into one
/// answer.
///
/// Nothing here panics. The suites assert with the `derived!` macro instead, so
/// the panic lands inside a `#[test]` body where `clippy.toml` allows it — the
/// exemptions do not reach a free function like this one.
///
/// # Errors
///
/// [`FixtureError`] if the fixture will not load, and
/// [`NormalisationError`] if the derivation fails.
pub fn derive(reversed: bool) -> Result<Result<Produced, NormalisationError>, FixtureError> {
    let fixture = derivation()?;
    block_on(async move { fixture.run(reversed).await })
}
