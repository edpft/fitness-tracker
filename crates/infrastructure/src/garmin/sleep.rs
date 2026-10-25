//! Which watch Garmin's sleep record names for a night (#569).
//!
//! **Garmin's HRV and daily heart rate name no device; its sleep record does**,
//! for the same date. `dailySleepDTO.sleepNeed.deviceId` is the one field there
//! on every night since 2024-12-22, and on every night it agrees with the `SpO2`
//! summary's and the daily summary's where they state one. It is the watch's
//! unit id, which is its serial number.

use domain::{
    device::{DeviceIdentifier, SerialNumber},
    landing::LandedRecord,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Sleep {
    #[serde(rename = "dailySleepDTO")]
    daily: Option<Daily>,
}

#[derive(Debug, Deserialize)]
struct Daily {
    #[serde(rename = "sleepNeed")]
    need: Option<Need>,
}

#[derive(Debug, Deserialize)]
struct Need {
    #[serde(rename = "deviceId")]
    device: Option<u64>,
}

/// The watch the record names, or [`None`] where it names none or will not
/// read: no evidence about any device.
#[must_use]
pub fn recorded_by(sleep: &LandedRecord) -> Option<DeviceIdentifier> {
    let served: Sleep = serde_json::from_slice(sleep.payload().as_bytes()).ok()?;
    let device = served.daily?.need?.device?;
    SerialNumber::try_from(device.to_string())
        .ok()
        .map(DeviceIdentifier::Serial)
}
