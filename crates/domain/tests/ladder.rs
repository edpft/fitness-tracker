//! The plan (T028, covering US3 scenarios 1 to 4).
//!
//! No performed record is involved in any of this. The plan is a total function
//! of a duration, a starting 1RM, an authored opening and an authored rate —
//! which is the whole point of separating it from the failure mechanism, and is
//! why these are table and property tests rather than integration ones.

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
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    // Seven weeks: six climbing, then the test.
    let Ok(ladder) = Ladder::new(start, climb, 7) else {
        panic!("a rising climb over seven weeks is a ladder")
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
    // load, which the type says rather than a caller remembering.
    let Ok(past_the_climb) = week(7) else {
        panic!("weeks are one-based")
    };
    assert!(
        ladder
            .heavy_top_set(anchor, past_the_climb, increment)
            .is_none()
    );
    assert!(
        ladder
            .implied_percentage(anchor, past_the_climb, increment)
            .is_none()
    );
}

/// One worked ladder, load for load.
///
/// **These are the same six loads the old span-divided ladder produced**, and
/// under that model it was a coincidence worth disclaiming: a 2.5-percentage-point
/// step happened to be 2.25kg at a 90kg anchor, which quantised back onto the
/// plate. Here one plate a week is what was authored, so the table is the
/// parameters restated rather than a near miss that rounded well.
#[test]
fn the_worked_ladder_reproduces_its_table() {
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    let Ok(ladder) = Ladder::new(start, climb, 7) else {
        panic!("a rising climb over seven weeks is a ladder")
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

/// The climb passes the anchor, and the report says so.
///
/// The last two rungs above are 92.5 and 95 against a 90kg anchor. Nothing
/// prescribes from this number — it is read back out of the load — but an
/// operator seeing 105.5% of a tested max is seeing the block's whole intent.
#[test]
fn the_implied_percentage_reads_the_climb_past_the_anchor() {
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    let (Ok(ladder), Ok(first), Ok(last)) = (Ladder::new(start, climb, 7), week(1), week(6)) else {
        panic!("the ladder and its weeks are valid")
    };

    let (Some(opened), Some(finished)) = (
        ladder.implied_percentage(anchor, first, increment),
        ladder.implied_percentage(anchor, last, increment),
    ) else {
        panic!("weeks one and six both climb")
    };

    // 82.5/90 and 95/90.
    assert_eq!(opened.as_basis_points(), 9166);
    assert_eq!(finished.as_basis_points(), 10555);
    assert!(finished > Percentage::WHOLE, "the climb passes the anchor");
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
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor), Ok(light_of_heavy)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"), pct("85%"))
    else {
        panic!("the fixture values are all valid")
    };
    let Ok(ladder) = Ladder::new(start, climb, 7) else {
        panic!("a rising climb over seven weeks is a ladder")
    };

    // 85% of the heavy weeks 82.5, 85 and 87.5, on the 2.5kg grid.
    let expected = ["70", "72.5", "75"];
    for (offset, want) in expected.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(0) + 1;
        let (Ok(w), Ok(want_kg)) = (week(index), kg(want)) else {
            panic!("week {index} and {want} are both valid")
        };
        let Some(got) = ladder.light_top_set(anchor, w, increment, light_of_heavy) else {
            panic!("light week {index} is a climbing week")
        };
        assert_eq!(got, want_kg, "light week {index}");
    }
}

/// US3-3: the rate is authored, so a longer duration climbs further rather than
/// climbing slower.
///
/// **This is the inverse of what this test used to assert.** The model authored
/// an endpoint and divided the span by the duration, so two durations finished
/// at the same load and a longer one advanced more slowly. The operator settled
/// on 2026-08-19 that a linear block does not target an endpoint at all: it adds
/// a fixed increment each week and the reset protocol regulates it. If someone
/// later re-derives the step from an endpoint, this is what fails.
#[test]
fn the_rate_is_authored_and_the_endpoint_is_wherever_the_calendar_stops() {
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    let (Ok(short), Ok(long)) = (Ladder::new(start, climb, 7), Ladder::new(start, climb, 13))
    else {
        panic!("both durations make a ladder")
    };

    // Same second week, whatever the duration: the rate does not know the
    // duration, which is the whole difference from the span model.
    let Ok(second) = week(2) else {
        panic!("weeks are one-based")
    };
    assert_eq!(
        short.heavy_top_set(anchor, second, increment),
        long.heavy_top_set(anchor, second, increment),
        "the step does not depend on how long the block runs"
    );

    // Different endpoint, because the longer block climbs for longer.
    let (Ok(short_last), Ok(long_last)) = (week(6), week(12)) else {
        panic!("weeks are one-based")
    };
    let (Some(short_end), Some(long_end)) = (
        short.heavy_top_set(anchor, short_last, increment),
        long.heavy_top_set(anchor, long_last, increment),
    ) else {
        panic!("both blocks climb to their last week")
    };
    assert!(
        long_end > short_end,
        "a longer block finishes higher: {long_end} vs {short_end}"
    );
}

/// A block with one climbing week is degenerate, not invalid — and opens where
/// the ladder opens, having had no week in which to climb.
#[test]
fn a_single_climbing_week_opens_and_stops() {
    let (Ok(start), Ok(climb), Ok(increment), Ok(anchor)) =
        (pct("92.5%"), kg("2.5"), grid(), kg("90"))
    else {
        panic!("the fixture values are all valid")
    };
    // Two weeks: one climbing, then the test.
    let Ok(ladder) = Ladder::new(start, climb, 2) else {
        panic!("two weeks is the shortest block")
    };
    assert_eq!(ladder.climbing_weeks(), 1);

    let (Ok(only), Ok(opening)) = (week(1), kg("82.5")) else {
        panic!("the week and the opening load are valid")
    };
    // 92.5% of 90 is 83.25, which is one plate grid down.
    assert_eq!(ladder.heavy_top_set(anchor, only, increment), Some(opening));
}

#[test]
fn a_block_too_short_to_climb_is_refused() {
    let (Ok(start), Ok(climb)) = (pct("92.5%"), kg("2.5")) else {
        panic!("the opening and rate are valid")
    };
    assert!(Ladder::new(start, climb, 1).is_err());
    assert!(Ladder::new(start, climb, 0).is_err());
}

/// A rate of nothing is not a plan to increase anything.
///
/// The old model refused a span that descended or stood still. A rate cannot
/// descend — [`Kg`] is unsigned — so what is left to refuse is zero, and the
/// store's `CHECK (ladder_climb_grams > 0)` refuses it again.
#[test]
fn a_ladder_that_does_not_rise_is_refused() {
    let Ok(start) = pct("92.5%") else {
        panic!("the opening is valid")
    };
    assert!(Ladder::new(start, Kg::from_grams(0), 8).is_err());
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
        let (Ok(start), Ok(climb), Ok(increment)) = (pct("92.5%"), kg("2.5"), grid()) else {
            panic!("the opening, rate and grid are valid")
        };
        let Ok(ladder) = Ladder::new(start, climb, duration) else {
            panic!("a rising climb over two or more weeks is a ladder")
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
        let (Ok(start), Ok(climb), Ok(increment)) = (pct("80%"), kg("2.5"), grid()) else {
            panic!("the opening, rate and grid are valid")
        };
        let Ok(ladder) = Ladder::new(start, climb, duration) else {
            panic!("a rising climb is a ladder")
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

    /// Consecutive weeks differ by exactly the authored rate.
    ///
    /// The property the span model could not have: there, the gap between two
    /// weeks depended on the anchor, the span and the duration together, and
    /// quantisation collapsed some pairs onto one bar. A rate that is a whole
    /// number of plates puts the same gap between every pair, at every anchor.
    #[test]
    fn every_step_is_the_authored_rate(
        anchor_grams in 40_000_u64..300_000,
        duration in 3_u32..16,
    ) {
        let (Ok(start), Ok(climb), Ok(increment)) = (pct("80%"), kg("2.5"), grid()) else {
            panic!("the opening, rate and grid are valid")
        };
        let Ok(ladder) = Ladder::new(start, climb, duration) else {
            panic!("a rising climb is a ladder")
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
            if let Some(previous) = previous {
                prop_assert_eq!(
                    load.as_grams() - previous.as_grams(),
                    climb.as_grams(),
                    "week {} does not sit one rate above week {}", index, index - 1
                );
            }
            previous = Some(load);
        }
    }
}
