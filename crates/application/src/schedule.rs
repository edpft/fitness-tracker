//! Recording the ordinary week, altering it, and illness (#670).
//!
//! **What each asks, and what each refuses, are here and nowhere else.** The
//! operator, 2026-10-31: the use case *"should never have been in the CLI"*.
//! A frontend asks the questions these take and prints what they return, so a
//! page offering the same thing has nothing to implement again.

use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    normalised::OperatorZone,
    planner::commencing,
    schedule::{self, Alteration, Diary, Illness, SlotLength, TrainingPattern, TrainingSlot},
};
use jiff::civil::{Date, Weekday};

use crate::{DiaryAuthor, DiaryStore, ScheduleError};

/// The weekdays to ask about for a run of days, in the order it meets them.
///
/// Friday 18 December for 3 days is Friday, Saturday and Sunday: a slot on any
/// other weekday is one [`Scheduling::alter`] refuses.
#[must_use]
pub fn weekdays(start: Date, days: NonZeroU8) -> Vec<Weekday> {
    schedule::covered_weekdays(start, days)
}

/// The seven days of the calendar week containing `date`, Monday first.
#[must_use]
pub fn days_of(date: Date) -> Vec<Date> {
    let monday = commencing(date);
    (0..7)
        .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
        .collect()
}

/// The use case, over the store that keeps the diary.
pub struct Scheduling<S> {
    store: S,
}

impl<S: DiaryStore + DiaryAuthor + Sync> Scheduling<S> {
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Everything the operator has said about their week.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::Store`] if the store cannot be read.
    pub async fn diary(&self) -> Result<Diary, ScheduleError> {
        Ok(self.store.diary().await?)
    }

    /// Record the ordinary week, in force from `from`.
    ///
    /// **Nothing to refuse.** A week with no slots is a period with no room to
    /// train, and is recorded as one.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::Store`] if the store cannot be written.
    pub async fn add(
        &self,
        from: Date,
        zone: OperatorZone,
        slots: BTreeMap<TrainingSlot, SlotLength>,
    ) -> Result<TrainingPattern, ScheduleError> {
        let pattern = TrainingPattern::new(from, zone, slots);
        self.store.record_pattern(&pattern).await?;
        Ok(pattern)
    }

    /// Record a run of days that departs from the ordinary week.
    ///
    /// `zone` is `None` when it is unchanged; `slots` is `None` when the
    /// ordinary week stands, and empty when there is no room at all.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoReason`] for a blank reason,
    /// [`ScheduleError::SlotOutsideRun`] for a slot on a weekday the run does
    /// not meet, and [`ScheduleError::Store`] if the store cannot be written.
    pub async fn alter(
        &self,
        start: Date,
        days: NonZeroU8,
        zone: Option<OperatorZone>,
        slots: Option<BTreeMap<TrainingSlot, SlotLength>>,
        reason: &str,
    ) -> Result<Alteration, ScheduleError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(ScheduleError::NoReason);
        }

        let covered = weekdays(start, days);
        if let Some(outside) = slots
            .iter()
            .flat_map(BTreeMap::keys)
            .find(|slot| !covered.contains(&slot.weekday))
        {
            return Err(ScheduleError::SlotOutsideRun {
                slot: outside.to_string(),
                start,
                days: days.get(),
            });
        }

        let alteration = Alteration::new(start, days, zone, slots, reason.to_owned());
        self.store.record_alteration(&alteration).await?;
        Ok(alteration)
    }

    /// Record a calendar week as it was tapped: the days that differ from the
    /// week as it stands, as one alteration from the first of them to the
    /// last (#671).
    ///
    /// **One run, as `alter` would have been asked for it.** Friday evening
    /// and Sunday morning tapped off the week commencing 14 December is 18
    /// December for 3 days with no room to train; Saturday, unchanged, is in
    /// the run and keeps what it had.
    ///
    /// `week` is any date in the week; `slots` is the whole week as tapped.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoReason`] for a blank reason,
    /// [`ScheduleError::Unchanged`] when no day differs, and
    /// [`ScheduleError::Store`] if the store cannot be read or written.
    pub async fn alter_week(
        &self,
        week: Date,
        slots: &BTreeMap<TrainingSlot, SlotLength>,
        reason: &str,
    ) -> Result<Alteration, ScheduleError> {
        if reason.trim().is_empty() {
            return Err(ScheduleError::NoReason);
        }

        let diary = self.store.diary().await?;
        let tapped = |date: Date| -> Vec<(TrainingSlot, SlotLength)> {
            slots
                .iter()
                .filter(|(slot, _)| slot.weekday == date.weekday())
                .map(|(slot, length)| (*slot, *length))
                .collect()
        };
        let changed: Vec<Date> = days_of(week)
            .into_iter()
            .filter(|date| diary.free_on(*date, false) != tapped(*date))
            .collect();
        let (Some(&start), Some(&last)) = (changed.first(), changed.last()) else {
            return Err(ScheduleError::Unchanged {
                what: "week as it stands",
            });
        };

        let run = start.until(last).map_or(0, |span| span.get_days()) + 1;
        let days = u8::try_from(run)
            .ok()
            .and_then(NonZeroU8::new)
            .unwrap_or(NonZeroU8::MIN);
        let covered = weekdays(start, days);
        let stated = slots
            .iter()
            .filter(|(slot, _)| covered.contains(&slot.weekday))
            .map(|(slot, length)| (*slot, *length))
            .collect();
        self.alter(start, days, None, Some(stated), reason).await
    }

    /// Record a calendar week as it was tapped as the weekly availability,
    /// from its Monday on (#671), in the time zone of the one it replaces.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoWeeklyAvailability`] when none is recorded to take
    /// a time zone from, [`ScheduleError::Unchanged`] when it is the weekly
    /// availability already in force, and [`ScheduleError::Store`] if the
    /// store cannot be read or written.
    pub async fn from_week(
        &self,
        week: Date,
        slots: BTreeMap<TrainingSlot, SlotLength>,
    ) -> Result<TrainingPattern, ScheduleError> {
        let monday = commencing(week);
        let diary = self.store.diary().await?;
        let replaced = diary
            .pattern_on(monday)
            .or_else(|| diary.patterns().first())
            .ok_or(ScheduleError::NoWeeklyAvailability)?;
        if diary
            .pattern_on(monday)
            .is_some_and(|ordinary| *ordinary.slots() == slots)
        {
            return Err(ScheduleError::Unchanged {
                what: "weekly availability",
            });
        }
        self.add(monday, replaced.zone().clone(), slots).await
    }

    /// Record the operator ill from `from`, and still ill.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::Store`] if the store cannot be written.
    pub async fn ill(&self, from: Date) -> Result<Illness, ScheduleError> {
        let illness = Illness::new(from);
        self.store.record_illness(&illness).await?;
        Ok(illness)
    }

    /// The illness [`Self::better`] would end.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoIllnessOpen`] if there is none, and
    /// [`ScheduleError::Store`] if the store cannot be read.
    pub async fn open_illness(&self) -> Result<Illness, ScheduleError> {
        self.store
            .diary()
            .await?
            .open_illness()
            .copied()
            .ok_or(ScheduleError::NoIllnessOpen)
    }

    /// End the open illness: `on` is the first day the operator is better.
    ///
    /// # Errors
    ///
    /// [`ScheduleError::NoIllnessOpen`] if there is none,
    /// [`ScheduleError::Illness`] if `on` is before it began, and
    /// [`ScheduleError::Store`] if the store cannot be read or written.
    pub async fn better(&self, on: Date) -> Result<Illness, ScheduleError> {
        let open = self.open_illness().await?;
        let illness = open.ended(on)?;
        self.store.record_illness(&illness).await?;
        Ok(illness)
    }
}
