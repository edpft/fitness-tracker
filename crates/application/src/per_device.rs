//! Overnight HRV and resting heart rate, one series per device (#254).
//!
//! **The attribution is `domain`'s** ([`per_device`]); this reads the figures
//! and the register and hands them over. Nothing is written: every series
//! re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        hrv::Trended,
        per_device::{DeviceSeries, per_device},
    },
    device::Measurand,
    measure::{BeatsPerMinute, HeartRateVariability},
};

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    ports::{OvernightHrvHistory, RecordedDevices, RestingHeartRateHistory, WearRecord},
};

/// The per-device use case.
pub struct PerDevice<N, D, R, W> {
    nights: N,
    days: D,
    devices: DeviceRegister<R, W>,
}

impl<N, D, R, W> PerDevice<N, D, R, W>
where
    N: OvernightHrvHistory + Sync,
    D: RestingHeartRateHistory + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(nights: N, days: D, devices: DeviceRegister<R, W>) -> Self {
        Self {
            nights,
            days,
            devices,
        }
    }

    /// Last night's average HRV, by morning, one series per watch.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn overnight_hrv(
        &self,
    ) -> Result<Vec<DeviceSeries<HeartRateVariability>>, StoreError> {
        let register = self.devices.register().await?;
        Ok(per_device(
            self.nights.nights().await?,
            Measurand::HeartRate,
            &register,
        ))
    }

    /// Last night's average HRV, one series per watch, and the trend through
    /// each (#417).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn overnight_hrv_trend(&self) -> Result<Vec<Trended>, StoreError> {
        Ok(self
            .overnight_hrv()
            .await?
            .into_iter()
            .map(Trended::of)
            .collect())
    }

    /// Each day's resting heart rate, one series per watch.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn resting_heart_rate(
        &self,
    ) -> Result<Vec<DeviceSeries<BeatsPerMinute>>, StoreError> {
        let register = self.devices.register().await?;
        Ok(per_device(
            self.days.days().await?,
            Measurand::HeartRate,
            &register,
        ))
    }
}
