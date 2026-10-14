//! Which device sent a reading that arrives without naming one (#427).
//!
//! **The register is `domain`'s** ([`register`]); this reads what every
//! source's record says about the devices it used, and which days a watch was
//! worn, and hands both over. Nothing is written: the register re-derives from
//! what has landed exactly (§ 7).

use domain::device::{Register, register};

use crate::{
    error::StoreError,
    ports::{RecordedDevices, WearRecord},
};

/// The register use case.
pub struct DeviceRegister<R, W> {
    recorded: R,
    wear: W,
}

impl<R, W> DeviceRegister<R, W>
where
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(recorded: R, wear: W) -> Self {
        Self { recorded, wear }
    }

    /// Every device the record identifies, and the days a watch was worn.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn register(&self) -> Result<Register, StoreError> {
        Ok(register(
            &self.recorded.sightings().await?,
            self.wear.wear().await?,
        ))
    }
}
