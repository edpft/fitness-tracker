//! What one Garmin activity's normalised layer holds: the gym session, where it
//! is one, and the devices its recording names (#569).
//!
//! **A sum rather than two derivations because the stream is one**, as
//! [`crate::spreadsheets::SpreadsheetEntity`] is: one activity can be a gym
//! session and a recording of which watch and sensors made it, and the stream
//! has one run log and one set of refusals.
//!
//! **Every activity's devices, not only the gym's.** A run or a ride is refused
//! as a gym session, and the watch that recorded it was still worn: which
//! device was in use when is what the register is built from.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    device::Sighting,
    gym::MeasuredGymSession,
    landing::{LandingRecordId, SourceRecordId},
    normalised::{NormalisedEntity, StatedZones},
    sequence::NonEmpty,
};

use super::{ActivityAccount, GarminGymTranslator, devices::sightings};

/// The devices one activity's recording names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityDevices {
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub sightings: NonEmpty<Sighting>,
}

impl NormalisedEntity for ActivityDevices {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

/// Either of what one activity derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GarminActivityEntity {
    /// Boxed for [`crate::spreadsheets::SpreadsheetEntity`]'s reason.
    GymSession(Box<MeasuredGymSession>),
    /// Boxed for the same reason: a recording's devices are a list of
    /// sightings, and the sum is only how one pass hands back both.
    Devices(Box<ActivityDevices>),
}

impl NormalisedEntity for GarminActivityEntity {
    fn composes(&self) -> Vec<&SourceRecordId> {
        match self {
            Self::GymSession(session) => session.composes(),
            Self::Devices(devices) => devices.composes(),
        }
    }
}

/// Garmin's adapter for its activities: the gym translator, and the devices
/// beside it.
#[derive(Debug, Clone, Default)]
pub struct GarminActivityTranslator {
    gym: GarminGymTranslator,
}

impl GarminActivityTranslator {
    pub const fn new(gym: GarminGymTranslator) -> Self {
        Self { gym }
    }
}

impl Translator for GarminActivityTranslator {
    type Account = ActivityAccount;
    type Entity = GarminActivityEntity;
    const VERSION: u32 = 1;

    /// **A refusal as a gym session stays a refusal**, beside the devices: the
    /// activity is still not a session this layer models.
    fn translate(
        &self,
        account: &ActivityAccount,
        zones: &StatedZones,
    ) -> Result<Translation<GarminActivityEntity>, NormalisationError> {
        let gym = self.gym.translate(account, zones)?;
        let record = account.activity();
        let devices = NonEmpty::new(sightings(account, zones))
            .ok()
            .map(|sightings| {
                GarminActivityEntity::Devices(Box::new(ActivityDevices {
                    landed_as: record.id(),
                    source_record_id: record.source_record_id().clone(),
                    sightings,
                }))
            });

        Ok(match (gym, devices) {
            (Translation::Retraction { of }, _) => Translation::Retraction { of },
            (Translation::Entity { entity, refusals }, devices) => Translation::Entities {
                entities: NonEmpty::of(
                    GarminActivityEntity::GymSession(entity),
                    devices.into_iter().collect(),
                ),
                refusals,
            },
            (Translation::Entities { entities, refusals }, devices) => {
                let first = GarminActivityEntity::GymSession(Box::new(entities.first().clone()));
                let rest = entities
                    .into_vec()
                    .into_iter()
                    .skip(1)
                    .map(|session| GarminActivityEntity::GymSession(Box::new(session)))
                    .chain(devices)
                    .collect();
                Translation::Entities {
                    entities: NonEmpty::of(first, rest),
                    refusals,
                }
            }
            (Translation::Refused(refusals), None) => Translation::Refused(refusals),
            (Translation::Refused(refusals), Some(devices)) => Translation::Entities {
                entities: NonEmpty::of(devices, Vec::new()),
                refusals: refusals.into_vec(),
            },
        })
    }
}
