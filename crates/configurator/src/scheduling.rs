//! The ordinary week, alterations and illness, over this build's store (#670).

use std::{collections::BTreeMap, num::NonZeroU8};

use application::{ScheduleError, schedule::Scheduling};
use domain::{
    normalised::OperatorZone,
    schedule::{Alteration, Diary, Illness, SlotLength, TrainingPattern, TrainingSlot},
};
use jiff::civil::Date;

use crate::store::Store;

fn scheduling(
    store: &Store,
) -> Scheduling<impl application::DiaryStore + application::DiaryAuthor + Sync + use<>> {
    Scheduling::new(store.diary())
}

/// Everything the operator has said about their week.
///
/// # Errors
///
/// See [`Scheduling::diary`].
pub async fn diary(store: &Store) -> Result<Diary, ScheduleError> {
    scheduling(store).diary().await
}

/// Record the ordinary week.
///
/// # Errors
///
/// See [`Scheduling::add`].
pub async fn add(
    store: &Store,
    from: Date,
    zone: OperatorZone,
    slots: BTreeMap<TrainingSlot, SlotLength>,
) -> Result<TrainingPattern, ScheduleError> {
    scheduling(store).add(from, zone, slots).await
}

/// Record a run of days that departs from the ordinary week.
///
/// # Errors
///
/// See [`Scheduling::alter`].
pub async fn alter(
    store: &Store,
    start: Date,
    days: NonZeroU8,
    zone: Option<OperatorZone>,
    slots: Option<BTreeMap<TrainingSlot, SlotLength>>,
    reason: &str,
) -> Result<Alteration, ScheduleError> {
    scheduling(store)
        .alter(start, days, zone, slots, reason)
        .await
}

/// Record the operator ill from a date.
///
/// # Errors
///
/// See [`Scheduling::ill`].
pub async fn ill(store: &Store, from: Date) -> Result<Illness, ScheduleError> {
    scheduling(store).ill(from).await
}

/// The illness [`better`] would end.
///
/// # Errors
///
/// See [`Scheduling::open_illness`].
pub async fn open_illness(store: &Store) -> Result<Illness, ScheduleError> {
    scheduling(store).open_illness().await
}

/// End the open illness on the first day better.
///
/// # Errors
///
/// See [`Scheduling::better`].
pub async fn better(store: &Store, on: Date) -> Result<Illness, ScheduleError> {
    scheduling(store).better(on).await
}
