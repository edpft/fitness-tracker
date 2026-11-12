//! The planned macrocycle: every mesocycle of the term, numbered by its place
//! in it, and each planned, in progress or completed by where the microcycle
//! walk stands (#789).
//!
//! Here because the mesocycle fixtures are.

use std::num::NonZeroU8;

use domain::{
    macrocycle::{Macrocycle, MesocycleState, PlannedMacrocycle},
    schedule::{Holidays, PublicHoliday, SchoolHoliday},
};
use jiff::civil::{Date, date};

use crate::support::mesocycle::{as_mesocycle, entry_test_from, linear_from, named_plan};

/// Autumn 2026: the summer holiday before it and the Christmas holiday after.
fn autumn() -> Option<Macrocycle> {
    let holiday = |first: Date, last: Date| {
        let days = (last - first).get_days().checked_add(1)?;
        Some(SchoolHoliday::new(
            first,
            NonZeroU8::new(u8::try_from(days).ok()?)?,
        ))
    };
    let school = Holidays::from_school(
        vec![
            holiday(date(2026, 7, 21), date(2026, 9, 5))?,
            holiday(date(2026, 12, 21), date(2027, 1, 3))?,
        ],
        Some(date(2027, 1, 3)),
    );
    let public = Holidays::from_public(
        vec![
            PublicHoliday::new(date(2026, 8, 31), "Summer bank holiday".to_owned())
                .naming(domain::schedule::SchoolHolidayKind::Summer),
            PublicHoliday::new(date(2026, 12, 25), "Christmas Day".to_owned()),
        ],
        Some(date(2027, 12, 31)),
    );
    Macrocycle::on(date(2026, 9, 14), &school.and(public))
}

/// The entry test from 14 September, then a block from the week after.
fn planned(commencing: Date) -> Option<PlannedMacrocycle> {
    let plan = named_plan(
        "2026-autumn",
        vec![
            entry_test_from(date(2026, 9, 14)).ok()?,
            as_mesocycle(linear_from(date(2026, 9, 21)).ok()?),
        ],
    )
    .ok()?;
    Some(PlannedMacrocycle::new(autumn()?, plan, commencing))
}

fn states(planned: &PlannedMacrocycle) -> Vec<(u32, Date, MesocycleState)> {
    planned
        .mesocycles()
        .iter()
        .map(|mesocycle| {
            (
                mesocycle.ordinal().as_u32(),
                mesocycle.span().start(),
                mesocycle.state(),
            )
        })
        .collect()
}

/// **The block is in progress while one of its microcycles is current**, and
/// the entry test before it is completed.
#[test]
fn the_block_is_in_progress_after_the_entry_test() {
    let planned = planned(date(2026, 10, 12)).expect("the plan builds");

    assert_eq!(
        states(&planned),
        vec![
            (1, date(2026, 9, 14), MesocycleState::Completed),
            (2, date(2026, 9, 21), MesocycleState::InProgress),
        ]
    );
    assert_eq!(
        planned.state_on(date(2026, 9, 28)),
        Some(MesocycleState::InProgress)
    );
}

/// **Gated**: while the entry test runs, the block after it is planned.
#[test]
fn the_block_is_planned_while_the_entry_test_runs() {
    let planned = planned(date(2026, 9, 14)).expect("the plan builds");

    assert_eq!(
        states(&planned),
        vec![
            (1, date(2026, 9, 14), MesocycleState::InProgress),
            (2, date(2026, 9, 21), MesocycleState::Planned),
        ]
    );
}
