//! The plan as it now stands, for every command that reads one (#177).
//!
//! **One place that works out which weeks were lost**, so that `fitness next`,
//! `gym next`, `mesocycle show` and `cycling next` cannot disagree about which
//! microcycle this week is. Each reads the store and nothing else: collecting
//! from a source is the porcelain's job, and a command asked where the plan
//! stands answers from what is already landed.

use application::{
    DiaryStore as _, MesocycleStore, PlanStore,
    microcycle::{Microcycle, MicrocyclePorts, Standing},
    reschedule::{Reschedule, Rescheduled},
};
use domain::{normalised::OperatorZone, schedule::DayPart};

use crate::{
    next::{NextFailure, destination},
    store::Store,
};

/// Where the plan stands at a moment: every week since it began, and this one.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read or the build names no
/// destination for a discipline.
pub async fn standing(
    store: &Store,
    zone: &OperatorZone,
    now: DayPart,
) -> Result<Standing, NextFailure> {
    Ok(Microcycle::new(
        MicrocyclePorts {
            diary: store.diary(),
            plans: store.plans(zone),
            gym_deliveries: store.gym_deliveries(),
            cycling_deliveries: store.cycling_deliveries(),
            gym_performed: store.canonical_gym(),
            cycling_performed: store.rides(),
        },
        destination(domain::schedule::Discipline::Gym)?,
        destination(domain::schedule::Discipline::Cycling)?,
    )
    .standing(now)
    .await?)
}

/// Which weeks were lost, as of now in the operator's zone.
///
/// **Now, whatever date a command was asked about.** Which weeks were lost is
/// a fact about the record, and the record holds nothing from the future — so
/// asking about next Friday reads the same lost weeks as asking about today.
///
/// # Errors
///
/// As [`standing`].
pub async fn reschedule(store: &Store, zone: &OperatorZone) -> Result<Reschedule, NextFailure> {
    let here = jiff::Timestamp::now().to_zoned(zone.as_time_zone());
    let standing = standing(store, zone, DayPart::containing(here.datetime())).await?;
    let diary = store.diary().diary().await?;
    Ok(Reschedule::new(standing.reruns, diary, standing.missed))
}

/// The gym's mesocycles, as the plan now stands.
///
/// # Errors
///
/// As [`reschedule`].
pub async fn gym(
    store: &Store,
    zone: &OperatorZone,
) -> Result<
    Rescheduled<impl MesocycleStore + Send + Sync + use<>, impl PlanStore + Send + Sync + use<>>,
    NextFailure,
> {
    Ok(Rescheduled::new(
        store.gym_mesocycles(zone),
        store.plans(zone),
        reschedule(store, zone).await?,
    ))
}
