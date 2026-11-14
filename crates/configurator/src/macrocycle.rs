//! The macrocycle as planned, over this build's store: what Plan opens with
//! (#770, #789).

use application::{
    commit::{CommitError, CommitPorts, Committing},
    macrocycle::{Planned, PlanningPorts},
    next::Scope,
};
use domain::{macrocycle::PlannedMacrocycle, normalised::OperatorZone, schedule::DayPart};
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteDiaryStore, SqliteGenerationParameterStore,
    SqlitePelotonClassStore, SqlitePlanStore,
    peloton::{PelotonHoldingRides, PelotonProgrammes, catalogue::ClassCatalogue},
};

use crate::{
    holidays::{self, Calendars},
    next::NextFailure,
    rescheduling::Walk,
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

/// What Plan reads: the walk through the plan and the calendars its term is
/// read from, the plans and how each workout prescribes, the classes the
/// cycling mesocycles are built from, the rides done, and the diary.
///
/// **Of the type `planned` builds**, so what Plan reads cannot differ from what
/// it declares.
type Planning<'a> = Planned<
    Walk,
    Committing<
        SqlitePlanStore,
        SqliteGenerationParameterStore,
        PelotonHoldingRides<'a>,
        SqliteCanonicalCyclingSessionStore,
        PelotonProgrammes<'a>,
    >,
    SqliteDiaryStore,
    Calendars,
>;

/// The parts a run collecting what Plan reads takes (#809).
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Planning<'static>>()
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
    let classes = SqlitePelotonClassStore::new(pool.clone());
    let catalogue = ClassCatalogue::over(&classes, None);
    let use_case: Planning<'_> = Planned::new(PlanningPorts {
        walk: Walk::over(store, zone).await?,
        committing: Committing::new(
            CommitPorts {
                plans: SqlitePlanStore::new(pool.clone(), zone.clone()),
                parameters: SqliteGenerationParameterStore::new(pool.clone()),
                holding: PelotonHoldingRides::new(catalogue),
                ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
                published: PelotonProgrammes::new(catalogue),
            },
            zone.clone(),
        ),
        diary: SqliteDiaryStore::new(pool.clone()),
        holidays: holidays::calendars(pool),
    });
    Ok(use_case.on(now).await?)
}
