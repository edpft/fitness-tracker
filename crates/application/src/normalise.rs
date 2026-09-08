//! Deriving the normalised layer from raw.
//!
//! Reads every account for one stream, translates each, and replaces the
//! stream's normalised layer with the result. It contacts nothing: a derivation
//! works with every source down, which is § 36 satisfied by construction rather
//! than by degrading gracefully.
//!
//! Two passes over the records, and the reason for the first is the whole of
//! how retraction works here. A withdrawal is **absorbing**, not latest-wins:
//! every retracted source record id is collected before any workout is emitted,
//! so the result does not depend on the order the records are read in.
//! Latest-wins would depend on it, and would also answer a question the corpus
//! cannot — whether a source that deletes a workout and later serves it again
//! has re-created it — which § 10 reserves for the canonical layer.

use domain::{
    landing::{LandingStream, RecordCount, SourceRecordId},
    normalised::{
        NormalisationFailure, NormalisationOutcome, NormalisationRunId, NormalisedEntity,
        OperatorZone, Refusal, RefusalCount, WorkoutCount,
    },
};

use crate::{
    error::NormalisationError,
    ports::{
        AccountReader, Clock, DerivationStatus, DerivationStatusReporter, NormalisationRunLog,
        NormalisationSummary, NormalisedEntityStore, RawExtent, RefusalReport, RefusalReporter,
        RefusalStore, SourceAccount, Translation, Translator, WorkoutNormaliser,
    },
};

/// The derivation use case.
///
/// Generic over every port it uses, so the composition root decides what it
/// talks to and a test can drive the whole of it with fakes and no I/O.
pub struct Normalisation<R, T, W, F, G, C> {
    stream: LandingStream,
    zone: OperatorZone,
    raw: R,
    translator: T,
    workouts: W,
    refusals: F,
    runs: G,
    clock: C,
}

/// The adapters a derivation needs.
///
/// A struct rather than six positional arguments: at the composition root the
/// names are what make the wiring readable, and two ports of the same shape
/// cannot be swapped by accident.
pub struct NormalisationPorts<R, T, W, F, G, C> {
    pub raw: R,
    pub translator: T,
    pub workouts: W,
    pub refusals: F,
    pub runs: G,
    pub clock: C,
}

impl<R, T, W, F, G, C> Normalisation<R, T, W, F, G, C>
where
    R: AccountReader,
{
    /// No stream argument: the reader is bound to one table and is asked which,
    /// so a derivation cannot be built holding a stream its ports disagree
    /// with. The rule extraction established, unchanged.
    ///
    /// The zone *is* an argument, because it is not a fact about any port — it
    /// is a declared interpretive parameter, and § II.3 says translation takes
    /// it from configuration.
    pub fn new(ports: NormalisationPorts<R, T, W, F, G, C>, zone: OperatorZone) -> Self {
        Self {
            stream: ports.raw.stream().clone(),
            zone,
            raw: ports.raw,
            translator: ports.translator,
            workouts: ports.workouts,
            refusals: ports.refusals,
            runs: ports.runs,
            clock: ports.clock,
        }
    }
}

/// What translating the whole corpus produced, before it is written.
///
/// `refused` counts accounts, and is *not* `refusals.len()`: a refusal is one
/// omission, and an account that translated perfectly well can carry several of
/// them. What it counts is the accounts that yielded no entity at all.
#[derive(Debug)]
struct Derived<E> {
    entities: Vec<E>,
    refusals: Vec<Refusal>,
    retracted: Vec<SourceRecordId>,
    refused: usize,
    superseded: usize,
    composed: usize,
}

impl<E> Default for Derived<E> {
    /// By hand, because deriving it would demand `E: Default` — and there is
    /// no such thing as a default gym workout.
    fn default() -> Self {
        Self {
            entities: Vec::new(),
            refusals: Vec::new(),
            retracted: Vec::new(),
            refused: 0,
            superseded: 0,
            composed: 0,
        }
    }
}

impl<R, T, W, F, G, C> Normalisation<R, T, W, F, G, C>
where
    R: AccountReader + Sync,
    T: Translator<Account = R::Account> + Sync,
    W: NormalisedEntityStore<Entity = T::Entity> + Sync,
    F: RefusalStore + Sync,
    G: NormalisationRunLog + Sync,
    C: Clock + Sync,
{
    /// Record why a derivation stopped, then hand the failure back.
    ///
    /// Its own call rather than inline, for extraction's reason: the difference
    /// between a derivation that broke and one that quietly found nothing is
    /// the whole of § 38.
    async fn record_failure(
        &self,
        run: NormalisationRunId,
        error: NormalisationError,
    ) -> NormalisationError {
        let reason = match error {
            NormalisationError::Store(_) => NormalisationFailure::StoreFailure,
            NormalisationError::UnmappedExercise { .. } => NormalisationFailure::UnmappedExercise,
            NormalisationError::MissingTimeZone => NormalisationFailure::MissingTimeZone,
        };
        let outcome = NormalisationOutcome::Failed {
            finished_at: self.clock.now(),
            reason,
        };
        // If recording the failure also fails, the original failure is the one
        // worth reporting — the store being unreachable is why we are here.
        let _ = self.runs.finish(run, outcome).await;
        error
    }
}

impl<R, T, W, F, G, C> WorkoutNormaliser for Normalisation<R, T, W, F, G, C>
where
    R: AccountReader + Sync,
    T: Translator<Account = R::Account> + Sync,
    T::Entity: NormalisedEntity + Send,
    W: NormalisedEntityStore<Entity = T::Entity> + Sync,
    F: RefusalStore + Sync,
    G: NormalisationRunLog + Sync,
    C: Clock + Sync,
{
    async fn normalise(&self) -> Result<NormalisationSummary, NormalisationError> {
        let stream = &self.stream;
        let started_at = self.clock.now();
        let run = self.runs.begin(stream, started_at).await?;

        // No lock. A derivation reads raw and writes only its own tables, so it
        // neither takes the extraction lock nor advances the resumption point —
        // a record landed after this began is simply picked up by the next one.
        let accounts = match self.raw.accounts().await {
            Ok(accounts) => accounts,
            Err(error) => return Err(self.record_failure(run, error.into()).await),
        };

        // Records, not accounts. A session composes several of them and the
        // reconciliation below is a statement about records (§ 38).
        let records_read =
            RecordCount::from(accounts.iter().map(SourceAccount::records).sum::<usize>());
        let mut derived = Derived::default();

        for account in &accounts {
            match self.translator.translate(account, &self.zone) {
                Ok(Translation::Entity { entity, refusals }) => {
                    derived.entities.push(*entity);
                    derived.refusals.extend(refusals);
                    // Every record in an account that translated is either
                    // composed into the entity or superseded by a later
                    // serving. Counted here rather than off the entity, which
                    // knows how many *rides* it holds and not how many
                    // responses each of those took.
                    derived.superseded = derived.superseded.saturating_add(account.superseded());
                    derived.composed = derived
                        .composed
                        .saturating_add(account.records().saturating_sub(account.superseded()));
                }
                Ok(Translation::Retraction { of }) => derived.retracted.push(of),
                Ok(Translation::Refused(refusals)) => {
                    // A superseded serving inside a refused account is counted
                    // as superseded rather than refused: what happened to it is
                    // that a later serving replaced it, which is true whatever
                    // became of the account.
                    derived.superseded = derived.superseded.saturating_add(account.superseded());
                    derived.refused = derived
                        .refused
                        .saturating_add(account.records().saturating_sub(account.superseded()));
                    derived.refusals.extend(refusals.iter().cloned());
                }
                // The only thing that stops a derivation is a gap in our own
                // vocabulary. Nothing is written, so the previous derivation is
                // left standing rather than half-replaced.
                Err(error) => return Err(self.record_failure(run, error).await),
            }
        }

        // The second pass. A retraction removes the entity composing the record
        // it names, wherever that entity sat in the sequence, and one naming a
        // record that was never landed removes nothing and is not an error.
        //
        // **Any of the entity's records, not just its first.** A session is
        // composed of several (§ 3.1), and a source withdrawing one of them is
        // no longer saying part of what the session asserts.
        // Accumulated as accounts translated, which is before the retraction
        // pass — a record composed into an entity a retraction then removed was
        // still composed rather than refused, and every record must be
        // accounted for exactly once.
        let records_composed = RecordCount::from(derived.composed);

        let before = derived.entities.len();
        derived.entities.retain(|entity| {
            !entity
                .composes()
                .iter()
                .any(|id| derived.retracted.contains(id))
        });
        let workouts_retracted = WorkoutCount::from(before.saturating_sub(derived.entities.len()));
        let retractions_read = RecordCount::from(derived.retracted.len());
        let records_refused = RecordCount::from(derived.refused);
        let records_superseded = RecordCount::from(derived.superseded);

        let written = match self.workouts.replace(run, derived.entities).await {
            Ok(written) => written,
            Err(error) => return Err(self.record_failure(run, error.into()).await),
        };
        let refusals = match self.refusals.replace(run, derived.refusals).await {
            Ok(refusals) => refusals,
            Err(error) => return Err(self.record_failure(run, error.into()).await),
        };

        let summary = NormalisationSummary {
            run_id: run,
            records_read,
            workouts_written: written,
            records_composed,
            records_superseded,
            workouts_retracted,
            retractions_read,
            records_refused,
            refusals_recorded: refusals,
        };

        let finished_at = self.clock.now();
        if let Err(error) = self
            .runs
            .finish(
                run,
                NormalisationOutcome::Succeeded {
                    finished_at,
                    records_read: summary.records_read,
                    workouts_written: summary.workouts_written,
                    records_composed: summary.records_composed,
                    records_superseded: summary.records_superseded,
                    workouts_retracted: summary.workouts_retracted,
                    retractions_read: summary.retractions_read,
                    records_refused: summary.records_refused,
                    refusals_recorded: summary.refusals_recorded,
                },
            )
            .await
        {
            return Err(error.into());
        }

        Ok(summary)
    }
}

/// Reporting what the domain would not accept.
///
/// Its own use case rather than a second trait on [`Normalisation`]. Reading
/// refusals needs the refusal store and the run log and nothing else — no
/// translator, no raw, and above all no declared zone. Hanging it off the
/// derivation would have made `fitness refusals` demand a time zone in order to
/// print a list it never consults one to produce.
pub struct Refusals<F, G> {
    stream: LandingStream,
    store: F,
    runs: G,
}

impl<F, G> Refusals<F, G>
where
    F: RefusalStore,
{
    /// No stream argument: the refusal store is bound to one and is asked
    /// which.
    pub fn new(store: F, runs: G) -> Self {
        Self {
            stream: store.stream().clone(),
            store,
            runs,
        }
    }
}

impl<F, G> RefusalReporter for Refusals<F, G>
where
    F: RefusalStore + Sync,
    G: NormalisationRunLog + Sync,
{
    async fn refusals(&self) -> Result<RefusalReport, NormalisationError> {
        let refusals = self.store.all().await?;
        // When the derivation ran, so a stale list reads as stale rather than
        // as current (§ 38).
        let derived_at = self
            .runs
            .latest_success(&self.stream)
            .await?
            .and_then(|run| run.outcome().finished_at());

        Ok(RefusalReport {
            derived_at,
            refusals,
        })
    }
}

/// Where the derivation stands.
///
/// Built from the same two ports as [`Refusals`] plus a count of raw, because
/// "how far behind is the normalised layer" is a question about both sides and
/// neither store can answer it alone.
pub struct DerivationStanding<L, W, F, G> {
    stream: LandingStream,
    raw: L,
    workouts: W,
    refusals: F,
    runs: G,
}

impl<L, W, F, G> DerivationStanding<L, W, F, G>
where
    W: NormalisedEntityStore,
{
    pub fn new(raw: L, workouts: W, refusals: F, runs: G) -> Self {
        Self {
            stream: workouts.stream().clone(),
            raw,
            workouts,
            refusals,
            runs,
        }
    }
}

impl<L, W, F, G> DerivationStatusReporter for DerivationStanding<L, W, F, G>
where
    L: RawExtent + Sync,
    W: NormalisedEntityStore + Sync,
    F: RefusalStore + Sync,
    G: NormalisationRunLog + Sync,
{
    async fn derivation_status(&self) -> Result<DerivationStatus, NormalisationError> {
        // Never having derived is a fact to report, not an error to raise.
        let last_success = self.runs.latest_success(&self.stream).await?;
        let workouts_held = self.workouts.count().await?;
        let refusals_held = RefusalCount::from(self.refusals.all().await?.len());

        let held = self.raw.records().await?.as_usize();
        let read = last_success.as_ref().map_or(0, |run| match run.outcome() {
            NormalisationOutcome::Succeeded { records_read, .. } => records_read.as_usize(),
            NormalisationOutcome::InFlight | NormalisationOutcome::Failed { .. } => 0,
        });

        Ok(DerivationStatus {
            last_success,
            workouts_held,
            refusals_held,
            records_behind: RecordCount::from(held.saturating_sub(read)),
        })
    }
}

impl NormalisationSummary {
    /// Whether every record is accounted for.
    ///
    /// Each landing record has exactly one outcome, so the three must add to
    /// the number read: a record was composed into an entity — one that stands
    /// or one a retraction then withdrew — or a later serving of the same thing
    /// replaced it, or it was a retraction of its own, or it was refused. A
    /// record that went missing shows up here as arithmetic that does not
    /// reconcile, which is why the numbers are reported rather than merely
    /// computed.
    ///
    /// **`workouts_written` is deliberately not in the sum.** It counts
    /// entities where everything else counts records, and one cycling session
    /// is up to six of them.
    pub const fn reconciles(&self) -> bool {
        let accounted = self
            .records_composed
            .as_usize()
            .saturating_add(self.records_superseded.as_usize())
            .saturating_add(self.retractions_read.as_usize())
            .saturating_add(self.records_refused.as_usize());
        accounted == self.records_read.as_usize()
    }
}
