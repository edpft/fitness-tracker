//! The plan as it now stands, for every command that reads one (#177).
//!
//! **One place that works out which weeks were lost**, so that `fitness next`,
//! `gym next`, `programme show` and `cycling next` cannot disagree about which
//! microcycle this week is. Each reads the store and nothing else: collecting
//! from a source is the porcelain's job, and a command asked where the plan
//! stands answers from what is already landed.

use application::{
    DiaryStore as _,
    microcycle::{Microcycle, MicrocyclePorts, Standing},
    reschedule::{Reschedule, Rescheduled},
};
use domain::{normalised::OperatorZone, schedule::DayPart};
use infrastructure::{
    SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore, SqliteCyclingSessionLog,
    SqliteDiaryStore, SqliteGymMesocycleStore, SqlitePerformedWorkoutReader, SqlitePlanStore,
    SqlitePool, SqlitePrescriptionDeliveryStore,
};

use crate::{Failure, next};

/// The gym's mesocycles, as the plan now stands.
pub type GymMesocycles = Rescheduled<SqliteGymMesocycleStore, SqlitePlanStore>;

/// Cycling's mesocycles, as the plan now stands.
pub type CyclingMesocycles = Rescheduled<SqliteCyclingMesocycleStore, SqlitePlanStore>;

/// Where the plan stands at a moment: every week since it began, and this one.
///
/// # Errors
///
/// [`Failure`] if the store cannot be read or the build names no destination
/// for a discipline.
pub async fn standing(
    pool: &SqlitePool,
    zone: &OperatorZone,
    now: DayPart,
) -> Result<Standing, Failure> {
    Ok(Microcycle::new(
        MicrocyclePorts {
            diary: SqliteDiaryStore::new(pool.clone()),
            plans: SqlitePlanStore::new(pool.clone(), zone.clone()),
            gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
            cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
            gym_performed: SqlitePerformedWorkoutReader::new(pool.clone()),
            cycling_performed: SqliteCyclingSessionLog::new(pool.clone()),
        },
        next::destination(domain::schedule::Discipline::Gym)?,
        next::destination(domain::schedule::Discipline::Cycling)?,
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
pub async fn reschedule(pool: &SqlitePool, zone: &OperatorZone) -> Result<Reschedule, Failure> {
    let here = jiff::Timestamp::now().to_zoned(zone.as_time_zone());
    let standing = standing(pool, zone, DayPart::containing(here.datetime())).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    Ok(Reschedule::new(standing.reruns, diary))
}

/// The gym's mesocycles, as the plan now stands.
///
/// # Errors
///
/// As [`reschedule`].
pub async fn gym(pool: &SqlitePool, zone: &OperatorZone) -> Result<GymMesocycles, Failure> {
    Ok(Rescheduled::new(
        SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
        SqlitePlanStore::new(pool.clone(), zone.clone()),
        reschedule(pool, zone).await?,
    ))
}

/// Cycling's mesocycles, as the plan now stands.
///
/// # Errors
///
/// As [`reschedule`].
pub async fn cycling(pool: &SqlitePool, zone: &OperatorZone) -> Result<CyclingMesocycles, Failure> {
    Ok(Rescheduled::new(
        SqliteCyclingMesocycleStore::new(pool.clone()),
        SqlitePlanStore::new(pool.clone(), zone.clone()),
        reschedule(pool, zone).await?,
    ))
}
