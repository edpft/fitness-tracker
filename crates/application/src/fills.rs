//! Changing a slot's fill partway through a mesocycle (#454).
//!
//! The change holds from a date, so a session prescribed before it keeps
//! what it was prescribed; and it names what assists the exercise, which must
//! already be known equipment.

use domain::{
    gym::exercise::Implement,
    prescription::{FillChange, SlotId, linear::SlotContent},
    schedule::SessionRole,
};
use jiff::civil::Date;

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

    /// Record a change: a slot filled with an exercise from a date, assisted
    /// by a specific or not, and loaded above bodyweight by a kind held or
    /// not.
    ///
    /// # Errors
    ///
    /// [`FillChangeError`] if no mesocycle is in force then, the slot is
    /// static or the primary, the range of bands is not known on that date,
    /// or what loads it is named for an exercise that is not bodyweight.
    pub async fn change(&self, change: FillChange) -> Result<(), FillChangeError> {
        let FillChange {
            slot,
            from,
            exercise,
            ref assisted_by,
            loaded_by,
            ..
        } = change;
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
        if let Some(specific) = assisted_by {
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
        // An exercise on its own implement is loaded by it already.
        if loaded_by.is_some() && exercise.implement() != Implement::Bodyweight {
            return Err(FillChangeError::NotBodyweight {
                exercise: exercise.name(),
            });
        }
        Ok(self.changes.record_change(found.id, &change).await?)
    }
}
