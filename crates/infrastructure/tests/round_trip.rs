//! The round trip: a performance read as a prescription, and compared with one.
//!
//! SC-010, and research D9 is the design. The projection is a `domain` function,
//! so what needs an adapter suite is the other two thirds: reading a whole
//! `GymWorkout` back out of the five tables it was written to, and generating a
//! prescription for the same date to compare it against.
//!
//! **This is a diagnostic, not a reproduction test.** Nothing in the corpus was
//! issued — it records a programme run by hand whose template changed while it
//! ran and whose arithmetic was sometimes wrong — so comparing a projection
//! against a *regenerated* prescription says where the model and the history part
//! company. Each parting must be attributable; none of them is a failure of
//! generation unless it falls outside the named causes. Asserting agreement here
//! would make reproducing human error a requirement.

mod support;

use application::{
    PerformedWorkoutReader as _, WorkoutPrescriber as _,
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::{
    gym::{GymWorkout, Kg, Load, NonEmpty, RepCount},
    prescription::{
        Divergence, PrescribedExercise, PrescribedItem, PrescribedSet, SlotId, Target,
        WorkoutShape, project, satisfies,
    },
};
use infrastructure::{
    SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePerformedWorkoutReader,
    SqlitePrescribedWorkoutStore, SqliteProgrammeStore,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, store};

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
>;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The store, a reader over it and a prescriber against it.
async fn ready() -> Fallible<(SqlitePerformedWorkoutReader, Prescriber, tempfile::TempDir)> {
    let (directory, pool): (tempfile::TempDir, SqlitePool) = store::derived_and_authored().await?;
    let reader = SqlitePerformedWorkoutReader::new(pool.clone());
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool, "Europe/London".to_owned()),
    });
    Ok((reader, prescriber, directory))
}

macro_rules! ready {
    () => {
        match corpus::block_on(ready()) {
            Ok(Ok(ready)) => ready,
            Ok(Err(error)) => panic!("the corpus lands, derives and authors: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the operation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// The one workout performed on a date.
///
/// A macro rather than a function for the reason `run!` is one: `panic` is
/// `forbid` and clippy's test exemption covers a `#[test]` body, not a helper
/// defined alongside it. Fragmentation would show up here as more than one
/// workout, and does not on these dates.
macro_rules! performance {
    ($reader:expr, $date:expr) => {{
        let workouts: Vec<GymWorkout> = run!($reader.between($date, $date));
        match workouts.into_iter().next() {
            Some(workout) => workout,
            None => panic!("{} has a performed session", $date),
        }
    }};
}

/// A date's performance projected, against the prescription generated for it.
macro_rules! compare {
    ($reader:expr, $prescriber:expr, $date:expr) => {{
        let performed: GymWorkout = performance!($reader, $date);
        let issued = run!($prescriber.prescribe($date));
        satisfies(&project(&performed).shape, issued.workout.shape())
    }};
}

/// Every session the corpus holds, whatever else is wrong with it.
const fn all_of_time() -> (Date, Date) {
    (Date::constant(2000, 1, 1), Date::constant(2030, 12, 31))
}

/// The sessions the fixture block covers: eight weeks from 2026-07-06, on the
/// Mondays and Fridays the programme runs.
const IN_BLOCK: [Date; 8] = [
    Date::constant(2026, 7, 6),
    Date::constant(2026, 7, 10),
    Date::constant(2026, 7, 13),
    Date::constant(2026, 7, 17),
    Date::constant(2026, 7, 20),
    Date::constant(2026, 8, 3),
    Date::constant(2026, 8, 7),
    Date::constant(2026, 8, 10),
];

/// SC-010a: every performed workout projects into a prescription shape.
///
/// **Totality is the claim, not well-formedness.** 163 normalised workouts —
/// front squats, runs, sled pushes, a session fragmented across four landing
/// records — and none of them fails to produce a shape. That is the model's own
/// invariant: if a performance could fail to project, the comparison SC-010b and
/// SC-010c rest on would be conditional on the data.
///
/// **Fourteen sessions since 15 June, where the criterion says fifteen.** The
/// fifteenth is 2026-08-14 and it is not in this corpus: the fixture was fetched
/// on 2026-08-12, so the session the spec and `contracts/programme.md` describe
/// postdates every record here. Recorded rather than adjusted for — the count in
/// the spec is right about the operator's block and wrong about the fixture.
#[test]
fn every_session_projects() {
    let (reader, _prescriber, _directory) = ready!();
    let (from, to) = all_of_time();
    let all: Vec<GymWorkout> = run!(reader.between(from, to));

    assert_eq!(all.len(), 163, "the corpus derives 163 workouts");
    for workout in &all {
        let projection = project(workout);
        assert_eq!(
            projection.shape.items().count(),
            workout.items().count() - projection.gaps.len(),
            "every item is either in the shape or accounted for as a gap: {}",
            workout.started_at().wall_clock().date()
        );
        assert!(
            projection.shape.set_count() > 0,
            "a projected session prescribes something"
        );
    }

    let since_june = run!(reader.between(Date::constant(2026, 6, 15), to));
    assert_eq!(
        since_june.len(),
        14,
        "fourteen sessions from 15 June to the end of the corpus"
    );
}

/// SC-010d: satisfaction is direction-aware.
///
/// A performed six satisfies a prescribed four-to-six; a prescribed six is not
/// satisfied by a performed four-to-six. Only one of the two is an instruction,
/// which is why equality on `WorkoutShape` is the wrong relation and this is a
/// relation rather than a comparison.
#[test]
fn satisfaction_is_direction_aware() {
    let (Ok(four), Ok(six)) = (reps(4), reps(6)) else {
        panic!("both are repetition counts")
    };
    let (Ok(exact), Ok(span)) = (shape_of(Target::Exactly(six)), Target::range(four, six)) else {
        panic!("an exact six and a four-to-six range both build")
    };
    let Ok(range) = shape_of(span) else {
        panic!("a shape is built from the range")
    };

    assert_eq!(
        satisfies(&exact, &range),
        Vec::new(),
        "a performed six satisfies a prescribed four-to-six"
    );
    assert!(
        !satisfies(&range, &exact).is_empty(),
        "a performed four-to-six does not satisfy a prescribed six"
    );
    // And the ordinary case still holds in both directions.
    assert_eq!(satisfies(&exact, &exact), Vec::new());
}

fn reps(count: u32) -> Fallible<RepCount> {
    Ok(RepCount::new(count)?)
}

/// A one-item shape carrying one set, so a measure can be compared in isolation.
fn shape_of(measure: Target<RepCount>) -> Fallible<WorkoutShape> {
    let set = PrescribedSet::fixed(Load::Absolute(Kg::from_grams(60_000)), measure);
    Ok(WorkoutShape::new(NonEmpty::of(
        PrescribedItem::Exercise {
            slot: SlotId::KneeDominant,
            exercise: PrescribedExercise::ForReps {
                exercise: domain::gym::exercise::RepsExercise::try_from("front-squat".to_owned())?,
                sets: NonEmpty::of(set, Vec::new()),
            },
        },
        Vec::new(),
    )))
}

/// SC-010b: generation reproduces the structure of the record.
///
/// Blocks, order, grouping and slots — not loads, which SC-010c takes. Three
/// things are asserted, and the third is the interesting one.
///
/// **No slot is ever assigned differently.** The projection assigns slots by
/// position against the template's issue order and generation assigns them from
/// the programme, and across all eight in-block sessions the two never disagree.
/// That is what makes the positional assignment usable rather than a guess.
///
/// **No measure is ever counted in a different thing.** A repetition count
/// compared against seconds would be a real defect in generation, and there is
/// none.
///
/// **The structure converges.** From 2026-07-20 the record and the template agree
/// exactly. Before it they differ only in the mobility block, where the record
/// grouped three holds into one superset and the template issues a hold and a
/// stretch — which `contracts/programme.md` records as variance the template does
/// not model.
#[test]
fn generation_reproduces_the_structure_of_the_record() {
    let (reader, prescriber, _directory) = ready!();

    for date in IN_BLOCK {
        let divergences = compare!(&reader, &prescriber, date);
        let structural: Vec<&Divergence> = divergences
            .iter()
            .filter(|divergence| {
                matches!(
                    divergence,
                    Divergence::ItemCount { .. }
                        | Divergence::Grouping { .. }
                        | Divergence::Slot { .. }
                        | Divergence::MeasureKind { .. }
                )
            })
            .collect();

        for divergence in &structural {
            assert!(
                !matches!(divergence, Divergence::Slot { .. }),
                "{date}: a slot was assigned differently: {divergence}"
            );
            assert!(
                !matches!(divergence, Divergence::MeasureKind { .. }),
                "{date}: a measure was prescribed in the wrong currency: {divergence}"
            );
        }

        if date >= Date::constant(2026, 7, 20) {
            assert!(
                structural.is_empty(),
                "{date}: the record and the template agree structurally from 20 July: {structural:?}"
            );
        }
    }
}

/// SC-010c: every divergence is attributable.
///
/// **From 2026-08-07 onward**, which is where the record and the template have
/// converged structurally, so what is left is loads, counts and fills. Each one
/// must fall into a named cause; one that does not is a defect in generation
/// rather than a fact about the record.
///
/// **Hand arithmetic is not distinguishable from an unstated parameter yet**, and
/// the spec's third bucket is folded into the first for that reason. Telling them
/// apart means comparing against the span the operator actually ran, and that span
/// is the one value the authored document still leaves open (D8, T080). Recorded
/// rather than guessed at: a divergence of 2.5kg could be either until the
/// intended number exists to compare against.
#[test]
fn divergences_from_the_record_are_attributable() {
    let (reader, prescriber, _directory) = ready!();

    for date in IN_BLOCK
        .into_iter()
        .filter(|date| *date >= Date::constant(2026, 8, 7))
    {
        let performed: GymWorkout = performance!(&reader, date);
        let projection = project(&performed);
        let divergences = compare!(&reader, &prescriber, date);
        assert!(
            !divergences.is_empty(),
            "{date}: the record was hand-run, so something must differ"
        );

        for divergence in &divergences {
            let Some(cause) = attribute(divergence, &projection.shape) else {
                panic!("{date}: unattributable, so a defect in generation: {divergence}");
            };
            // Named so a reader of a failure sees the bucket, not just the count.
            assert!(
                matches!(
                    cause,
                    Cause::UpperPairOrder | Cause::AlternatingFill | Cause::UnstatedParameter
                ),
                "{date}: {divergence} attributed to {cause:?}"
            );
        }
    }
}

/// Why a divergence is not a defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cause {
    /// The record supersets the upper pair pull-first; the template names push
    /// first, because `PrimaryPattern` orders them that way. A fact about the
    /// template's declared order rather than about either session.
    UpperPairOrder,
    /// A slot whose fill the record alternates between sessions. The hip-dominant
    /// slot has no single hinge accessory — the operator stated this on
    /// 2026-08-18 — and the forearm and arm slots rotate the same way.
    AlternatingFill,
    /// The ladder span and the accessory schemes the record was run under are not
    /// in the authored document, so every load and every count derives from the
    /// fixture's test values instead.
    UnstatedParameter,
}

/// Which slot a divergence sits at, so a cause can be decided by locus rather
/// than only by kind.
fn slot_at(shape: &WorkoutShape, at: usize, member: usize) -> Option<SlotId> {
    shape
        .items()
        .iter()
        .nth(at)
        .and_then(|item| item.slots().nth(member))
}

fn attribute(divergence: &Divergence, performed: &WorkoutShape) -> Option<Cause> {
    match divergence {
        Divergence::Exercise { at, member, .. } => match slot_at(performed, at.as_usize(), *member)
        {
            Some(SlotId::UpperPush | SlotId::UpperPull) => Some(Cause::UpperPairOrder),
            Some(
                SlotId::HipDominant
                | SlotId::Arms
                | SlotId::Forearms
                | SlotId::Core
                | SlotId::MobilityHold
                | SlotId::MobilityStretch,
            ) => Some(Cause::AlternatingFill),
            _ => None,
        },
        Divergence::Load { .. } | Divergence::Measure { .. } | Divergence::SetCount { .. } => {
            Some(Cause::UnstatedParameter)
        }
        // A structural divergence this late is not attributable: the record and
        // the template have converged by now, so one would be a defect.
        Divergence::ItemCount { .. }
        | Divergence::Grouping { .. }
        | Divergence::Slot { .. }
        | Divergence::MeasureKind { .. } => None,
    }
}
