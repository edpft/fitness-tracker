//! The periodised block (research D11 and D12).
//!
//! Three inputs — a duration in phase weeks, the repetition count of the entry
//! test, and the entry test itself — and every load in the block comes out of
//! them. So these are table and property tests: no performed record is involved,
//! and none should be.
//!
//! **The numbers here are the ones in D12**, and they are asserted rather than
//! recomputed. A test that re-derives the derivation it is testing proves only
//! that the code is self-consistent.
//!
//! **There is no RIR in any of them**, which is the point of D12's correction: a
//! percentage-based plan states percentages, and the accumulation loads below are
//! pinned by Prilepin's repetitions-per-set column rather than by a reserve.

use domain::gym::RepCount;
use domain::prescription::{
    Percentage,
    block::{Block, InvalidBlock, Phase, WeekPlan},
};
use proptest::prelude::*;

// Helpers are free functions, so the test exemptions do not reach them and they
// may not panic.

fn reps(count: u32) -> Result<RepCount, Box<dyn std::error::Error>> {
    Ok(RepCount::new(count)?)
}

fn block(weeks: u32, entry_reps: u32) -> Result<Block, Box<dyn std::error::Error>> {
    Ok(Block::new(weeks, reps(entry_reps)?)?)
}

/// A week as `(sets, reps, percentage in basis points)`, for comparing a whole
/// block in one assertion.
const fn row(plan: WeekPlan) -> (u32, u32, Option<i32>) {
    match plan {
        // A test carries no load, and both of them are a test.
        WeekPlan::EntryTest { reps } | WeekPlan::ExitTest { reps, .. } => (0, reps.as_u32(), None),
        WeekPlan::Working {
            sets, reps, load, ..
        } => (sets.as_u32(), reps.as_u32(), Some(load.as_basis_points())),
    }
}

/// The operator's four rows, and the fifth the rule gives for free.
///
/// The split is one rule rather than a table: 3-3-2 at the floor, and each week
/// beyond it to accumulation, then intensification, then realisation, in
/// rotation. These are the durations the operator's research states, plus the
/// twelve-week case it did not.
#[test]
fn the_split_reproduces_the_stated_table() {
    let expected = [
        (8, (3, 3, 2)),
        (9, (4, 3, 2)),
        (10, (4, 4, 2)),
        (11, (4, 4, 3)),
        (12, (5, 4, 3)),
    ];
    for (weeks, split) in expected {
        let Ok(block) = block(weeks, 3) else {
            panic!("{weeks} weeks is plannable")
        };
        assert_eq!(
            (
                block.accumulation_weeks(),
                block.intensification_weeks(),
                block.realisation_weeks()
            ),
            split,
            "{weeks} weeks"
        );
        assert_eq!(
            block.duration_weeks(),
            weeks,
            "the split spends the duration"
        );
        assert_eq!(
            block.total_weeks(),
            weeks + 1,
            "and the entry test is the week the calendar carries on top"
        );
    }
}

/// The ten-week block, load for load.
///
/// The operator's autumn window: a 3RM entry test the week before, then ten weeks
/// of phases as four, four and two. Every number below comes from the duration
/// and the three literature constants.
#[test]
fn the_ten_week_block_reproduces_its_table() {
    let Ok(block) = block(10, 3) else {
        panic!("ten weeks is long enough for a block")
    };

    let expected = [
        (0, 3, None),        // entry test, the week before the block opens
        (4, 5, Some(7_250)), // accumulation — four sets, not five: Prilepin's
        (5, 4, Some(7_500)), //                band caps the total lifts
        (5, 3, Some(7_750)),
        (5, 2, Some(8_000)), // accumulation exits pinned at Prilepin's 80%, always
        (1, 6, Some(8_000)), // the top set opens where accumulation left off
        (1, 5, Some(8_500)), // intensification
        (1, 4, Some(9_000)),
        (1, 3, Some(9_500)),
        (1, 2, Some(10_000)), // realisation
        (0, 1, None),         // exit test — a single, and no load prescribed
    ];
    let got: Vec<_> = block.weeks().into_iter().map(row).collect();
    assert_eq!(got, expected);
}

/// The eight-week block: the same span in fewer, larger rungs.
///
/// The minimum. Duration changes where the block starts and never where it
/// finishes, so the endpoint below is the ten-week block's endpoint exactly.
#[test]
fn the_shortest_block_climbs_the_same_span_in_fewer_rungs() {
    let (Ok(short), Ok(long)) = (block(8, 3), block(10, 3)) else {
        panic!("both durations are plannable")
    };

    let expected = [
        (0, 3, None),        // entry test
        (5, 4, Some(7_500)), // accumulation, three rungs rather than four
        (5, 3, Some(7_750)),
        (5, 2, Some(8_000)),
        (1, 5, Some(8_000)), // the top set opens at the same load
        (1, 4, Some(8_625)), // and climbs in bigger steps
        (1, 3, Some(9_250)),
        (1, 2, Some(9_875)), // realisation
        (0, 1, None),        // exit test
    ];
    let got: Vec<_> = short.weeks().into_iter().map(row).collect();
    assert_eq!(got, expected);
    assert_eq!(
        short.endpoint(),
        long.endpoint(),
        "duration moves the start, never the finish"
    );
}

/// The block plans a gain, and the gain is 5% of the entry one-rep maximum.
///
/// The maximum each top-set week implies climbs past the entry test — 91.4, 94.4,
/// 97.2, 100.0, 102.5 — and the exit test is asked for 105% at a single, so the
/// plan is read in the unit it was planned in. This is the assertion that would
/// fail if the block were ever re-anchored on the entry maximum: it would flatten
/// at 100.0 and the whole point of the phase would be gone.
#[test]
fn every_top_set_week_implies_a_bigger_maximum_than_the_last() {
    let Ok(block) = block(10, 3) else {
        panic!("ten weeks is long enough for a block")
    };

    let mut implied = Vec::new();
    for plan in block.weeks() {
        match plan {
            WeekPlan::Working {
                phase: Phase::Intensification | Phase::Realisation,
                reps,
                load,
                ..
            } => {
                let Some(maximum) = domain::prescription::rep_max(reps) else {
                    panic!("{reps:?} sits on the table")
                };
                // Basis points of the entry maximum, to one decimal place.
                implied.push(
                    i64::from(load.as_basis_points()) * 1_000
                        / i64::from(maximum.as_basis_points()),
                );
            }
            WeekPlan::ExitTest { reps, expected } => {
                assert_eq!(reps.as_u32(), 1, "the block finishes on a single");
                assert_eq!(
                    expected.as_basis_points(),
                    10_500,
                    "the exit test is asked for 105% of the entry one-rep maximum"
                );
            }
            WeekPlan::EntryTest { .. } | WeekPlan::Working { .. } => {}
        }
    }

    // Truncated rather than rounded, which is what integer basis points do.
    assert_eq!(implied, vec![914, 944, 972, 1_000, 1_025]);
    assert!(
        implied.windows(2).all(|pair| pair[0] < pair[1]),
        "the maximum the plan implies only rises"
    );
}

/// Accumulation is where Prilepin's chart put it, at every rung.
///
/// The chart admits a band of total lifts per intensity zone. This asserts the
/// pairing rather than the loads — that each week's `sets × reps` falls in the
/// band for the load it carries — because that pairing is what replaced the
/// repetitions-in-reserve constant the plan used to place this phase with.
///
/// Asserted over every plannable duration, not just the operator's: a longer
/// accumulation phase starts at a higher repetition count, and the highest rungs
/// are the ones the chart trims.
#[test]
fn every_accumulation_week_falls_in_its_prilepin_band() {
    for weeks in 8..=15 {
        let Ok(block) = block(weeks, 3) else {
            panic!("{weeks} weeks is plannable")
        };
        assert_in_band(block);
    }
}

fn assert_in_band(block: Block) {
    for plan in block.weeks() {
        let WeekPlan::Working {
            phase: Phase::Accumulation,
            sets,
            reps,
            load,
        } = plan
        else {
            continue;
        };
        let lifts = sets.as_u32() * reps.as_u32();
        let band = match load.as_basis_points() {
            ..7_000 => (18, 30),
            7_000..8_000 => (12, 24),
            8_000..9_000 => (10, 20),
            _ => (4, 10),
        };
        assert!(
            (band.0..=band.1).contains(&lifts),
            "{lifts} lifts at {load} is outside Prilepin's {band:?}"
        );
    }
}

/// Seven weeks cannot hold three weeks of each of the first two phases and two
/// of realisation.
#[test]
fn a_block_shorter_than_eight_weeks_is_refused() {
    let Ok(entry) = reps(3) else {
        panic!("three is a repetition count")
    };
    assert_eq!(
        Block::new(7, entry),
        Err(InvalidBlock::TooShort { weeks: 7 })
    );
    assert!(Block::new(8, entry).is_ok());
}

/// And sixteen weeks is too long, for a reason nobody authored.
///
/// The top set opens at the load accumulation finished on, at a repetition count
/// equal to the phases it spans. At fifteen weeks that is a set of nine at 80%,
/// which is exactly the maximum for nine repetitions. At sixteen it is a set of
/// ten at 80%, which is heavier than a ten-rep maximum — not a hard set but an
/// impossible one. The bound falls out of the table.
#[test]
fn a_block_whose_top_set_could_not_be_lifted_is_refused() {
    let Ok(entry) = reps(3) else {
        panic!("three is a repetition count")
    };
    assert!(Block::new(15, entry).is_ok());
    assert_eq!(
        Block::new(16, entry),
        Err(InvalidBlock::TooLong { weeks: 16 })
    );
}

/// An entry test at a repetition count the table cannot convert is refused.
#[test]
fn an_entry_test_off_the_table_is_refused() {
    let Ok(entry) = reps(41) else {
        panic!("forty-one is a repetition count, just not a useful one")
    };
    assert_eq!(
        Block::new(10, entry),
        Err(InvalidBlock::EntryTestTooLong { reps: 41 })
    );
}

fn a_block() -> impl Strategy<Value = Block> {
    (8_u32..=15, 1_u32..=8).prop_filter_map("a block its own rules accept", |(weeks, entry)| {
        Block::new(weeks, RepCount::new(entry).ok()?).ok()
    })
}

/// A working week as the properties below read it.
const fn working(plan: WeekPlan) -> Option<(bool, u32, i32)> {
    match plan {
        WeekPlan::Working {
            phase, reps, load, ..
        } => Some((
            matches!(phase, Phase::Accumulation),
            reps.as_u32(),
            load.as_basis_points(),
        )),
        WeekPlan::EntryTest { .. } | WeekPlan::ExitTest { .. } => None,
    }
}

proptest! {
    /// However long the block, the plan only gets heavier and the repetitions
    /// only descend — through accumulation, and then through intensification and
    /// realisation as one ladder.
    ///
    /// **Intensification and realisation are one ladder deliberately**, which is
    /// why this groups them: a break at that boundary would be a number somebody
    /// chose. The only discontinuity in the block is the wave at accumulation's
    /// end, where the repetitions jump back up and the load stands still.
    #[test]
    fn a_block_rises_and_its_repetitions_fall(block in a_block()) {
        let mut previous: Option<(bool, u32, i32)> = None;
        for (accumulating, reps, load) in block.weeks().into_iter().filter_map(working) {
            if let Some((was_accumulating, was_reps, was_load)) = previous {
                if was_accumulating == accumulating {
                    prop_assert!(reps < was_reps, "repetitions descend along a ladder");
                    prop_assert!(load > was_load, "and the load rises along it");
                } else {
                    prop_assert!(reps > was_reps, "the top set restarts higher up");
                    prop_assert_eq!(
                        load, was_load,
                        "at exactly the load accumulation finished on"
                    );
                }
            }
            previous = Some((accumulating, reps, load));
        }
    }

    /// The block opens on a test at the entry repetition count and closes on a
    /// single, whatever it opened on.
    #[test]
    fn a_block_opens_on_its_entry_test_and_closes_on_a_single(block in a_block()) {
        let weeks = block.weeks();
        let (Some(first), Some(last)) = (weeks.first(), weeks.last()) else {
            panic!("a block has weeks")
        };
        prop_assert_eq!(*first, WeekPlan::EntryTest { reps: block.entry_reps() });
        let exits_on_a_single = matches!(last, WeekPlan::ExitTest { reps, .. } if reps.as_u32() == 1);
        prop_assert!(exits_on_a_single);
        prop_assert_eq!(
            u32::try_from(weeks.len()).unwrap_or(u32::MAX),
            block.total_weeks()
        );
    }

    /// Accumulation always exits at 80%, whatever the block.
    ///
    /// It is a double, and Prilepin's chart is what puts a double at 80% — below
    /// it the chart asks for sets of three to six. Neither the duration nor the
    /// entry test reaches this. Worth pinning because it is the load the top set
    /// opens at, so a change here moves the second half of the block too.
    #[test]
    fn accumulation_always_exits_at_prilepins_floor_for_a_double(block in a_block()) {
        // The entry test takes the first week, so accumulation's last week is the
        // one at its own count plus one.
        let last = usize::try_from(block.accumulation_weeks()).unwrap_or(0);
        let Some(WeekPlan::Working { reps, load, .. }) = block.weeks().get(last).copied() else {
            panic!("accumulation has a last week")
        };
        prop_assert_eq!(reps.as_u32(), 2);
        prop_assert_eq!(load, Percentage::from_basis_points(8_000).unwrap_or(Percentage::WHOLE));
    }
}
