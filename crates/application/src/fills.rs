//! Changing a slot's fill partway through a mesocycle (#454).
//!
//! The change holds from a date, so a session prescribed before it keeps
//! what it was prescribed; and it names what assists the exercise, which must
//! already be known equipment.

use domain::{
    gym::{Exercise, equipment::Specific},
    prescription::{FillChange, SlotId, linear::SlotContent},
    schedule::{Relative, SessionRole},
};
use jiff::{Timestamp, civil::Date};

use crate::{EquipmentStore, FillChangeError, FillChangeStore, MesocycleInForce, MesocycleStore};

/// The use case, over the mesocycles, where changes are kept, and the
/// equipment.
pub struct Refilling<M, C, E> {
    mesocycles: M,
    changes: C,
    equipment: E,
}

impl<M, C, E> Refilling<M, C, E>
where
    M: MesocycleStore + Sync,
    C: FillChangeStore + Sync,
    E: EquipmentStore + Sync,
{
    pub const fn new(mesocycles: M, changes: C, equipment: E) -> Self {
        Self {
            mesocycles,
            changes,
            equipment,
        }
    }

    /// The mesocycle in force on a date, with its changes.
    ///
    /// # Errors
    ///
    /// [`FillChangeError::Store`] if the store cannot be read.
    pub async fn on(&self, date: Date) -> Result<Option<MesocycleInForce>, FillChangeError> {
        Ok(self.mesocycles.on(date).await?)
    }

    /// Fill a slot with an exercise from a date, assisted by a specific or
    /// not.
    ///
    /// # Errors
    ///
    /// [`FillChangeError`] if no mesocycle is in force then, the slot is
    /// static or the primary, or the range of bands is not known on that
    /// date.
    pub async fn change(
        &self,
        slot: SlotId,
        intensity: Option<Relative>,
        from: Date,
        exercise: Exercise,
        assisted_by: Option<Specific>,
        at: Timestamp,
    ) -> Result<(), FillChangeError> {
        let Some(found) = self.mesocycles.on(from).await? else {
            return Err(FillChangeError::NoMesocycle { date: from });
        };
        // The primary is knee or hip dominant, and is what the mesocycle
        // progresses; a static slot carries its own sets and reps.
        let fills = found.mesocycle.fills();
        let primary = found.mesocycle.primary_exercise();
        let is_primary = matches!(slot, SlotId::KneeDominant | SlotId::HipDominant)
            && SessionRole::ALL.iter().any(|role| {
                matches!(fills.content(slot, *role), SlotContent::Single(held) if *held == primary)
            });
        if is_primary || fills.clone().single_mut(slot).is_none() {
            return Err(FillChangeError::NotChangeable {
                slot: slot.as_str(),
            });
        }
        if let Some(specific) = &assisted_by {
            let known = self
                .equipment
                .facts()
                .await?
                .bands
                .get(specific)
                .is_some_and(|timeline| timeline.on(from).is_some());
            if !known {
                return Err(FillChangeError::UnknownSpecific {
                    specific: specific.to_string(),
                    date: from,
                });
            }
        }
        let change = FillChange {
            slot,
            intensity,
            from,
            recorded_at: at,
            exercise,
            assisted_by,
        };
        Ok(self.changes.record_change(found.id, &change).await?)
    }
}
