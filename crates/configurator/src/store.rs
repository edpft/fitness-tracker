//! The store, as the ports a frontend reads and writes it through.
//!
//! **A frontend names the port and this names the adapter** (#402). `cli` and
//! `web` open a [`Store`] and ask it for a diary or an exercise history; that
//! the store is SQLite, and which table answers which port, is decided here and
//! nowhere above it.
//!
//! **The pool is held here and never handed out** (#816). Every adapter is
//! built through [`Store::adapter`] and comes back [`Spanned`], so each call a
//! use case makes through a port is a `call` span, and an adapter added later
//! is spanned without anyone listing it.

mod spanned;

pub use infrastructure::KeptDrawing;
pub use spanned::Spanned;

use spanned::Answers;

use std::path::Path;
use std::sync::Arc;

use tracing::Instrument as _;

use application::drawing::{Drawing, DrawnFrom};
use application::graph::Reads;
use application::next::Canonicalising;
use application::{
    CanonicalGymSessionStore, CyclingDeliveryStore, DiaryAuthor, DiaryStore, EquipmentStore,
    ExerciseHistory, FillChangeStore, FtpHistory, GenerationParameterStore, MesocycleStore,
    PerformedGymSessions, PerformedSessionLog, PlanStore, PrescribedWorkoutStore,
    PrescriptionDeliveryStore, PrescriptionLifecycle, ProgressionStore, RiddenSessionLog,
    RiddenVenues, StoreError, WorkoutStore,
};
use domain::normalised::OperatorZone;
use infrastructure::{
    FileRunLock, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCyclingDeliveryStore, SqliteDerivationLedger, SqliteDiaryStore, SqliteEquipmentStore,
    SqliteExerciseHistory, SqliteFtpHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteKeptDrawings, SqliteOperatorSettingsStore,
    SqlitePelotonClassStore, SqlitePlanStore, SqlitePool, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteRevisions, connect_counting,
};

/// The operator's store, opened.
///
/// **Opened once, by the process's entry point, and passed down** (#663).
/// Nothing below `main` takes the database's path, so nothing below it can open
/// a second pool: a clone shares this one's connections.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
    path: Arc<Path>,
    /// What the adapters over it answered, kept until it is next written.
    kept: Answers,
}

impl Store {
    /// Close every connection, waiting for each to finish (#817): what a
    /// command does last, so that each span its work opened has closed while
    /// the telemetry is still being recorded.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    /// Open the store at a path, creating it if it is not there.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if it cannot be opened or migrated.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one place the store is opened, so the pool stays here"
    )]
    pub async fn open(database: &Path) -> Result<Self, StoreError> {
        use tracing::Instrument as _;
        let (pool, commits) = connect_counting(database)
            .instrument(crate::telemetry::opening())
            .await?;
        Ok(Self {
            pool,
            path: Arc::from(database),
            kept: Answers::until(commits),
        })
    }

    /// The locks that keep two runs from collecting the same stream, beside
    /// this store: a copy of it elsewhere is a separate system with its own.
    #[must_use]
    pub fn run_lock(&self) -> FileRunLock {
        FileRunLock::beside(&self.path)
    }

    /// What `compute` gives, as it last gave it where nothing `on` declares
    /// has moved since (#830), and computed otherwise.
    ///
    /// **`with` is what else the answer depends on**, and `name` names it.
    pub(crate) fn held<T, E, F>(
        &self,
        name: &'static str,
        with: String,
        on: &Reads,
        compute: F,
    ) -> impl Future<Output = Result<T, E>> + Send + use<T, E, F>
    where
        T: Clone + Send + Sync + 'static,
        F: Future<Output = Result<T, E>> + Send,
    {
        let answers = self.kept.clone();
        let on = on.clone();
        async move {
            let at = match answers.answered::<T>(name, &with) {
                Ok(answer) => return Ok(answer),
                Err(at) => at,
            };
            let answer = compute.await?;
            if let Some(at) = at {
                answers.keep(name, with, at, &on, answer.clone());
            }
            Ok(answer)
        }
    }

    /// The adapter `new` builds over the store, spanned.
    ///
    /// A function and not a closure, so that the pool it is given cannot be
    /// kept: it goes into the adapter and nowhere else.
    pub(crate) fn adapter<A>(&self, new: fn(SqlitePool) -> A) -> Spanned<A> {
        Spanned::kept(new(self.pool.clone()), self.kept.clone())
    }

    /// The adapter `new` builds over the store, spanned, where it could be
    /// built.
    ///
    /// # Errors
    ///
    /// Why `new` could not build it.
    pub(crate) fn try_adapter<A, E>(
        &self,
        new: fn(SqlitePool) -> Result<A, E>,
    ) -> Result<Spanned<A>, E> {
        new(self.pool.clone()).map(|adapter| Spanned::kept(adapter, self.kept.clone()))
    }

    /// The adapter `new` builds over the store and `with`, spanned.
    pub(crate) fn adapter_with<W, A>(&self, new: fn(SqlitePool, W) -> A, with: W) -> Spanned<A> {
        Spanned::kept(new(self.pool.clone(), with), self.kept.clone())
    }

    /// The adapter `new` builds over the store and `with`, spanned, where it
    /// could be built.
    ///
    /// # Errors
    ///
    /// Why `new` could not build it.
    pub(crate) fn try_adapter_with<W, A, E>(
        &self,
        new: fn(SqlitePool, W) -> Result<A, E>,
        with: W,
    ) -> Result<Spanned<A>, E> {
        new(self.pool.clone(), with).map(|adapter| Spanned::kept(adapter, self.kept.clone()))
    }

    /// Peloton's class library, as a [`ClassCatalogue`] reads it.
    ///
    /// **Not spanned, because it is not a port.** It is read only inside the
    /// catalogue, and the ports the catalogue serves through are spanned where
    /// they are composed.
    ///
    /// [`ClassCatalogue`]: infrastructure::peloton::catalogue::ClassCatalogue
    pub(crate) fn peloton_classes(&self) -> SqlitePelotonClassStore {
        SqlitePelotonClassStore::new(self.pool.clone())
    }

    /// When there is room to train, and what departs from it.
    #[must_use]
    pub fn diary(&self) -> impl DiaryStore + DiaryAuthor + Send + Sync + use<> {
        self.adapter(SqliteDiaryStore::new)
    }

    /// The plans the operator has committed to, read in their zone.
    #[must_use]
    pub fn plans(&self, zone: &OperatorZone) -> impl PlanStore + Clone + Send + Sync + use<> {
        self.adapter_with(SqlitePlanStore::new, zone.clone())
    }

    /// What a prescription is generated from: the load scales (§ 14), and the
    /// versions of the programme and each primary progression (#192).
    #[must_use]
    pub fn parameters(
        &self,
    ) -> impl GenerationParameterStore + WorkoutStore + ProgressionStore + Clone + Send + Sync + use<>
    {
        self.adapter(SqliteGenerationParameterStore::new)
    }

    /// The equipment facts (#450).
    #[must_use]
    pub fn equipment(&self) -> impl EquipmentStore + Clone + Send + Sync + use<> {
        self.adapter(SqliteEquipmentStore::new)
    }

    /// Every exercise the gym record holds, and what was lifted in it.
    #[must_use]
    pub fn exercise_history(&self) -> impl ExerciseHistory + Send + Sync + use<> {
        self.adapter(SqliteExerciseHistory::new)
    }

    /// The gym's mesocycles, as they were committed.
    #[must_use]
    pub fn gym_mesocycles(&self, zone: &OperatorZone) -> impl MesocycleStore + Send + Sync + use<> {
        self.adapter_with(SqliteGymMesocycleStore::new, zone.clone())
    }

    /// Where changes to a mesocycle's fills are kept (#454).
    #[must_use]
    pub fn fill_changes(&self, zone: &OperatorZone) -> impl FillChangeStore + Send + Sync + use<> {
        self.adapter_with(SqliteGymMesocycleStore::new, zone.clone())
    }

    /// The sessions prescribed, recorded in the operator's zone.
    #[must_use]
    pub fn prescriptions(
        &self,
        zone: &OperatorZone,
    ) -> impl PrescribedWorkoutStore + Send + Sync + use<> {
        self.adapter_with(SqlitePrescribedWorkoutStore::new, zone.id().to_owned())
    }

    /// Where each gym prescription was put, and what became of it.
    #[must_use]
    pub fn gym_deliveries(
        &self,
    ) -> impl PrescriptionDeliveryStore + PrescriptionLifecycle + Clone + Send + Sync + use<> {
        self.adapter(SqlitePrescriptionDeliveryStore::new)
    }

    /// Where each ride was put.
    #[must_use]
    pub fn cycling_deliveries(&self) -> impl CyclingDeliveryStore + Send + Sync + use<> {
        self.adapter(SqliteCyclingDeliveryStore::new)
    }

    /// Every gym visit, however many sources recorded it.
    #[must_use]
    pub fn canonical_gym(
        &self,
    ) -> impl CanonicalGymSessionStore + PerformedGymSessions + PerformedSessionLog + Send + Sync + use<>
    {
        self.adapter(SqliteCanonicalGymSessionStore::new)
    }

    /// Rebuilding each canonical layer from the normalised ones, logged, each
    /// rebuild a `step` span (#631).
    #[must_use]
    pub fn canonicalising(&self) -> impl Canonicalising + Send + Sync + use<> {
        crate::telemetry::Traced(crate::next::canonical(self))
    }

    /// The rides performed, as the canonical layer holds them.
    #[must_use]
    pub fn rides(&self) -> impl RiddenSessionLog + Send + Sync + use<> {
        self.adapter(SqliteCanonicalCyclingSessionStore::new)
    }

    /// The classes already ridden, as the canonical layer holds them.
    #[must_use]
    pub fn ridden_venues(&self) -> impl RiddenVenues + Send + Sync + use<> {
        self.adapter(SqliteCanonicalCyclingSessionStore::new)
    }

    /// The functional threshold power, as it has been tested.
    #[must_use]
    pub fn ftp(&self) -> impl FtpHistory + Send + Sync + use<> {
        self.adapter(SqliteFtpHistory::new)
    }

    /// The zone the operator stated, if they stated one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn timezone(&self) -> Result<Option<String>, StoreError> {
        self.adapter(SqliteOperatorSettingsStore::new)
            .call(
                "SqliteOperatorSettingsStore::timezone",
                SqliteOperatorSettingsStore::timezone,
            )
            .await
    }

    /// State the operator's zone, superseding any stated before.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn state_timezone(&self, at: jiff::Timestamp, zone: &str) -> Result<(), StoreError> {
        self.adapter(SqliteOperatorSettingsStore::new)
            .call("SqliteOperatorSettingsStore::author", |adapter| {
                adapter.author(at, zone)
            })
            .await
    }

    /// What a page reading `reads` would be drawn from `now`, by `build`:
    /// `None` where something it reads has no version, or the build is not
    /// known, which is a page drawn every time it is opened (#805).
    ///
    /// **Read before the page is**, so a write landing while it is drawn is
    /// one its key does not include, and the next load draws it again.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn drawn_from(
        &self,
        reads: &Reads,
        build: Option<&str>,
        now: &crate::clock::Now,
    ) -> Result<Option<DrawnFrom>, StoreError> {
        let Some(build) = build else {
            return Ok(None);
        };
        Drawing::new(
            self.adapter(SqliteDerivationLedger::new),
            self.adapter(SqliteRevisions::new),
        )
        .drawn_from(crate::graph::graph(), reads, build, now.day_part())
        .instrument(crate::telemetry::keying())
        .await
    }

    /// Where what is read stands, asked once for each node however often it
    /// is asked (#830): one for each page load.
    pub(crate) fn drawing(
        &self,
    ) -> Drawing<Spanned<SqliteDerivationLedger>, Spanned<SqliteRevisions>> {
        Drawing::new(
            self.adapter(SqliteDerivationLedger::new),
            self.adapter(SqliteRevisions::new),
        )
    }

    /// Keep `drawing` as the page at `address`, drawn from `drawn_from`, so
    /// it can be shown at once the next time that page is opened, by this
    /// server or after a restart (#605).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn keep_drawing(
        &self,
        address: &str,
        drawn_from: Option<DrawnFrom>,
        drawing: &str,
    ) -> Result<(), StoreError> {
        self.adapter(SqliteKeptDrawings::new)
            .call("SqliteKeptDrawings::keep", |adapter| {
                adapter.keep(address, drawn_from, drawing)
            })
            .await
    }

    /// The page at `address` as it was last kept, if it has been.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn kept_drawing(&self, address: &str) -> Result<Option<KeptDrawing>, StoreError> {
        self.adapter(SqliteKeptDrawings::new)
            .call("SqliteKeptDrawings::kept", |adapter| adapter.kept(address))
            .await
    }
}
