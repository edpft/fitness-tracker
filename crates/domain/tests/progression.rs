//! The failure mechanism: hold, stall, reset, re-climb, resume.
//!
//! User story 3's second half, and `docs/primary-lift-progression.md` is the model
//! of record. The mechanism is pure arithmetic over a sequence of gating top sets,
//! so it is tested here rather than through the store: driving nine synthetic
//! sessions through a landing table would test the fixture, and the property that
//! matters — that the anchor is untouchable — is a property of the signature.
//!
//! **One correction to the model of record, and it is arithmetic.** The worked
//! example's second reset is written as dropping to 80kg from a failed 90 and
//! re-climbing 82.5, 85, 87.5, 90 — a 10kg drop at +2.5kg a week, which is four
//! re-climb weeks. The stated protocol is **−5%** at +2.5kg, which from 90 is 85.5
//! and lands on 85, re-climbing 87.5 then 90: two weeks, the same two the first
//! reset costs. The table below is what the stated parameters produce, and the
//! prose beside them agrees with it — "the drop and the increment are chosen as a
//! pair so both cost the same" is only true of −5% with +2.5kg. So the example's
//! rows are the error, not the protocol.

use domain::gym::Kg;
use domain::prescription::{
    Anchor, AnchorProvenance, ClimbBack, GatingTopSet, Ladder, Percentage, PlateIncrement,
    Progress, Reset, ResetProtocol, WeekIndex, progress_after,
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn kg(text: &str) -> Fallible<Kg> {
    Ok(Kg::try_from(text.to_owned())?)
}

fn pct(text: &str) -> Fallible<Percentage> {
    Ok(Percentage::try_from(text.to_owned())?)
}

fn grid() -> Fallible<PlateIncrement> {
    Ok(PlateIncrement::new(kg("2.5")?)?)
}

/// The two protocols as `docs/primary-lift-progression.md` states them.
fn protocols() -> Fallible<(ResetProtocol, ResetProtocol)> {
    Ok((
        ResetProtocol {
            drop: pct("-10%")?,
            reclimb_per_week: kg("5")?,
        },
        ResetProtocol {
            drop: pct("-5%")?,
            reclimb_per_week: kg("2.5")?,
        },
    ))
}

const fn missed(load: Kg) -> GatingTopSet {
    GatingTopSet {
        load,
        completed: false,
    }
}

const fn completed(load: Kg) -> GatingTopSet {
    GatingTopSet {
        load,
        completed: true,
    }
}

/// An eight-week block whose ladder opens at 90.
///
/// **Its entry test completed 87.5 and failed nothing**, so the block opens one
/// climb above what it reached — which is 90, the load the worked example below
/// misses twice. An entry test that had *failed* something would open the block
/// re-climbing, which is a different scenario and has its own tests at the end
/// of this file.
///
/// The failure mechanism is what is under test, so the ladder is here to be held
/// and resumed rather than read: every assertion below is either about a
/// re-climb — where the load comes from the failed set and not from the plan —
/// or about the ladder being the same object afterwards.
fn ladder_at_ninety() -> Fallible<(Ladder, Kg)> {
    let anchor = Anchor::new(
        kg("87.5")?,
        None,
        AnchorProvenance::Tested,
        jiff::civil::Date::new(2026, 7, 3)?,
    )?;
    Ok((Ladder::new(anchor, kg("2.5")?, 8, grid()?)?, kg("87.5")?))
}

/// The load a sequence of gating sessions leads to.
fn next_load(sets: &[GatingTopSet]) -> Fallible<(Progress, Option<Kg>)> {
    let (first, second) = protocols()?;
    let (ladder, anchor) = ladder_at_ninety()?;
    let _ = anchor;
    let progress = progress_after(sets, None, first, second, grid()?);
    Ok((progress, progress.heavy_top_set(ladder, grid()?)))
}

/// FR-021, and the first test written because it is the invariant an
/// implementation is most likely to break for convenience.
///
/// **The anchor cannot move, because nothing here is given one.** `progress_after`
/// takes the gating sets, the two protocols and the plate grid — a reset's drop
/// comes from the failed load and its re-climb from its own rate, so there is no
/// expression in which the anchor could appear. This asserts the observable
/// consequence: two blocks anchored differently produce identical re-climb loads
/// from identical failures.
#[test]
fn a_reset_never_touches_the_anchor() {
    let (Ok((first, second)), Ok(increment), Ok(ninety)) = (protocols(), grid(), kg("90")) else {
        panic!("the fixtures build")
    };
    let sets = [missed(ninety), missed(ninety)];
    let progress = progress_after(&sets, None, first, second, increment);

    let Progress::ReClimbing { load, toward, .. } = progress else {
        panic!("two misses at one load suspend the ladder, got {progress:?}")
    };
    assert_eq!(
        Some(load),
        kg("80").ok(),
        "−10% of 90 is 81, and the grid takes it to 80"
    );
    assert_eq!(
        Some(toward),
        kg("90").ok(),
        "the ladder waits at the load that failed"
    );

    // The same failure inside a block anchored anywhere gives the same re-climb,
    // which is what "the drop is taken from the failed load" means observably.
    for entry in ["80", "90", "140"] {
        let (Ok(entry), Ok(when)) = (kg(entry), jiff::civil::Date::new(2026, 7, 3)) else {
            panic!("the anchors are masses and the date is a date")
        };
        let (Ok(anchor), Ok(rate)) = (
            Anchor::new(entry, None, AnchorProvenance::Tested, when),
            kg("2.5"),
        ) else {
            panic!("a positive load is an anchor")
        };
        let Ok(plan) = Ladder::new(anchor, rate, 8, increment) else {
            panic!("the ladder builds")
        };
        assert_eq!(
            progress.heavy_top_set(plan, increment),
            Some(load),
            "the re-climb load is the same whatever the block was anchored at"
        );
    }
}

/// US3-5: a miss holds the ladder.
#[test]
fn a_miss_holds_the_ladder() {
    let Ok(ninety) = kg("90") else {
        panic!("90 is a mass")
    };
    let Ok((progress, load)) = next_load(&[missed(ninety)]) else {
        panic!("the fixtures build")
    };

    assert_eq!(
        progress,
        Progress::Climbing {
            week: WeekIndex::FIRST
        },
        "a first miss leaves the position alone"
    );
    assert_eq!(load, Some(ninety), "and re-issues the same load");
}

/// US3-4 and US3-2: nothing performed climbs it faster, and the anchor holds.
#[test]
fn a_completed_set_advances_exactly_one_week() {
    let Ok(ninety) = kg("90") else {
        panic!("90 is a mass")
    };
    let Ok((progress, _)) = next_load(&[completed(ninety)]) else {
        panic!("the fixtures build")
    };
    assert_eq!(
        progress,
        Progress::Climbing {
            week: WeekIndex::FIRST.next()
        }
    );
    assert_eq!(progress.reset(), None, "no reset is in play");
}

/// US3-6: a second miss at the same load suspends the ladder.
#[test]
fn a_second_miss_suspends_the_ladder() {
    let Ok(ninety) = kg("90") else {
        panic!("90 is a mass")
    };
    let Ok((progress, load)) = next_load(&[missed(ninety), missed(ninety)]) else {
        panic!("the fixtures build")
    };

    assert_eq!(progress.reset(), Some(Reset::First));
    assert_eq!(load, kg("80").ok());
    assert_eq!(
        progress.week(),
        WeekIndex::FIRST,
        "and the ladder waits where it was"
    );
}

/// A second miss at a *different* load is not a stall. It is a first miss there.
#[test]
fn a_miss_at_a_new_load_is_not_a_stall() {
    let (Ok(ninety), Ok(ninety_two)) = (kg("90"), kg("92.5")) else {
        panic!("both are masses")
    };
    let Ok((progress, _)) = next_load(&[missed(ninety), missed(ninety_two)]) else {
        panic!("the fixtures build")
    };
    assert_eq!(progress.reset(), None, "two loads, two first misses");
}

/// US3-7: a completed re-climb resumes the ladder where it was suspended.
#[test]
fn a_completed_re_climb_resumes_the_ladder() {
    let (Ok(ninety), Ok(eighty), Ok(eighty_five)) = (kg("90"), kg("80"), kg("85")) else {
        panic!("the masses build")
    };
    let sets = [
        missed(ninety),
        missed(ninety),
        completed(eighty),
        completed(eighty_five),
    ];
    let Ok((progress, load)) = next_load(&sets) else {
        panic!("the fixtures build")
    };

    assert_eq!(
        progress,
        Progress::Climbing {
            week: WeekIndex::FIRST
        },
        "the plan takes over again at the week it was suspended at"
    );
    assert_eq!(load, Some(ninety), "which is the load that failed");
}

/// US3-8: the second stall is the slower reset, and it drops from the failed load.
#[test]
fn the_second_stall_is_the_slower_reset() {
    let (Ok(ninety), Ok(eighty), Ok(eighty_five)) = (kg("90"), kg("80"), kg("85")) else {
        panic!("the masses build")
    };
    let sets = [
        missed(ninety),
        missed(ninety),
        completed(eighty),
        completed(eighty_five),
        // Resumed at 90 and missed twice more. The first of these is a *first*
        // miss: the reset that answered the last stall spent it.
        missed(ninety),
        missed(ninety),
    ];
    let Ok((progress, load)) = next_load(&sets) else {
        panic!("the fixtures build")
    };

    assert_eq!(progress.reset(), Some(Reset::Second));
    assert_eq!(
        load,
        kg("85").ok(),
        "−5% of 90 is 85.5, and the grid takes it to 85"
    );
}

/// SC-005: the worked example, load for load, with the anchor constant.
///
/// The example in `docs/primary-lift-progression.md`, corrected for the second
/// reset as the module note explains. Nine weeks rather than eleven, because two
/// re-climbs of two weeks each is what the stated protocols cost.
#[test]
fn the_worked_example_reproduces_load_for_load() {
    let Ok((first, second)) = protocols() else {
        panic!("the protocols build")
    };
    let (Ok(increment), Ok((ladder, anchor))) = (grid(), ladder_at_ninety()) else {
        panic!("the ladder builds")
    };

    // Week, what the plan prescribes, and what was done with it.
    let expected: [(&str, bool); 9] = [
        ("90", false),  // 1  miss
        ("90", false),  // 2  miss → reset 1
        ("80", true),   // 3  −10% from the failed load, on the grid
        ("85", true),   // 4  +5kg
        ("90", false),  // 5  the re-climb arrived; the ladder resumed. miss
        ("90", false),  // 6  miss → reset 2
        ("85", true),   // 7  −5% from the failed load, on the grid
        ("87.5", true), // 8  +2.5kg
        ("90", false),  // 9  arrived and resumed again
    ];

    let mut performed: Vec<GatingTopSet> = Vec::new();
    for (week, (load, went_up)) in expected.into_iter().enumerate() {
        let progress = progress_after(&performed, None, first, second, increment);
        let issued = progress.heavy_top_set(ladder, increment);
        assert_eq!(
            issued,
            kg(load).ok(),
            "week {} prescribes {load}, got {issued:?} in {progress:?}",
            week + 1
        );

        let Some(issued) = issued else {
            panic!("every week of this example prescribes a load")
        };
        performed.push(GatingTopSet {
            load: issued,
            completed: went_up,
        });
    }

    // The anchor was never an input to any of it, and the ladder never changed.
    // Rebuilding from the same entry test gives the same plan, which is what
    // "nothing performed moves it" means observably.
    let Ok((ladder_again, anchor_again)) = ladder_at_ninety() else {
        panic!("the ladder rebuilds")
    };
    assert_eq!(ladder, ladder_again, "the plan is untouched by any of this");
    assert_eq!(anchor, anchor_again, "and so is the anchor");
}

/// A third stall has no protocol, so it holds rather than inventing one.
#[test]
fn a_third_stall_holds_at_the_failed_load() {
    let (Ok(ninety), Ok(eighty), Ok(eighty_five), Ok(eighty_seven)) =
        (kg("90"), kg("80"), kg("85"), kg("87.5"))
    else {
        panic!("the masses build")
    };
    let sets = [
        missed(ninety),
        missed(ninety),
        completed(eighty),
        completed(eighty_five),
        missed(ninety),
        missed(ninety),
        completed(eighty_five),
        completed(eighty_seven),
        missed(ninety),
        missed(ninety),
    ];
    let Ok((progress, load)) = next_load(&sets) else {
        panic!("the fixtures build")
    };

    assert_eq!(
        progress.reset(),
        None,
        "there is no third reset to escalate to, so the plan stays in charge"
    );
    assert_eq!(load, Some(ninety), "and it holds at the load that failed");
}

/// US3-10, at the boundary rather than in the mechanism: only the gating role's
/// sets reach this at all.
///
/// Asserted as the absence it is — an empty sequence is a block nobody has trained
/// yet, and it prescribes week one. A non-gating miss contributes nothing to the
/// slice, so it cannot be distinguished from that here, which is exactly why the
/// filtering belongs at the caller and is tested there.
#[test]
fn an_untrained_block_is_at_its_first_week() {
    let Ok((progress, _)) = next_load(&[]) else {
        panic!("the fixtures build")
    };
    assert_eq!(
        progress,
        Progress::Climbing {
            week: WeekIndex::FIRST
        }
    );
}

/// A block whose entry test found a ceiling opens climbing back to it.
///
/// The test completed 90 and failed 95, so the block opens at 95 and gets there
/// the way any re-climb does: the first reset's drop from the failed load, then
/// its rate. Nothing has been performed yet, so this is the state the very first
/// session of the block is issued from.
#[test]
fn an_entry_test_that_found_a_ceiling_opens_climbing_back_to_it() {
    let (Ok((first, second)), Ok(increment), Ok(failed)) = (protocols(), grid(), kg("95")) else {
        panic!("the fixture values are all valid")
    };

    let progress = progress_after(&[], Some(failed), first, second, increment);

    // −10% of 95 is 85.5, and the plate grid takes it to 85.
    let Ok(opening) = kg("85") else {
        panic!("85 is a mass")
    };
    assert_eq!(progress.climb_back(), Some(ClimbBack::Entry));
    assert_eq!(
        progress.reset(),
        None,
        "the entry is not a reset and must not report as one"
    );
    match progress {
        Progress::ReClimbing { load, toward, .. } => {
            assert_eq!(load, opening);
            assert_eq!(toward, failed, "it climbs back to what the test failed");
        }
        Progress::Climbing { .. } => panic!("a ceiling opens re-climbing"),
    }
}

/// US3-11: the entry climb spends no stall, so the next failure still gets both.
///
/// **This is the whole reason [`ClimbBack`] exists.** Settled by the operator on
/// 2026-08-19: opening a block from a failed test is not a stall, so a stall
/// later in the block is the *first* reset and a second is still available. If
/// the entry were counted, the sequence below would escalate straight to the
/// second reset and the block would have one protocol left instead of two.
#[test]
fn the_entry_climb_spends_no_stall() {
    let (Ok((first, second)), Ok(increment), Ok(failed)) = (protocols(), grid(), kg("95")) else {
        panic!("the fixture values are all valid")
    };

    // Climb in from the entry (85, 90), reach 95, then stall there twice.
    let Ok((eighty_five, ninety)) =
        (|| Ok::<_, Box<dyn std::error::Error>>((kg("85")?, kg("90")?)))()
    else {
        panic!("the loads are masses")
    };
    let performed = [
        GatingTopSet {
            load: eighty_five,
            completed: true,
        },
        GatingTopSet {
            load: ninety,
            completed: true,
        },
        GatingTopSet {
            load: failed,
            completed: false,
        },
        GatingTopSet {
            load: failed,
            completed: false,
        },
    ];

    let progress = progress_after(&performed, Some(failed), first, second, increment);

    assert_eq!(
        progress.reset(),
        Some(Reset::First),
        "the first stall of the block is the first reset, not the second"
    );
}

/// A test that failed nothing opens on the plan, with no climb back at all.
#[test]
fn an_entry_test_that_failed_nothing_opens_on_the_plan() {
    let (Ok((first, second)), Ok(increment)) = (protocols(), grid()) else {
        panic!("the fixture values are all valid")
    };

    let progress = progress_after(&[], None, first, second, increment);

    assert_eq!(progress.climb_back(), None);
    assert_eq!(progress.week(), WeekIndex::FIRST);
}
