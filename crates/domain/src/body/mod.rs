//! What a body is measured as, by the instruments that measure it.
//!
//! **Mass is the same fact whatever recorded it; composition is not.** Body
//! mass is source-independent (§ 6), so every weigh-in's mass belongs in one
//! series, whether a Body Scan stamped it or the operator typed it into a
//! spreadsheet. A body-fat figure is inseparable from the scale and the
//! algorithm that produced it, so composition never joins another source's
//! series, and it is always read against the mass from the same step on the
//! same scale (operator, 2026-09-27, #273).

pub mod hrv;
pub mod manual;
pub mod quantity;
pub mod weigh_in;

pub use hrv::{
    HrvBaseline, HrvReading, HrvStatus, InvalidWindow, LastNight, MeasurementWindow, OvernightHrv,
    OvernightHrvRecord, WeeklyStatus,
};
pub use manual::{
    Cell, CellRef, InvalidCellRef, InvalidSheetName, InvalidWeighIn, ManualWeighIn, SheetName,
};
pub use quantity::{Age, KilocaloriesPerDay, PulseWaveVelocity, SkinConductance, VisceralFat};
pub use weigh_in::{
    BodyScanWeighIn, Composition, HeartReading, MeasuredBy, NerveReading, Rhythm, Segment,
    Segments, VascularReading, WeighInRecord,
};
