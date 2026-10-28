//! A night's sleep, how long and when, one series per watch (#255).
//!
//! **The attribution is `domain`'s** ([`per_device`]), on the watch each
//! night's own record names, as [`crate::per_device`]'s are. Nothing is
//! written: every series re-derives from the nights and the register (§ 5).

use domain::{
    analytical::{
        per_device::{DeviceSeries, per_device},
        sleep::Night,
    },
    device::Measurand,
};

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    ports::{RecordedDevices, SleepHistory, WearRecord},
};

/// The sleep use case.
pub struct Sleeping<S, R, W> {
    nights: S,
    devices: DeviceRegister<R, W>,
}

impl<S, R, W> Sleeping<S, R, W>
where
    S: SleepHistory + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(nights: S, devices: DeviceRegister<R, W>) -> Self {
        Self { nights, devices }
    }

    /// Every night's sleep, by morning, one series per watch.
    ///
    /// **Placed as a heart rate is**: the wrist tells sleep from waking by its
    /// heart rate and its movement, so a watch that measured no heart rate
    /// cannot have recorded one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn nights(&self) -> Result<Vec<DeviceSeries<Night>>, StoreError> {
        let register = self.devices.register().await?;
        Ok(per_device(
            self.nights.nights().await?,
            Measurand::HeartRate,
            &register,
        ))
    }
}
