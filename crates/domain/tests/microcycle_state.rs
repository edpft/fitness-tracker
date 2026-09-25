//! The second of the two state machines: where the concurrent microcycle
//! stands (#177).
//!
//! The operator, 2026-09-21: *"From the perspective of `fitness next`, the
//! 'programme' is the concurrent programme, i.e. both the gym and cycling
//! programmes moving together."* So a microcycle is not the gym's or the
//! bike's; it is the week, and what re-runs is decided across both.

use domain::{
    planner::{
        ESSENTIAL, MicrocycleState, Owed, Placed, Rerun, SessionState, microcycle_state, owed_by,
        rerun,
    },
    schedule::{AbsenceKind, Discipline, Relative, SessionRole},
};
use jiff::civil::{Date, date};

/// The easier, longer session of the week: never the essential one.
const fn supporting() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

const fn session(discipline: Discipline, role: SessionRole, state: SessionState) -> Placed {
    Placed {
        discipline,
        role,
        state,
        test: false,
    }
}

/// The essential session of a test microcycle: the 1RM test or the FTP test.
const fn test(discipline: Discipline, state: SessionState) -> Placed {
    Placed {
        discipline,
        role: ESSENTIAL,
        state,
        test: true,
    }
}

/// The Monday of the entry test week the operator re-ran.
const MONDAY: Date = date(2026, 9, 21);

/// **The operator's week of 14 September 2026.** The light gym session went to
/// Rome, the entry test to illness; the Wednesday ride was the *easier* one, so
/// the FTP test is still owed. Neither discipline performed its essential
/// session, so neither is ahead and no holding week is reached.
#[test]
fn a_week_that_lost_both_essential_sessions_is_incomplete() {
    let week = [
        session(
            Discipline::Gym,
            supporting(),
            SessionState::Skipped {
                absence: AbsenceKind::FamilyHoliday,
            },
        ),
        session(
            Discipline::Cycling,
            ESSENTIAL,
            SessionState::NotPerformed { absence: None },
        ),
        session(
            Discipline::Gym,
            ESSENTIAL,
            SessionState::NotPerformed {
                absence: Some(AbsenceKind::Illness),
            },
        ),
        session(Discipline::Cycling, supporting(), SessionState::Performed),
    ];

    assert_eq!(microcycle_state(&week), MicrocycleState::Incomplete);
}

/// **A supporting session lost costs the microcycle nothing.** That is what
/// naming an essential session is for: the light gym session went to Rome and
/// the week still completed.
#[test]
fn losing_a_supporting_session_does_not_stop_the_week() {
    let week = [
        session(
            Discipline::Gym,
            supporting(),
            SessionState::Skipped {
                absence: AbsenceKind::FamilyHoliday,
            },
        ),
        session(Discipline::Gym, ESSENTIAL, SessionState::Performed),
        session(Discipline::Cycling, ESSENTIAL, SessionState::Performed),
        session(
            Discipline::Cycling,
            supporting(),
            SessionState::NotPrescribed,
        ),
    ];

    assert_eq!(microcycle_state(&week), MicrocycleState::Completed);
}

/// **One did and one did not**: the transition a holding week comes from. The
/// gym repeats its microcycle and cycling holds, so the two still start the
/// next mesocycle together.
#[test]
fn one_discipline_completing_and_one_losing_is_partially_completed() {
    let week = [
        session(
            Discipline::Gym,
            ESSENTIAL,
            SessionState::NotPerformed {
                absence: Some(AbsenceKind::Illness),
            },
        ),
        session(Discipline::Cycling, ESSENTIAL, SessionState::Performed),
    ];

    assert_eq!(
        microcycle_state(&week),
        MicrocycleState::PartiallyCompleted {
            completed: Discipline::Cycling,
            lost: Discipline::Gym,
        }
    );
}

/// **A week with time left in it has not decided anything.** An essential
/// session still to be prescribed, or prescribed and not yet performed, keeps
/// the microcycle running — and nothing re-runs while it is.
#[test]
fn an_essential_session_with_time_left_keeps_the_week_running() {
    for state in [SessionState::ToBePrescribed, SessionState::Prescribed] {
        let week = [
            session(Discipline::Gym, ESSENTIAL, state),
            session(Discipline::Cycling, ESSENTIAL, SessionState::Performed),
        ];

        assert_eq!(
            microcycle_state(&week),
            MicrocycleState::Running,
            "{state} leaves the week running"
        );
    }
}

/// **A discipline the week holds no essential session for owes nothing.** The
/// diary allocates the week, and a week it gives cycling nothing is a week
/// cycling is not in — not a week cycling failed.
#[test]
fn a_discipline_with_no_essential_session_owes_nothing() {
    let week = [
        session(Discipline::Gym, ESSENTIAL, SessionState::Performed),
        session(Discipline::Cycling, supporting(), SessionState::Performed),
    ];

    assert_eq!(owed_by(&week, Discipline::Cycling), Owed::Absent);
    assert_eq!(microcycle_state(&week), MicrocycleState::Completed);
}

/// **A week with nothing in it is complete**, because nothing is waiting on it.
#[test]
fn an_empty_week_is_complete() {
    assert_eq!(microcycle_state(&[]), MicrocycleState::Completed);
}

/// **Every way of losing an essential session loses it**, whatever the reason.
/// What the reason decides is what the *next* week looks like, which is rule
/// 5's business rather than this machine's.
#[test]
fn every_way_of_not_performing_the_essential_session_loses_it() {
    for state in [
        SessionState::Skipped {
            absence: AbsenceKind::FamilyHoliday,
        },
        SessionState::Skipped {
            absence: AbsenceKind::Illness,
        },
        SessionState::NotPrescribed,
        SessionState::NotPerformed { absence: None },
        SessionState::NotPerformed {
            absence: Some(AbsenceKind::Illness),
        },
    ] {
        let week = [session(Discipline::Gym, ESSENTIAL, state)];
        assert_eq!(owed_by(&week, Discipline::Gym), Owed::Lost, "{state}");
    }
}

/// **The FTP test ridden and the 1RM test missed** (#190). The gym re-runs its
/// test week; cycling holds, because it would otherwise have to ride the test
/// again.
#[test]
fn a_test_completed_holds_while_the_test_missed_re_runs() {
    let week = [
        test(
            Discipline::Gym,
            SessionState::NotPerformed {
                absence: Some(AbsenceKind::Illness),
            },
        ),
        test(Discipline::Cycling, SessionState::Performed),
    ];

    assert_eq!(
        rerun(MONDAY, microcycle_state(&week), &week),
        Some(Rerun {
            monday: MONDAY,
            holding: Some(Discipline::Cycling),
        })
    );
}

/// **And the other way round**: the 1RM test done and the FTP test missed.
#[test]
fn the_gym_holds_when_its_test_was_done_and_the_ride_was_not() {
    let week = [
        test(Discipline::Gym, SessionState::Performed),
        test(Discipline::Cycling, SessionState::NotPrescribed),
    ];

    assert_eq!(
        rerun(MONDAY, microcycle_state(&week), &week),
        Some(Rerun {
            monday: MONDAY,
            holding: Some(Discipline::Gym),
        })
    );
}

/// **A partially completed week that tested nothing re-runs for both.** The
/// operator, 2026-09-25: *"if the partially completed microcycle is a non-test
/// microcycle, we should just repeat the microcycle."*
#[test]
fn a_partially_completed_week_of_no_test_re_runs_for_both() {
    let week = [
        session(Discipline::Gym, ESSENTIAL, SessionState::Performed),
        session(
            Discipline::Cycling,
            ESSENTIAL,
            SessionState::NotPerformed { absence: None },
        ),
    ];

    assert_eq!(
        rerun(MONDAY, microcycle_state(&week), &week),
        Some(Rerun {
            monday: MONDAY,
            holding: None,
        })
    );
}

/// **The discipline that holds is the one that completed its test**, not any
/// discipline that had one: a test missed is re-run, never held.
#[test]
fn a_test_missed_is_never_held() {
    let week = [
        test(Discipline::Gym, SessionState::NotPrescribed),
        session(Discipline::Cycling, ESSENTIAL, SessionState::Performed),
    ];

    assert_eq!(
        rerun(MONDAY, microcycle_state(&week), &week),
        Some(Rerun {
            monday: MONDAY,
            holding: None,
        })
    );
}

/// A week that completed, or has not finished, runs nothing again.
#[test]
fn a_completed_or_running_week_runs_nothing_again() {
    let done = [
        test(Discipline::Gym, SessionState::Performed),
        test(Discipline::Cycling, SessionState::Performed),
    ];
    let running = [
        test(Discipline::Gym, SessionState::Performed),
        test(Discipline::Cycling, SessionState::Prescribed),
    ];

    assert_eq!(rerun(MONDAY, microcycle_state(&done), &done), None);
    assert_eq!(rerun(MONDAY, microcycle_state(&running), &running), None);
}
