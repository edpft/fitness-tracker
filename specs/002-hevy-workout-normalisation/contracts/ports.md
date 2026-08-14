# Contract: application ports

Phase 1. Declared in `crates/application/src/ports.rs`, in the application's own
vocabulary. No `serde_json`, `sqlx` or `jiff` type appears in a signature; every
asynchronous method spells out `impl Future<…> + Send`, for the reason the
existing file gives.

## Driven

### `LandingRecordReader`

Raw, read-only, for **one** stream — bound at construction and asked which,
exactly as `LandingStore` is. Extraction's store and this one are separate
traits rather than one widened trait, because a reader that cannot append is
what makes "derivation never writes to raw" a fact about the type rather than a
promise about the code.

```rust
fn stream(&self) -> &LandingStream;

/// Every record for this stream, oldest first, in the order the source served
/// them. Ordering is defined so a derivation is reproducible, not because the
/// use case depends on it — retraction is absorbing and order-independent
/// (FR-028), and this is what lets that be tested by reversing the stream.
fn records(&self) -> impl Future<Output = Result<Vec<LandingRecord>, StoreError>> + Send;
```

Returning a `Vec` rather than a stream: the corpus is 164 payloads and the use
case needs two passes over it anyway — one to collect retracted ids, one to
emit. A stream would buy laziness the feature has no use for and cost the
second pass.

### `WorkoutTranslator`

The one port that knows a source's format. Synchronous and total: it makes no
request, touches no clock, and consults no overlay (FR-002, FR-003) — which is
what "deterministic translation" means, and is visible in a signature that has
nothing to be non-deterministic with.

```rust
/// # Errors
///
/// [`TranslationError::UnmappedExercise`] and nothing else. A defect in our
/// vocabulary, not in the data, so it stops the run (FR-017) where every data
/// problem becomes a `Refusal` inside a successful `Translation` (FR-024).
fn translate(
    &self,
    record: &LandingRecord,
    zone: &OperatorZone,
) -> Result<Translation, TranslationError>;
```

The zone is a parameter rather than adapter state, so the same translator
answers for any declared configuration and a test can pin both sides of a
switchover without building two of them.

### `NormalisedWorkoutStore`

```rust
fn stream(&self) -> &LandingStream;

/// Replace this stream's normalised layer entirely. One transaction: a
/// derivation is never half-applied, because a half-applied derivation is not
/// a function of anything (§ II).
fn replace(
    &self,
    run: NormalisationRunId,
    workouts: Vec<GymWorkout>,
) -> impl Future<Output = Result<WorkoutCount, StoreError>> + Send;

fn count(&self) -> impl Future<Output = Result<WorkoutCount, StoreError>> + Send;
```

### `RefusalStore`

```rust
fn stream(&self) -> &LandingStream;

fn replace(
    &self,
    run: NormalisationRunId,
    refusals: Vec<Refusal>,
) -> impl Future<Output = Result<RefusalCount, StoreError>> + Send;

/// FR-023. Read back after a derivation, so what the domain will not accept is
/// visible rather than surfacing only in a log.
fn all(&self) -> impl Future<Output = Result<Vec<Refusal>, StoreError>> + Send;
```

### `NormalisationRunLog`

`begin` / `finish` / `latest_success`, mirroring `ExtractionRunLog` exactly.
Same reasoning, same shape: § 38 wants a broken derivation visible rather than
merely absent, and a derivation that found nothing must be distinguishable from
one that failed.

## Driving

Named for the thing that does the work. Neither takes a stream: both are built
from ports already bound to one.

```rust
/// Derives the normalised layer for a stream.
pub trait WorkoutNormaliser {
    fn normalise(&self)
        -> impl Future<Output = Result<NormalisationSummary, NormalisationError>> + Send;
}

/// Reports what the domain would not accept.
pub trait RefusalReporter {
    fn refusals(&self) -> impl Future<Output = Result<RefusalReport, NormalisationError>> + Send;
}
```

`NormalisationSummary` carries `run_id`, `records_read`, `workouts_written`,
`retractions_applied` and `refusals_recorded` — five numbers that must add up,
which is how SC-005 is asserted without reading a row: `records_read` equals
workouts plus retractions plus records refused outright.

FR-029 is satisfied by these two traits existing at all. `cli` implements
nothing; it constructs the use case and calls the trait, and a future `web`
handler does the same against the same signature.

## Errors

`NormalisationError`, in `application::error`, with the same shape as
`ExtractionError`:

| Variant | Raised by | Meaning |
| --- | --- | --- |
| `Store(StoreError)` | any store port | The database is unavailable or holds something unreadable |
| `UnmappedExercise { template_id }` | `WorkoutTranslator` | FR-017. The vocabulary has a gap; naming the id is the whole point |
| `MissingZone` | the composition root | FR-020. No operator zone declared, so no timestamp can be built |

No variant for bad data. That is deliberate and is the feature's central
distinction: a wrong record produces a `Refusal` and a successful run, while
only a defect in our own code produces an error.

## What no port does

- **None takes an overlay.** § 9 forbids consulting one, and the strongest form
  of that is a port that could not be handed one.
- **None takes a `LandingStream`** — each is bound to one and asked which
  (`HevyWorkoutLandingStore::STREAM`), so two streams that must agree are one
  stream. The rule 001 established, unchanged.
- **None reaches the network.** A derivation with every source down is a
  derivation (§ 36).
