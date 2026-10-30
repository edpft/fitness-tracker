//! A fill change holds into the mesocycles that follow it in the plan (#578).
//!
//! The operator changed three slots from 2026-10-16, in the plan's second
//! mesocycle, and the third opened on 2026-10-26 with the fills stated when the
//! plan was authored: the changes had been kept against the mesocycle they were
//! made in, and nothing later read them.

use crate::support;

use application::{FillChangeStore, MesocycleStore, PlanAuthor, prescribe::Authoring};
use domain::{
    gym::exercise::{Exercise, Implement, RepsExercise},
    prescription::{FillChange, SlotId, linear::SlotContent},
    schedule::SessionRole,
};
use jiff::civil::date;
use support::mesocycle::{self, entry_test_from};

#[test]
fn a_change_in_one_mesocycle_holds_in_the_next() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    runtime.block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let pool = infrastructure::connect(&directory.path().join("test.db"))
            .await
            .expect("a store");
        mesocycle::record_the_week(&pool).await.expect("the week");
        let zone = support::corpus::zone().expect("the zone");

        // Two test weeks a fortnight apart, both stating the fixture's fills:
        // the core slot is the bent-over cable chop.
        let plan = mesocycle::plan(vec![
            entry_test_from(date(2026, 9, 14)).expect("a test week"),
            entry_test_from(date(2026, 9, 28)).expect("a test week"),
        ])
        .expect("a plan");
        mesocycle::design(&pool).await.expect("the fixture designs");
        Authoring::new(infrastructure::SqlitePlanStore::new(
            pool.clone(),
            zone.clone(),
        ))
        .author(&plan)
        .await
        .expect("the plan is authored");

        let store = infrastructure::SqliteGymMesocycleStore::new(pool, zone);
        let first = store
            .on(date(2026, 9, 16))
            .await
            .expect("the store reads")
            .expect("the first test week is in force");
        store
            .record_change(
                first.id,
                &FillChange {
                    slot: SlotId::Core,
                    intensity: None,
                    from: date(2026, 9, 16),
                    recorded_at: "2026-09-16T08:00:00Z".parse().expect("an instant"),
                    exercise: Exercise::Reps(RepsExercise::DeadBug),
                    assisted_by: None,
                    loaded_by: Some(Implement::Dumbbell),
                },
            )
            .await
            .expect("the change is recorded");

        let second = store
            .on(date(2026, 9, 30))
            .await
            .expect("the store reads")
            .expect("the second test week is in force");
        let core = |role| match second
            .mesocycle
            .clone()
            .changed_on(&second.changes, date(2026, 9, 30))
            .fills()
            .content(SlotId::Core, role)
        {
            SlotContent::Single(exercise) => Some(*exercise),
            SlotContent::Static(_) => None,
        };
        for role in SessionRole::ALL {
            assert_eq!(
                core(*role),
                Some(Exercise::Reps(RepsExercise::DeadBug)),
                "the change made in the first week holds in the second"
            );
        }
        assert_eq!(
            second
                .changes
                .loaded_by(SlotId::Core, SessionRole::ALL[0], date(2026, 9, 30)),
            Some(Implement::Dumbbell),
            "and so does what loads it"
        );
    });
}
