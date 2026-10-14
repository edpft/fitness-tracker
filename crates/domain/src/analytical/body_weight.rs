//! Body weight as one time series (#250).
//!
//! **Every canonical weigh-in, once, in date order.** Body mass is
//! source-independent (§ 6), so the scale's readings and the ones typed into
//! a spreadsheet or MyFitnessPal are one series, and the canonical layer has
//! already made a weighing recorded twice one weigh-in (#409).
//!
//! **Ordered here, by the operator's day and then by the clock where one was
//! read**, so the series does not depend on the order a store returned rows in
//! (§ 9).
//!
//! **Nothing is filled in** (§ 37). A day with no weigh-in has no point, so a
//! gap in the record is a gap in the series.

use super::Weighed;

/// The body-weight series the canonical weigh-ins give, oldest first.
pub fn body_weight(mut weigh_ins: Vec<Weighed>) -> Vec<Weighed> {
    weigh_ins.sort_by_key(|weighed| {
        (
            weighed.at.day(),
            weighed.at.instant().map(|at| at.instant().as_second()),
        )
    });
    weigh_ins
}
