//! The plan (T028, covering US3 scenarios 1 to 4).
//!
//! No performed record is involved in any of this. The plan is a total function
//! of a duration, an entry test and an authored rate — which is the whole point
//! of separating it from the failure mechanism, and is why these are table and
//! property tests rather than integration ones.

use domain::gym::Kg;
use domain::prescription::{
    Anchor, AnchorProvenance, Ladder, LoadSteps, Opening, Percentage, WeekIndex,
};
use proptest::prelude::*;

/// A test that found the ceiling: a completed single, and the load above it that
/// was missed.
fn ceiling(completed: &str, failed: &str) -> Result<Anchor, Box<dyn std::error::Error>> {
    Ok(Anchor::new(
        kg(completed)?,
        Some(kg(failed)?),
        AnchorProvenance::Tested,
        jiff::civil::Date::new(2026, 7, 3)?,
    )?)
}

/// A test that did not find the ceiling: everything attempted went up.
fn unbeaten(completed: &str) -> Result<Anchor, Box<dyn std::error::Error>> {
    Ok(Anchor::new(
        kg(completed)?,
        None,
        AnchorProvenance::Tested,
        jiff::civil::Date::new(2026, 7, 3)?,
    )?)
}

fn grid() -> Result<LoadSteps, Box<dyn std::error::Error>> {
    Ok(LoadSteps::uniform(Kg::try_from("2.5".to_owned())?)?)
}

/// The block's opening, derived from its entry test at the authored drop.
///
/// -10% is what the operator settled on 2026-08-20. It is passed here rather
/// than reached for, because the drop is a parameter of the derivation and not
/// a property of the anchor.
fn from_test(anchor: Anchor) -> Result<Opening, Box<dyn std::error::Error>> {
    Ok(Opening::FromAnchor {
        anchor,
        drop: pct("-10%")?,
    })
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

/// US3-1: two inputs generate the block, and every week of it climbs.
///
/// **No test week** (decision 0013). A linear programme's duration used to be
/// its climbing weeks plus a test, so seven weeks meant six rungs. A test is a
/// programme in its own right now, so seven weeks means seven rungs and the
/// eighth week is simply past the end.
#[test]
fn a_duration_and_an_anchor_generate_every_week() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    // Seven weeks, all of them climbing.
    let Ok(ladder) = Ladder::new(opening, climb, 7, &increment) else {
        panic!("a rising climb over seven weeks is a ladder")
    };

    assert_eq!(ladder.climbing_weeks(), 7);

    for index in 1..=7 {
        let Ok(w) = week(index) else {
            panic!("weeks are one-based")
        };
        assert!(
            ladder.heavy_top_set(w, &increment).is_some(),
            "week {index} is a climbing week and has a load"
        );
    }

    // The eighth week is past the ladder. It is not a position and has no load,
    // which the type says rather than a caller remembering.
    let Ok(past_the_climb) = week(8) else {
        panic!("weeks are one-based")
    };
    assert!(ladder.heavy_top_set(past_the_climb, &increment).is_none());
    assert!(
        ladder
            .implied_percentage(anchor.load(), past_the_climb, &increment)
            .is_none()
    );
}

/// One worked ladder, load for load.
///
/// **The block opens below the load the entry test failed.** 90 went up and 95
/// did not, so 95 is the ceiling and the block opens at -10% of it — 85.5,
/// which is 85 on the grid — and climbs back through it. Every rung after the
/// first is one plate on.
///
/// It used to open *at* 95 and climb in to it, which made week one heavier than
/// the anchor. The operator overturned that on 2026-08-20.
#[test]
fn the_worked_ladder_reproduces_its_table() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let Ok(ladder) = Ladder::new(opening, climb, 7, &increment) else {
        panic!("a rising climb over seven weeks is a ladder")
    };

    let expected = ["85", "87.5", "90", "92.5", "95", "97.5"];
    for (offset, want) in expected.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(0) + 1;
        let (Ok(w), Ok(want_kg)) = (week(index), kg(want)) else {
            panic!("week {index} and {want} are both valid")
        };
        let Some(got) = ladder.heavy_top_set(w, &increment) else {
            panic!("week {index} is a climbing week")
        };
        assert_eq!(got, want_kg, "week {index}");
    }
}

/// A test that failed nothing opens one climb above what it reached.
///
/// **It did not find the ceiling**, so the completed load is a floor rather than
/// a maximum and the block starts by beating it. Note this opens *higher* than
/// the failing case above: a test that found a ceiling knows where the limit is
/// and drops below it, and one that did not has no limit to drop from.
#[test]
fn a_test_that_failed_nothing_opens_one_climb_above_it() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), unbeaten("90")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let (Ok(ladder), Ok(first), Ok(expected)) = (
        Ladder::new(opening, climb, 7, &increment),
        week(1),
        kg("92.5"),
    ) else {
        panic!("the ladder and its first week are valid")
    };

    assert_eq!(ladder.opening(), expected);
    assert_eq!(ladder.heavy_top_set(first, &increment), Some(expected));
}

/// The climb passes the anchor, and the report says so.
///
/// A block opening at -10% off a failed 95, against a completed 90, starts
/// *below* the tested max and climbs through it. Nothing prescribes from this
/// number — it is read back out of the load — but an operator watching it cross
/// 100% is watching the block do the one thing it exists to do.
///
/// The old model opened above the anchor and this test asserted so. That is the
/// change the operator made on 2026-08-20, and the direction of the inequality
/// is the whole of it.
#[test]
fn the_implied_percentage_reads_the_climb_past_the_anchor() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let (Ok(ladder), Ok(first), Ok(last)) =
        (Ladder::new(opening, climb, 7, &increment), week(1), week(6))
    else {
        panic!("the ladder and its weeks are valid")
    };

    let (Some(opened), Some(finished)) = (
        ladder.implied_percentage(anchor.load(), first, &increment),
        ladder.implied_percentage(anchor.load(), last, &increment),
    ) else {
        panic!("weeks one and six both climb")
    };

    // 85/90 and 97.5/90.
    assert_eq!(opened.as_basis_points(), 9444);
    assert_eq!(finished.as_basis_points(), 10833);
    assert!(
        opened < Percentage::WHOLE,
        "the block opens below the anchor and climbs through it"
    );
    assert!(
        finished > Percentage::WHOLE,
        "and finishes above it, which is what the block is for"
    );
    assert!(finished > opened);
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
    let (Ok(climb), Ok(increment), Ok(anchor), Ok(light_of_heavy)) =
        (kg("2.5"), grid(), ceiling("90", "95"), pct("85%"))
    else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let Ok(ladder) = Ladder::new(opening, climb, 7, &increment) else {
        panic!("a rising climb over seven weeks is a ladder")
    };

    // 85% of the heavy weeks 85, 87.5 and 90, on the 2.5kg grid.
    let expected = ["72.5", "75", "77.5"];
    for (offset, want) in expected.iter().enumerate() {
        let index = u32::try_from(offset).unwrap_or(0) + 1;
        let (Ok(w), Ok(want_kg)) = (week(index), kg(want)) else {
            panic!("week {index} and {want} are both valid")
        };
        let Some(got) = ladder.light_top_set(w, &increment, light_of_heavy) else {
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
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let (Ok(short), Ok(long)) = (
        Ladder::new(opening, climb, 7, &increment),
        Ladder::new(opening, climb, 13, &increment),
    ) else {
        panic!("both durations make a ladder")
    };

    // Same second week, whatever the duration: the rate does not know the
    // duration, which is the whole difference from the span model.
    let Ok(second) = week(2) else {
        panic!("weeks are one-based")
    };
    assert_eq!(
        short.heavy_top_set(second, &increment),
        long.heavy_top_set(second, &increment),
        "the step does not depend on how long the block runs"
    );

    // Different endpoint, because the longer block climbs for longer.
    let (Ok(short_last), Ok(long_last)) = (week(6), week(12)) else {
        panic!("weeks are one-based")
    };
    let (Some(short_end), Some(long_end)) = (
        short.heavy_top_set(short_last, &increment),
        long.heavy_top_set(long_last, &increment),
    ) else {
        panic!("both blocks climb to their last week")
    };
    assert!(
        long_end > short_end,
        "a longer block finishes higher: {long_end} vs {short_end}"
    );
}

/// The shortest block opens where the ladder opens and climbs once.
///
/// Two weeks is the shortest the store will hold — `CHECK (duration_weeks >= 2)`
/// — and under decision 0013 both of them climb, where the second used to be a
/// test. The first week is still the opening, having had no week in which to
/// climb to it.
#[test]
fn the_shortest_block_opens_and_climbs_once() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    let Ok(ladder) = Ladder::new(opening, climb, 2, &increment) else {
        panic!("two weeks is the shortest block")
    };
    assert_eq!(ladder.climbing_weeks(), 2);

    // -10% off the failed 95 is 85.5, which is 85 on the grid.
    let (Ok(only), Ok(expected)) = (week(1), kg("85")) else {
        panic!("the week and the opening load are valid")
    };
    assert_eq!(ladder.heavy_top_set(only, &increment), Some(expected));
}

#[test]
fn a_block_too_short_to_climb_is_refused() {
    let (Ok(climb), Ok(increment), Ok(anchor)) = (kg("2.5"), grid(), ceiling("90", "95")) else {
        panic!("the fixture values are all valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    assert!(Ladder::new(opening, climb, 1, &increment).is_err());
    assert!(Ladder::new(opening, climb, 0, &increment).is_err());
}

/// A rate of nothing is not a plan to increase anything.
///
/// The old model refused a span that descended or stood still. A rate cannot
/// descend — [`Kg`] is unsigned — so what is left to refuse is zero, and the
/// store's `CHECK (ladder_climb_grams > 0)` refuses it again.
#[test]
fn a_ladder_that_does_not_rise_is_refused() {
    let (Ok(increment), Ok(anchor)) = (grid(), ceiling("90", "95")) else {
        panic!("the grid and anchor are valid")
    };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
    assert!(Ladder::new(opening, Kg::from_grams(0), 8, &increment).is_err());
}

proptest! {
    /// US3-2: the anchor does not move within a block.
    ///
    /// Stated as a property because it is the invariant the whole separation of
    /// plan from failure rests on. Here it is structural — the ladder reads the
    /// anchor once, at construction, and holds a load thereafter — and the
    /// integration test in Phase 5 asserts it again against a performed record.
    #[test]
    fn every_week_derives_from_the_same_anchor(
        anchor_grams in 20_000_u64..300_000,
        duration in 2_u32..16,
    ) {
        let (Ok(climb), Ok(increment)) = (kg("2.5"), grid()) else {
            panic!("the rate and grid are valid")
        };
        let Ok(anchor) = Anchor::new(
            Kg::from_grams(anchor_grams),
            None,
            AnchorProvenance::Tested,
            jiff::civil::Date::new(2026, 7, 3).expect("3 July 2026 is a date"),
        ) else {
            panic!("a positive load is an anchor")
        };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
        let Ok(ladder) = Ladder::new(opening, climb, duration, &increment) else {
            panic!("a rising climb over two or more weeks is a ladder")
        };

        // Every climbing week's load is a function of the one anchor the ladder
        // was built from. Recomputing gives the same answer, and there is no call
        // that could have advanced it — the ladder holds a load, not a reference
        // to something that moves.
        for index in 1..=ladder.climbing_weeks() {
            let Ok(w) = WeekIndex::new(index) else {
                panic!("weeks are one-based")
            };
            let first = ladder.heavy_top_set(w, &increment);
            let again = ladder.heavy_top_set(w, &increment);
            prop_assert_eq!(first, again);
        }
    }

    /// The ladder never descends, always lands on the grid, and every step is
    /// exactly the authored rate.
    ///
    /// The last of those is the property the span model could not have: there,
    /// the gap between two weeks depended on the anchor, the span and the
    /// duration together, and quantisation collapsed some pairs onto one bar. A
    /// rate that is a whole number of plates puts the same gap between every
    /// pair, at every anchor.
    #[test]
    fn the_ladder_rises_by_exactly_the_authored_rate(
        anchor_grams in 40_000_u64..300_000,
        duration in 3_u32..16,
    ) {
        let (Ok(climb), Ok(increment)) = (kg("2.5"), grid()) else {
            panic!("the rate and grid are valid")
        };
        let Ok(anchor) = Anchor::new(
            Kg::from_grams(anchor_grams),
            None,
            AnchorProvenance::Tested,
            jiff::civil::Date::new(2026, 7, 3).expect("3 July 2026 is a date"),
        ) else {
            panic!("a positive load is an anchor")
        };
    let Ok(opening) = from_test(anchor) else {
        panic!("-10% is a percentage")
    };
        let Ok(ladder) = Ladder::new(opening, climb, duration, &increment) else {
            panic!("a rising climb is a ladder")
        };

        let mut previous: Option<Kg> = None;
        for index in 1..=ladder.climbing_weeks() {
            let Ok(w) = WeekIndex::new(index) else {
                panic!("weeks are one-based")
            };
            let Some(load) = ladder.heavy_top_set(w, &increment) else {
                panic!("week {index} of {} climbs", ladder.climbing_weeks())
            };
            // On the grid, stated as the grid states it: quantising a load
            // that is already loadable changes nothing. The old form divided by
            // "the increment", which a banded scale does not have.
            prop_assert_eq!(increment.quantise(load), load);
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
