//! What a body is measured as, by the instruments that measure it.
//!
//! **One instrument, one entity**, for the reason [`crate::cycling::BikePlusRide`]
//! gives: a body-fat figure is inseparable from the scale and the algorithm that
//! produced it (§ 6), so an entity abstracting over scales would assert a
//! comparability that does not exist. A Withings Body Scan weigh-in is a
//! [`BodyScanWeighIn`]; a different scale is a different type, unbuilt.

pub mod hrv;
pub mod quantity;
pub mod weigh_in;

pub use hrv::{
    HrvBaseline, HrvReading, HrvStatus, InvalidWindow, LastNight, MeasurementWindow, OvernightHrv,
    OvernightHrvRecord, WeeklyStatus,
};
pub use quantity::{Age, KilocaloriesPerDay, PulseWaveVelocity, SkinConductance, VisceralFat};
pub use weigh_in::{
    BodyScanWeighIn, Composition, HeartReading, MeasuredBy, NerveReading, Rhythm, Segment,
    Segments, VascularReading, WeighInRecord,
};
