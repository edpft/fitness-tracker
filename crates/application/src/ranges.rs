//! The ranges a chart can be read over, on a day (#252, #497).
//!
//! **Worked out once, here, for every chart**: the mesocycle and the
//! macrocycle in force are the plans', and which days they start on is
//! `domain`'s ([`Ranges::on`]). Nothing is written.

use domain::{analytical::range::Ranges, plan::Plan};
use jiff::civil::Date;

/// The mesocycle and macrocycle `today` falls in among `plans`, as they now
/// stand, each to `today`.
#[must_use]
pub fn of(today: Date, plans: &[Plan]) -> Ranges {
    let mesocycles: Vec<_> = plans.iter().flat_map(Plan::mesocycle_spans).collect();
    let spans: Vec<_> = plans.iter().map(Plan::span).collect();
    Ranges::on(today, &mesocycles, &spans)
}
