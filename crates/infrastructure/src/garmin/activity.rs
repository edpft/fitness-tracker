//! What one Garmin activity's normalised layer holds: the gym session or the
//! padel game, where it is one, and the devices its recording names (#569,
//! #517).
//!
//! **A sum rather than separate derivations because the stream is one**, as
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
    normalised::{NormalisedEntity, RefusalLocus, StatedZones},
    padel::PadelGame,
    sequence::NonEmpty,
};

use crate::scribe::Scribe;

use super::{ActivityAccount, GarminGymTranslator, devices::sightings, padel};

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
    /// A game of padel, which is not a gym session and is not refused as one
    /// (#517).
    PadelGame(Box<PadelGame>),
    /// Boxed for the same reason: a recording's devices are a list of
    /// sightings, and the sum is only how one pass hands back both.
    Devices(Box<ActivityDevices>),
}

impl NormalisedEntity for GarminActivityEntity {
    fn composes(&self) -> Vec<&SourceRecordId> {
        match self {
            Self::GymSession(session) => session.composes(),
            Self::PadelGame(game) => game.composes(),
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

    /// The devices the activity's recording names, where it names any.
    fn devices(account: &ActivityAccount, zones: &StatedZones) -> Option<GarminActivityEntity> {
        let record = account.activity();
        NonEmpty::new(sightings(account, zones))
            .ok()
            .map(|sightings| {
                GarminActivityEntity::Devices(Box::new(ActivityDevices {
                    landed_as: record.id(),
                    source_record_id: record.source_record_id().clone(),
                    sightings,
                }))
            })
    }
}

impl Translator for GarminActivityTranslator {
    type Account = ActivityAccount;
    type Entity = GarminActivityEntity;
    /// 2: a padel game is a padel game, where 1 refused it as unmodelled
    /// (#517).
    const VERSION: u32 = 2;

    /// **A refusal as a gym session stays a refusal**, beside the devices: the
    /// activity is still not a session this layer models.
    fn translate(
        &self,
        account: &ActivityAccount,
        zones: &StatedZones,
    ) -> Result<Translation<GarminActivityEntity>, NormalisationError> {
        let gym = self.gym.translate(account, zones)?;
        let record = account.activity();
        // **Only where the gym refused it**: a retraction is still one, and
        // nothing is both a gym session and a game.
        let played = matches!(gym, Translation::Refused(_))
            .then(|| padel::game(record, zones))
            .flatten();
        let gym = match played {
            Some(Ok(game)) => {
                return Ok(Translation::Entities {
                    entities: NonEmpty::of(
                        GarminActivityEntity::PadelGame(Box::new(game)),
                        Self::devices(account, zones).into_iter().collect(),
                    ),
                    refusals: Vec::new(),
                });
            }
            Some(Err(reason)) => Scribe::new(record).only(RefusalLocus::Record, reason),
            None => gym,
        };
        let devices = Self::devices(account, zones);

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
