//! The plan (T028, covering US3 scenarios 1 to 4).
//!
//! No performed record is involved in any of this. The plan is a total function
//! of a duration, a starting 1RM and an authored span — which is the whole point
//! of separating it from the failure mechanism, and is why these are table and
//! property tests rather than integration ones.

use domain::gym::Kg;
use domain::prescription::{Ladder, Percentage, PlateIncrement, WeekIndex};
use proptest::prelude::*;

fn grid() -> Result<PlateIncrement, Box<dyn std::error::Error>> {
    Ok(PlateIncrement::new(Kg::try_from("2.5".to_owned())?)?)
}

fn pct(value: &str) -> Result<Percentage, Box<dyn std::error::Error>> {
    Ok(Percentage::try_from(value.to_owned())?)
}

fn kg(value: &str) -> Result<Kg, Box<dyn std::error::Error>> {
    Ok(Kg::try_from(value.to_owned())?)
}

fn week(index: u32) -> Result<WeekIndex, Box<dyn std::error::Error>> {
    Ok(WeekIndex::new(index)?)
}

/// US3-1: two inputs generate the block, and the last week is the test.
#[test]
fn a_duration_and_an_anchor_generate_every_week() {
    let (Ok(start), Ok(end), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), pct("105%"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    // Seven weeks: six climbing, then the test.
    let Ok(ladder) = Ladder::new(start, end, 7) else {
        panic!("a rising span over seven weeks is a ladder")
    };

    assert_eq!(ladder.climbing_weeks(), 6);

    for index in 1..=6 {
        let Ok(w) = week(index) else {
            panic!("weeks are one-based")
        };
        assert!(
            ladder.heavy_top_set(anchor, w, increment).is_some(),
            "week {index} is a climbing week and has a load"
        );
    }

    // The seventh week is the test. It is not a ladder position and has no
    // percentage, which the type says rather than a caller remembering.
    let Ok(past_the_climb) = week(7) else {
        panic!("weeks are one-based")
    };
    assert!(ladder.percentage(past_the_climb).is_none());
    assert!(
        ladder
            .heavy_top_set(anchor, past_the_climb, increment)
            .is_none()
    );
}

/// One worked ladder, load for load.
///
/// An example and nothing more. These particular numbers advance by one plate
/// each week because a 90kg anchor happens to sit near 100, so a
/// 2.5-percentage-point step is 2.25kg — which is a coincidence of the example
/// and not a property the ladder has or should have.
#[test]
fn the_worked_ladder_reproduces_its_table() {
    let (Ok(start), Ok(end), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), pct("105%"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    let Ok(ladder) = Ladder::new(start, end, 7) else {
        panic!("a rising span over seven weeks is a ladder")
    };

    let expected = ["82.5", "85", "87.5", "90", "92.5", "95"];
    for (offset, want) in expected.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(0) + 1;
        let (Ok(w), Ok(want_kg)) = (week(index), kg(want)) else {
            panic!("week {index} and {want} are both valid")
        };
        let Some(got) = ladder.heavy_top_set(anchor, w, increment) else {
            panic!("week {index} is a climbing week")
        };
        assert_eq!(got, want_kg, "week {index}");
    }
}

/// The light session tracks the heavy one at the authored proportion.
///
/// **It deliberately does not reproduce the record.** The three validated weeks
/// ran 72.5/75/77.5 light against 82.5/85/87.5 heavy — a flat −10kg every time.
/// An earlier version of this test asserted that, through an 88.5% fitted to it;
/// the operator has since stated 85%, which is one plate lighter and is a
/// decision rather than a curve through three points. The divergence from the
/// record is attributable to a parameter that was different when those sessions
/// were trained by hand, which is exactly the bucket SC-002 asks divergences to
/// fall into.
#[test]
fn the_light_session_tracks_the_heavy_one() {
    let (Ok(start), Ok(end), Ok(increment), Ok(anchor), Ok(light_of_heavy)) =
        (pct("92.5%"), pct("105%"), grid(), kg("90"), pct("85%"))
    else {
        panic!("the fixture values are all valid")
    };
    let Ok(ladder) = Ladder::new(start, end, 7) else {
        panic!("a rising span over seven weeks is a ladder")
    };

    // 85% of the heavy weeks 82.5, 85 and 87.5, on the 2.5kg grid.
    let expected = ["70", "72.5", "75"];
    for (offset, want) in expected.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(0) + 1;
        let (Ok(w), Ok(want_kg)) = (week(index), kg(want)) else {
            panic!("week {index} and {want} are both valid")
        };
        let Some(got) = ladder.light_top_set(anchor, w, increment, light_of_heavy) else {
            panic!("week {index} is a climbing week")
        };
        assert_eq!(got, want_kg, "light week {index}");
    }
}

/// US3-3: the step derives from the endpoint, so changing the duration changes
/// the step and not where the block finishes.
///
/// If someone later authors a step and derives the endpoint, this is what fails.
#[test]
fn the_step_derives_from_the_endpoint_not_the_reverse() {
    let (Ok(start), Ok(end)) = (pct("92.5%"), pct("105%")) else {
        panic!("the span is valid")
    };
    let (Ok(short), Ok(long)) = (Ladder::new(start, end, 7), Ladder::new(start, end, 13)) else {
        panic!("both durations make a ladder")
    };

    // Same endpoint, whatever the duration.
    let (Ok(short_last), Ok(long_last)) = (week(6), week(12)) else {
        panic!("weeks are one-based")
    };
    assert_eq!(short.percentage(short_last), Some(end));
    assert_eq!(long.percentage(long_last), Some(end));

    // Different step, because the same span is spread over more weeks.
    let Ok(second) = week(2) else {
        panic!("weeks are one-based")
    };
    let (Some(short_second), Some(long_second)) =
        (short.percentage(second), long.percentage(second))
    else {
        panic!("week two climbs in both")
    };
    assert!(
        short_second > long_second,
        "a shorter block advances faster: {short_second} vs {long_second}"
    );
}

/// A block with one climbing week is degenerate, not invalid — and must not
/// divide by zero working out a step it does not have.
#[test]
fn a_single_climbing_week_does_not_divide_by_zero() {
    let (Ok(start), Ok(end), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), pct("105%"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    // Two weeks: one climbing, then the test.
    let Ok(ladder) = Ladder::new(start, end, 2) else {
        panic!("two weeks is the shortest block")
    };
    assert_eq!(ladder.climbing_weeks(), 1);

    let Ok(only) = week(1) else {
        panic!("weeks are one-based")
    };
    assert_eq!(ladder.percentage(only), Some(start));
    assert!(ladder.heavy_top_set(anchor, only, increment).is_some());
}

#[test]
fn a_block_too_short_to_climb_is_refused() {
    let (Ok(start), Ok(end)) = (pct("92.5%"), pct("105%")) else {
        panic!("the span is valid")
    };
    assert!(Ladder::new(start, end, 1).is_err());
    assert!(Ladder::new(start, end, 0).is_err());
}

#[test]
fn a_ladder_that_does_not_rise_is_refused() {
    let (Ok(low), Ok(high)) = (pct("92.5%"), pct("105%")) else {
        panic!("the span is valid")
    };
    // Descending, and flat. Neither is a plan to increase anything.
    assert!(Ladder::new(high, low, 8).is_err());
    assert!(Ladder::new(high, high, 8).is_err());
}

proptest! {
    /// US3-2: the anchor does not move within a block.
    ///
    /// Stated as a property because it is the invariant the whole separation of
    /// plan from failure rests on. Here it is structural — the ladder is never
    /// handed anything that could change the anchor — and the integration test
    /// in Phase 5 asserts it again against a performed record.
    #[test]
    fn every_week_derives_from_the_same_anchor(
        anchor_grams in 20_000_u64..300_000,
        duration in 2_u32..16,
    ) {
        let (Ok(start), Ok(end), Ok(increment)) = (pct("92.5%"), pct("105%"), grid()) else {
            panic!("the span and grid are valid")
        };
        let Ok(ladder) = Ladder::new(start, end, duration) else {
            panic!("a rising span over two or more weeks is a ladder")
        };
        let anchor = Kg::from_grams(anchor_grams);

        // Every climbing week's load is a function of this one anchor. Recomputing
        // with the same anchor gives the same answer, and there is no call that
        // could have advanced it — the ladder takes the anchor per query and
        // holds none.
        for index in 1..=ladder.climbing_weeks() {
            let Ok(w) = WeekIndex::new(index) else {
                panic!("weeks are one-based")
            };
            let first = ladder.heavy_top_set(anchor, w, increment);
            let again = ladder.heavy_top_set(anchor, w, increment);
            prop_assert_eq!(first, again);
        }
    }

    /// The ladder never descends, and always lands on the grid.
    #[test]
    fn the_ladder_rises_monotonically(
        anchor_grams in 40_000_u64..300_000,
        duration in 3_u32..16,
    ) {
        let (Ok(start), Ok(end), Ok(increment)) = (pct("80%"), pct("105%"), grid()) else {
            panic!("the span and grid are valid")
        };
        let Ok(ladder) = Ladder::new(start, end, duration) else {
            panic!("a rising span is a ladder")
        };
        let anchor = Kg::from_grams(anchor_grams);

        let mut previous: Option<Kg> = None;
        for index in 1..=ladder.climbing_weeks() {
            let Ok(w) = WeekIndex::new(index) else {
                panic!("weeks are one-based")
            };
            let Some(load) = ladder.heavy_top_set(anchor, w, increment) else {
                panic!("week {index} of {} climbs", ladder.climbing_weeks())
            };
            prop_assert_eq!(load.as_grams() % increment.as_kg().as_grams(), 0);
            if let Some(previous) = previous {
                prop_assert!(load >= previous, "the ladder must not descend");
            }
            previous = Some(load);
        }
    }
}
