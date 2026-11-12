//! The macrocycle as planned, over this build's store: what Plan opens with
//! (#770, #789).

use application::{
    commit::{CommitError, CommitPorts, Committing},
    macrocycle::{Planned, PlanningPorts},
};
use domain::{macrocycle::PlannedMacrocycle, normalised::OperatorZone, schedule::DayPart};
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteDiaryStore, SqliteGenerationParameterStore,
    SqlitePelotonClassStore, SqlitePlanStore, SqlitePrescriptionDeliveryStore,
    peloton::{PelotonHoldingRides, PelotonProgrammes, catalogue::ClassCatalogue},
};

use crate::{
    holidays::{self, Calendars},
    next::{NextFailure, microcycle},
    store::Store,
};

/// Why the planned macrocycle could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PlannedFailure {
    #[error(transparent)]
    Wiring(#[from] NextFailure),
    #[error(transparent)]
    Commit(#[from] CommitError),
}

/// The macrocycle `now` falls in, as planned: `None` where the calendar does
/// not reach it or no plan following a chain is in force.
///
/// **Read from the store alone.** The planned mesocycles are built from the
/// Peloton classes already stored, as committing builds them without a login,
/// so opening Plan reaches no source.
///
/// # Errors
///
/// [`PlannedFailure`] if the store or the school calendar cannot be read, or
/// a planned mesocycle will not build.
pub async fn planned(
    store: &Store,
    zone: &OperatorZone,
    now: DayPart,
) -> Result<Option<PlannedMacrocycle>, PlannedFailure> {
    let pool = store.pool();
    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let classes = SqlitePelotonClassStore::new(pool.clone());
    let catalogue = ClassCatalogue::over(&classes, None);
    let walk = microcycle(pool, &plans, &deliveries, holidays::term().await)?;
    let use_case = Planned::new(PlanningPorts {
        walk,
        committing: Committing::new(
            CommitPorts {
                plans,
                parameters: SqliteGenerationParameterStore::new(pool.clone()),
                holding: PelotonHoldingRides::new(catalogue),
                ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
                published: PelotonProgrammes::new(catalogue),
            },
            zone.clone(),
        ),
        diary: SqliteDiaryStore::new(pool.clone()),
        holidays: Calendars,
    });
    Ok(use_case.on(now).await?)
}
