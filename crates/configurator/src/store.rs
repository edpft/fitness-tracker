//! The store, as the ports a frontend reads and writes it through.
//!
//! **A frontend names the port and this names the adapter** (#402). `cli` and
//! `web` open a [`Store`] and ask it for a diary or an exercise history; that
//! the store is SQLite, and which table answers which port, is decided here and
//! nowhere above it.

use std::path::Path;

use application::next::Canonicalising;
use application::{
    CanonicalGymSessionStore, CyclingDeliveryStore, DiaryAuthor, DiaryStore, EquipmentStore,
    ExerciseHistory, FillChangeStore, FtpHistory, GenerationParameterStore, MesocycleStore,
    PerformedGymSessions, PerformedSessionLog, PlanStore, PrescribedWorkoutStore,
    PrescriptionDeliveryStore, PrescriptionLifecycle, ProgressionStore, RiddenSessionLog,
    RiddenVenues, StoreError, WeighInHistory, WorkoutStore,
};
use domain::normalised::OperatorZone;
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCanonicalWeighInStore, SqliteCyclingDeliveryStore, SqliteDiaryStore,
    SqliteEquipmentStore, SqliteExerciseHistory, SqliteFtpHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteKeptDrawings, SqliteOperatorSettingsStore, SqlitePlanStore,
    SqlitePool, SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, connect,
};

/// The operator's store, opened.
#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open the store at a path, creating it if it is not there.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if it cannot be opened or migrated.
    pub async fn open(database: &Path) -> Result<Self, StoreError> {
        Ok(Self {
            pool: connect(database).await?,
        })
    }

    pub(crate) const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// When there is room to train, and what departs from it.
    #[must_use]
    pub fn diary(&self) -> impl DiaryStore + DiaryAuthor + Send + Sync + use<> {
        SqliteDiaryStore::new(self.pool.clone())
    }

    /// The plans the operator has committed to, read in their zone.
    #[must_use]
    pub fn plans(&self, zone: &OperatorZone) -> impl PlanStore + Clone + Send + Sync + use<> {
        SqlitePlanStore::new(self.pool.clone(), zone.clone())
    }

    /// What a prescription is generated from: the load scales (§ 14), and the
    /// versions of the programme and each primary progression (#192).
    #[must_use]
    pub fn parameters(
        &self,
    ) -> impl GenerationParameterStore + WorkoutStore + ProgressionStore + Clone + Send + Sync + use<>
    {
        SqliteGenerationParameterStore::new(self.pool.clone())
    }

    /// The equipment facts (#450).
    #[must_use]
    pub fn equipment(&self) -> impl EquipmentStore + Clone + Send + Sync + use<> {
        SqliteEquipmentStore::new(self.pool.clone())
    }

    /// Every exercise the gym record holds, and what was lifted in it.
    #[must_use]
    pub fn exercise_history(&self) -> impl ExerciseHistory + Send + Sync + use<> {
        SqliteExerciseHistory::new(self.pool.clone())
    }

    /// The gym's mesocycles, as they were committed.
    #[must_use]
    pub fn gym_mesocycles(&self, zone: &OperatorZone) -> impl MesocycleStore + Send + Sync + use<> {
        SqliteGymMesocycleStore::new(self.pool.clone(), zone.clone())
    }

    /// Where changes to a mesocycle's fills are kept (#454).
    #[must_use]
    pub fn fill_changes(&self, zone: &OperatorZone) -> impl FillChangeStore + Send + Sync + use<> {
        SqliteGymMesocycleStore::new(self.pool.clone(), zone.clone())
    }

    /// The sessions prescribed, recorded in the operator's zone.
    #[must_use]
    pub fn prescriptions(
        &self,
        zone: &OperatorZone,
    ) -> impl PrescribedWorkoutStore + Send + Sync + use<> {
        SqlitePrescribedWorkoutStore::new(self.pool.clone(), zone.id().to_owned())
    }

    /// Where each gym prescription was put, and what became of it.
    #[must_use]
    pub fn gym_deliveries(
        &self,
    ) -> impl PrescriptionDeliveryStore + PrescriptionLifecycle + Clone + Send + Sync + use<> {
        SqlitePrescriptionDeliveryStore::new(self.pool.clone())
    }

    /// Where each ride was put.
    #[must_use]
    pub fn cycling_deliveries(&self) -> impl CyclingDeliveryStore + Send + Sync + use<> {
        SqliteCyclingDeliveryStore::new(self.pool.clone())
    }

    /// Every gym visit, however many sources recorded it.
    #[must_use]
    pub fn canonical_gym(
        &self,
    ) -> impl CanonicalGymSessionStore + PerformedGymSessions + PerformedSessionLog + Send + Sync + use<>
    {
        SqliteCanonicalGymSessionStore::new(self.pool.clone())
    }

    /// Rebuilding each canonical layer from the normalised ones, logged.
    #[must_use]
    pub fn canonicalising(&self) -> impl Canonicalising + Send + Sync + use<> {
        crate::next::canonical(&self.pool)
    }

    /// The rides performed, as the canonical layer holds them.
    #[must_use]
    pub fn rides(&self) -> impl RiddenSessionLog + Send + Sync + use<> {
        SqliteCanonicalCyclingSessionStore::new(self.pool.clone())
    }

    /// The classes already ridden, as the canonical layer holds them.
    #[must_use]
    pub fn ridden_venues(&self) -> impl RiddenVenues + Send + Sync + use<> {
        SqliteCanonicalCyclingSessionStore::new(self.pool.clone())
    }

    /// The functional threshold power, as it has been tested.
    #[must_use]
    pub fn ftp(&self) -> impl FtpHistory + Send + Sync + use<> {
        SqliteFtpHistory::new(self.pool.clone())
    }

    /// Body weight, one weigh-in per weighing.
    #[must_use]
    pub fn weigh_ins(&self) -> impl WeighInHistory + Send + Sync + use<> {
        SqliteCanonicalWeighInStore::new(self.pool.clone())
    }

    /// The zone the operator stated, if they stated one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn timezone(&self) -> Result<Option<String>, StoreError> {
        SqliteOperatorSettingsStore::new(self.pool.clone())
            .timezone()
            .await
    }

    /// State the operator's zone, superseding any stated before.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn state_timezone(&self, at: jiff::Timestamp, zone: &str) -> Result<(), StoreError> {
        SqliteOperatorSettingsStore::new(self.pool.clone())
            .author(at, zone)
            .await
    }

    /// Keep `drawing` as the page at `address`, so it can be shown at once
    /// the next time that page is opened, by this server or after a restart
    /// (#605).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn keep_drawing(&self, address: &str, drawing: &str) -> Result<(), StoreError> {
        SqliteKeptDrawings::new(self.pool.clone())
            .keep(address, drawing)
            .await
    }

    /// The page at `address` as it was last kept, if it has been.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn kept_drawing(&self, address: &str) -> Result<Option<String>, StoreError> {
        SqliteKeptDrawings::new(self.pool.clone())
            .kept(address)
            .await
    }

    /// Close it, so the next open sees everything written.
    pub async fn close(self) {
        self.pool.close().await;
    }
}
