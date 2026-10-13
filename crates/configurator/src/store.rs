//! The store, as the ports a frontend reads and writes it through.
//!
//! **A frontend names the port and this names the adapter** (#402). `cli` and
//! `web` open a [`Store`] and ask it for a diary or an exercise history; that
//! the store is SQLite, and which table answers which port, is decided here and
//! nowhere above it.

use std::path::Path;

use application::{
    CanonicalCyclingSessionStore, CanonicalGymSessionStore, CyclingDeliveryStore, DiaryAuthor,
    DiaryStore, ExerciseHistory, FtpHistory, GenerationParameterStore, MesocycleStore,
    NormalisedCyclingSessionReader, NormalisedGymSessionReader, PerformedGymSessions,
    PerformedSessionLog, PlanStore, PrescribedWorkoutStore, PrescriptionDeliveryStore,
    PrescriptionLifecycle, RiddenSessionLog, RiddenVenues, StoreError, WeighInHistory,
};
use domain::normalised::OperatorZone;
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore, SqliteCyclingDeliveryStore,
    SqliteDiaryStore, SqliteExerciseHistory, SqliteFtpHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteNormalisedCyclingSessionReader,
    SqliteNormalisedGymSessionReader, SqliteOperatorSettingsStore, SqlitePlanStore, SqlitePool,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, SqliteWeighInHistory, connect,
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

    /// The parameters a prescription is generated from.
    #[must_use]
    pub fn parameters(&self) -> impl GenerationParameterStore + Clone + Send + Sync + use<> {
        SqliteGenerationParameterStore::new(self.pool.clone())
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

    /// Every gym session each source recorded, in our terms.
    #[must_use]
    pub fn normalised_gym(&self) -> impl NormalisedGymSessionReader + Send + Sync + use<> {
        SqliteNormalisedGymSessionReader::new(self.pool.clone())
    }

    /// Every cycling session, however many sources recorded it.
    #[must_use]
    pub fn canonical_cycling(&self) -> impl CanonicalCyclingSessionStore + Send + Sync + use<> {
        SqliteCanonicalCyclingSessionStore::new(self.pool.clone())
    }

    /// Every cycling session each source recorded, in our terms.
    #[must_use]
    pub fn normalised_cycling(&self) -> impl NormalisedCyclingSessionReader + Send + Sync + use<> {
        SqliteNormalisedCyclingSessionReader::new(self.pool.clone())
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

    /// Body weight, as it was measured.
    #[must_use]
    pub fn weigh_ins(&self) -> impl WeighInHistory + Send + Sync + use<> {
        SqliteWeighInHistory::new(self.pool.clone())
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

    /// Close it, so the next open sees everything written.
    pub async fn close(self) {
        self.pool.close().await;
    }
}
