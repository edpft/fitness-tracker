//! The second of the two state machines: where the concurrent microcycle
//! stands (#177).
//!
//! The operator, 2026-09-21: *"From the perspective of `fitness next`, the
//! 'programme' is the concurrent programme, i.e. both the gym and cycling
//! programmes moving together."* So a microcycle is not the gym's or the
//! bike's; it is the week, and what re-runs is decided across both.

use domain::{
    planner::{ESSENTIAL, MicrocycleState, Owed, Placed, SessionState, microcycle_state, owed_by},
    schedule::{AbsenceKind, Discipline, Relative, SessionRole},
};

/// The easier, longer session of the week: never the essential one.
const fn supporting() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

const fn session(discipline: Discipline, role: SessionRole, state: SessionState) -> Placed {
    Placed {
        discipline,
        role,
        state,
    }
}

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
