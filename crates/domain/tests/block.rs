//! The periodised block (research D11).
//!
//! Three inputs — a duration, a target repetition maximum and an entry test —
//! and every load in the block comes out of them. So these are table and
//! property tests: no performed record is involved, and none should be.
//!
//! **The numbers here are the ones in D11**, and they are asserted rather than
//! recomputed. A test that re-derives the derivation it is testing proves only
//! that the code is self-consistent.

use domain::gym::RepCount;
use domain::prescription::{
    Percentage,
    v2::{Block, InvalidBlock, Phase, WeekPlan},
};
use proptest::prelude::*;

// Helpers are free functions, so the test exemptions do not reach them and they
// may not panic.

fn reps(count: u32) -> Result<RepCount, Box<dyn std::error::Error>> {
    Ok(RepCount::new(count)?)
}

fn block(weeks: u32, target: u32) -> Result<Block, Box<dyn std::error::Error>> {
    Ok(Block::new(weeks, reps(target)?)?)
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

/// The eleven-week block, load for load.
///
/// The operator's autumn window. Five accumulation weeks and five of
/// intensification, from a duration and a target and nothing else.
#[test]
fn the_eleven_week_block_reproduces_its_table() {
    let Ok(block) = block(11, 3) else {
        panic!("eleven weeks is long enough for a block")
    };
    assert_eq!(block.accumulation_weeks(), 5);
    assert_eq!(block.intensification_weeks(), 5);

    let expected = [
        (0, 3, None),        // week  1  entry test
        (4, 6, Some(7_250)), // week  2  accumulation — four sets, not five:
        (4, 5, Some(7_500)), //          Prilepin's band caps the total lifts
        (5, 4, Some(7_750)),
        (5, 3, Some(8_000)),
        (5, 2, Some(8_250)), // accumulation exits at 82.5%, always
        (1, 7, Some(8_250)), // week  7  intensification opens where it left off
        (1, 6, Some(8_681)),
        (1, 5, Some(9_112)),
        (1, 4, Some(9_543)),
        (0, 3, None), // week 11  exit test — no load prescribed
    ];
    let got: Vec<_> = block.weeks().into_iter().map(row).collect();
    assert_eq!(got, expected);
}

/// The seven-week block: the same span in three rungs rather than five.
///
/// The minimum. Duration changes where the block starts and never where it
/// finishes, so the endpoint below is the eleven-week block's endpoint exactly.
#[test]
fn the_shortest_block_climbs_the_same_span_in_fewer_rungs() {
    let (Ok(short), Ok(long)) = (block(7, 3), block(11, 3)) else {
        panic!("both durations are plannable")
    };
    assert_eq!(short.accumulation_weeks(), 3);
    assert_eq!(short.intensification_weeks(), 3);

    let expected = [
        (0, 3, None),        // week 1  entry test
        (5, 4, Some(7_750)), // accumulation, three rungs rather than five
        (5, 3, Some(8_000)),
        (5, 2, Some(8_250)),
        (1, 5, Some(8_250)), // intensification opens at the same load
        (1, 4, Some(9_112)), // and climbs in bigger steps
        (0, 3, None),        // week 7  exit test
    ];
    let got: Vec<_> = short.weeks().into_iter().map(row).collect();
    assert_eq!(got, expected);
    assert_eq!(
        short.endpoint(),
        long.endpoint(),
        "duration moves the start, never the finish"
    );
}

/// The block plans a gain, and the gain is 5%.
///
/// The load each intensification week implies for the one-rep maximum climbs
/// past the entry test — 97.1, 99.2, 101.2, 103.2 — and the exit test is asked
/// for 105%. This is the assertion that would fail if the block were ever
/// re-anchored on the entry maximum: it would flatten at 100.0 and the whole
/// point of the phase would be gone.
#[test]
fn every_intensification_week_implies_a_bigger_maximum_than_the_last() {
    let Ok(block) = block(11, 3) else {
        panic!("eleven weeks is long enough for a block")
    };

    let mut implied = Vec::new();
    for plan in block.weeks() {
        match plan {
            WeekPlan::Working {
                phase: Phase::Intensification,
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
            WeekPlan::ExitTest { expected, .. } => {
                assert_eq!(
                    expected.as_basis_points(),
                    9_975,
                    "the exit test is asked for 105% of the entry maximum, \
                     expressed at three repetitions"
                );
            }
            WeekPlan::EntryTest { .. } | WeekPlan::Working { .. } => {}
        }
    }

    // Truncated rather than rounded, which is what integer basis points do.
    assert_eq!(implied, vec![970, 992, 1_012, 1_031]);
    assert!(
        implied.windows(2).all(|pair| pair[0] < pair[1]),
        "the maximum the plan implies only rises"
    );
}

/// Accumulation is where Prilepin's chart put it, at every rung.
///
/// The chart admits a band of total lifts per intensity zone. This asserts the
/// pairing rather than the loads — that each week's `sets × reps` falls in the
/// band for the load it carries — because that pairing is the reason the
/// three-in-reserve constant is not a preference.
///
/// Asserted over every plannable duration, not just the operator's: a longer
/// accumulation phase starts at a higher repetition count, and the highest
/// rungs are the ones the chart trims.
#[test]
fn every_accumulation_week_falls_in_its_prilepin_band() {
    for weeks in 7..=20 {
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

/// Six weeks cannot hold an entry test and three weeks of each phase.
#[test]
fn a_block_shorter_than_seven_weeks_is_refused() {
    let Ok(target) = reps(3) else {
        panic!("three is a repetition count")
    };
    assert_eq!(
        Block::new(6, target),
        Err(InvalidBlock::TooShort { weeks: 6 })
    );
    assert!(Block::new(7, target).is_ok());
}

fn a_block() -> impl Strategy<Value = Block> {
    (7_u32..=20, 1_u32..=8).prop_filter_map("a block its own rules accept", |(weeks, target)| {
        Block::new(weeks, RepCount::new(target).ok()?).ok()
    })
}

proptest! {
    /// However long the block and whatever it is for, the plan only ever gets
    /// heavier and the repetitions only ever descend within a phase.
    ///
    /// The property the whole structure exists to deliver, stated without
    /// reference to any particular duration — and the one a change to the
    /// interpolation is most likely to break silently.
    #[test]
    fn a_block_rises_and_its_repetitions_fall(block in a_block()) {
        let mut previous: Option<(Phase, u32, i32)> = None;
        for plan in block.weeks() {
            let WeekPlan::Working { phase, reps, load, .. } = plan else { continue };
            if let Some((was, was_reps, was_load)) = previous {
                if was == phase {
                    prop_assert!(reps.as_u32() < was_reps, "repetitions descend within a phase");
                    prop_assert!(load.as_basis_points() > was_load, "the load rises within a phase");
                } else {
                    prop_assert!(reps.as_u32() > was_reps, "the second phase restarts higher up");
                    prop_assert!(
                        load.as_basis_points() >= was_load,
                        "and never below where the first finished"
                    );
                }
            }
            previous = Some((phase, reps.as_u32(), load.as_basis_points()));
        }
    }

    /// The block opens and closes on a test, and both are at the target.
    #[test]
    fn a_block_is_bounded_by_two_tests_at_the_target(block in a_block()) {
        let weeks = block.weeks();
        let (Some(first), Some(last)) = (weeks.first(), weeks.last()) else {
            panic!("a block has weeks")
        };
        prop_assert_eq!(*first, WeekPlan::EntryTest { reps: block.target() });
        let exits_at_the_target =
            matches!(last, WeekPlan::ExitTest { reps, .. } if *reps == block.target());
        prop_assert!(exits_at_the_target);
        prop_assert_eq!(
            u32::try_from(weeks.len()).unwrap_or(u32::MAX),
            block.duration_weeks()
        );
    }

    /// Accumulation always exits at the same load, whatever the block.
    ///
    /// It is a double at three in reserve, and neither the duration nor the
    /// target reaches it. Worth pinning because it is the load intensification
    /// opens at, so a change here moves the start of the second phase too.
    #[test]
    fn accumulation_always_exits_at_the_same_load(block in a_block()) {
        // Week 1 is the entry test, so accumulation's last week is the one at
        // its own count plus one.
        let last = usize::try_from(block.accumulation_weeks()).unwrap_or(0);
        let Some(WeekPlan::Working { reps, load, .. }) = block.weeks().get(last).copied() else {
            panic!("accumulation has a last week")
        };
        prop_assert_eq!(reps.as_u32(), 2);
        prop_assert_eq!(load, Percentage::from_basis_points(8_250).unwrap_or(Percentage::WHOLE));
    }
}
