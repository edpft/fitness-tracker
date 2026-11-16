//! The macrocycle as planned, over this build's store: what Plan opens with
//! (#770, #789).

use application::graph::{Input, Reads};
use application::{
    HolidayCalendar as _,
    commit::{CommitError, CommitPorts, Committing},
    next::Scope,
    reschedule::WalkedPlans,
};
use application::{microcycle::Standing, next::PlanStanding, reschedule::Reschedule};
use domain::{
    macrocycle::PlannedMacrocycle,
    normalised::OperatorZone,
    schedule::{DayPart, Diary},
};
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteGenerationParameterStore, SqlitePlanStore,
    peloton::{PelotonHoldingRides, PelotonProgrammes, catalogue::ClassCatalogue},
};
use tracing::Instrument as _;

use crate::clock::Now;
use crate::store::Spanned;
use crate::{
    holidays::{self, Calendars},
    next::NextFailure,
    rescheduling::{Walk, Walked},
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

/// What builds Plan's planned mesocycles after the written ones: the plans,
/// how each workout prescribes, the classes the cycling mesocycles are built
/// from, and the rides done.
type Committer<'a> = Committing<
    Spanned<SqlitePlanStore>,
    Spanned<SqliteGenerationParameterStore>,
    Spanned<PelotonHoldingRides<'a>>,
    Spanned<SqliteCanonicalCyclingSessionStore>,
    Spanned<PelotonProgrammes<'a>>,
>;

/// What Plan reads (#805).
///
/// The walk through the plan, which reads the plans, the diary and the
/// calendars its term is read from; what the committer does; the calendars
/// again where the walk could not read them; and the part of the day it is
/// planned from.
#[must_use]
pub fn reads() -> Reads {
    Walk::reads()
        .and(<Committer<'static> as Input>::reads())
        .and(Calendars::reads())
        .and(Now::reads())
}

/// The parts a run collecting what Plan reads takes (#809).
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// The macrocycle it is now in, as planned: `None` where the calendar does
/// not reach it or no plan following a chain is in force.
///
/// **Read from the store alone.** The planned mesocycles are built from the
/// Peloton classes already stored, as committing builds them without a login,
/// so opening Plan reaches no source.
///
/// **What the walk read is read once** (#817): the plans, the diary and the
/// holidays it walked through are the ones the macrocycle is planned from.
///
/// # Errors
///
/// [`PlannedFailure`] if the store or the school calendar cannot be read, or
/// a planned mesocycle will not build.
pub async fn planned(
    store: &Store,
    zone: &OperatorZone,
    now: &Now,
) -> Result<Option<PlannedMacrocycle>, PlannedFailure> {
    let classes = store.peloton_classes();
    let catalogue = ClassCatalogue::over(&classes, None);
    let committer: Committer<'_> = Committing::new(
        CommitPorts {
            plans: store.adapter_with(SqlitePlanStore::new, zone.clone()),
            parameters: store.adapter(SqliteGenerationParameterStore::new),
            holding: Spanned::new(PelotonHoldingRides::new(catalogue)),
            ridden: store.adapter(SqliteCanonicalCyclingSessionStore::new),
            published: Spanned::new(PelotonProgrammes::new(catalogue)),
        },
        zone.clone(),
    );
    let walk = Walk::over(store, zone).await?;
    async {
        let walked = walk
            .walked_on(now.day_part())
            .await
            .map_err(CommitError::from)?;
        // The calendars are read again only where the walk could not read
        // them, so that their failure is said as it always was.
        let holidays = match walk.holidays() {
            Some(holidays) => holidays.clone(),
            None => holidays::calendars(store)
                .holidays()
                .await
                .map_err(CommitError::from)?,
        };
        let plans = WalkedPlans::new(walked.written, &walked.reschedule, None, &walk)
            .map_err(CommitError::from)?;
        committer
            .planned_among(
                now.day_part(),
                &holidays,
                &walked.standing,
                &walked.diary,
                plans.now(),
            )
            .await
    }
    .instrument(crate::telemetry::reading("macrocycle"))
    .await
    .map_err(PlannedFailure::from)
}

/// Where the plan stands at a part of the day, and what the walk read to say
/// so (#830): what the home page's microcycle and curves are drawn from, and
/// what is next is derived from.
pub struct Stood {
    pub now: DayPart,
    pub standing: Standing,
    pub diary: Diary,
    pub reschedule: Reschedule,
    pub plan: PlanStanding,
}

/// Where the plan stands at `now`, as a run says it.
///
/// **The walk is read once**, and what it read is held with it, so deriving
/// what is next walks nothing again.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub(crate) async fn stood(
    store: &Store,
    zone: &OperatorZone,
    now: DayPart,
) -> Result<Stood, NextFailure> {
    let classes = store.peloton_classes();
    let catalogue = ClassCatalogue::over(&classes, None);
    let committer: Committer<'_> = Committing::new(
        CommitPorts {
            plans: store.adapter_with(SqlitePlanStore::new, zone.clone()),
            parameters: store.adapter(SqliteGenerationParameterStore::new),
            holding: Spanned::new(PelotonHoldingRides::new(catalogue)),
            ridden: store.adapter(SqliteCanonicalCyclingSessionStore::new),
            published: Spanned::new(PelotonProgrammes::new(catalogue)),
        },
        zone.clone(),
    );
    let walk = Walk::over(store, zone).await?;
    let Walked {
        diary,
        standing,
        reschedule,
        ..
    } = walk.walked_on(now).await?;
    let holidays = match walk.holidays() {
        Some(holidays) => Ok(holidays.clone()),
        None => holidays::calendars(store).holidays().await,
    };
    let plan = application::next::plan_standing(
        &committer,
        &store.adapter_with(SqlitePlanStore::new, zone.clone()),
        now,
        holidays,
        &standing,
        &diary,
        &reschedule,
    )
    .await?;
    Ok(Stood {
        now,
        standing,
        diary,
        reschedule,
        plan,
    })
}
