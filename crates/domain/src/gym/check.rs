//! What in the gym record may need correcting, one kind at a time (#896).
//!
//! **Each kind is a rule written for it**, so what is found grows one kind at
//! a time, and nothing here says a record *is* wrong. The operator,
//! 2026-11-24: what to look for is *"the raw or normalised data that you
//! thought was fishy"*, and *"things I've told you in previous
//! conversations"*. He looks at each, and changes it or says it is right.

use std::fmt;

use jiff::civil::{Date, date};

use super::{
    CanonicalExercise, CanonicalGymSession, Load, Performed, SetKind, exercise::Implement,
    exercise::RepsExercise,
};
use crate::{canonical::Attributed, measure::Kg};

/// Which rule found it. Persisted, with what he said is right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A barbell lift loaded in dumbbell steps.
    DumbbellLoad,
    /// A load that halved or doubled from the session before.
    HalvedOrDoubled,
    /// An exercise logged after he said he had switched from it.
    Switched,
}

impl Kind {
    /// Every member, in the order the page lists them.
    pub const ALL: [Self; 3] = [Self::DumbbellLoad, Self::HalvedOrDoubled, Self::Switched];

    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DumbbellLoad => "dumbbell-load",
            Self::HalvedOrDoubled => "halved-or-doubled",
            Self::Switched => "switched",
        }
    }
}

/// Why a kind could not be read back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name a kind of check")]
pub struct UnknownKind {
    value: String,
}

impl TryFrom<String> for Kind {
    type Error = UnknownKind;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .iter()
            .find(|kind| kind.as_str() == value)
            .copied()
            .ok_or(UnknownKind { value })
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

crate::newtype::from_str_via_string!(Kind, UnknownKind);

/// A switch from one exercise to another that the operator has stated, and
/// from when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    /// What he went on logging.
    pub logged: RepsExercise,
    /// The first day it may not have been what was done.
    pub from: Date,
    /// What it may have been instead.
    pub to: &'static [RepsExercise],
}

/// The switches he has stated.
///
/// **Pull-ups to neutral-grip pull-ups**, the operator, 2026-11-24: *"It was
/// the winter of 2024-2025, before I left the CrossFit gym. I started to
/// replace their prescribed pull ups with neutral grip pull ups or neutral
/// grip lat pull downs."* From the first day of that winter. The vocabulary
/// holds no neutral-grip lat pulldown, so only the pull-up is offered.
pub const SWITCHES: [Switch; 1] = [Switch {
    logged: RepsExercise::PullUp,
    from: date(2024, 12, 1),
    to: &[RepsExercise::NeutralGripPullUp],
}];

/// How a load on dumbbell steps reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dumbbells {
    /// A multiple of 4 kg: a pair, each half of it. The operator, 2026-11-24:
    /// *"Multiples of 4kg are even more of a tell 32, 36, 44 are 2x 16, 2x 18,
    /// and 2x 22."*
    Pair { total: Kg, each: Kg },
    /// A multiple of 2 kg and not of 4: one dumbbell.
    One(Kg),
}

/// What makes a record doubtful, and the figures that do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Doubt {
    /// A barbell lift whose working loads are not in 2.5 kg steps but in a
    /// dumbbell's. The operator, 2026-11-24: *"increments of 2.5kg, generally
    /// a sign of barbells and machines, to 2kg (for a single arm) of 4kg (for
    /// both arms), generally a sign of using dumbbells."*
    DumbbellLoad(Vec<Dumbbells>),
    /// The heaviest working load is about half or double the one in the
    /// session before, which was on `on`. The operator, 2026-11-24: *"drops in
    /// weight from x to ~x/2"*, a dumbbell logged per arm one session and as
    /// the pair the next.
    HalvedOrDoubled { before: Kg, on: Date, now: Kg },
    /// Logged as what he had stopped doing.
    Switched(Switch),
}

impl Doubt {
    pub const fn kind(&self) -> Kind {
        match self {
            Self::DumbbellLoad(_) => Kind::DumbbellLoad,
            Self::HalvedOrDoubled { .. } => Kind::HalvedOrDoubled,
            Self::Switched(_) => Kind::Switched,
        }
    }
}

/// One exercise of one session that may need correcting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// Which of the sessions given, by index.
    pub session: usize,
    /// Which of its exercises, from one, in the order performed.
    pub position: usize,
    pub exercise: RepsExercise,
    pub doubt: Doubt,
}

/// The longest gap over which a halving or doubling is read as one: longer
/// than this, and the load may have dropped because training did.
const DAYS_APART: i32 = 6 * 7;

/// A step on a barbell.
const BARBELL_STEP: u64 = 2_500;

/// Everything the rules find in `sessions`, oldest session first, and within
/// one session in the order its exercises were performed.
pub fn find(sessions: &[CanonicalGymSession]) -> Vec<Found> {
    let mut order: Vec<usize> = (0..sessions.len()).collect();
    order.sort_by_key(|index| {
        sessions.get(*index).map(|session| {
            let occurred = session.occurred();
            (
                occurred.day(),
                occurred
                    .instant()
                    .map(crate::normalised::StartedAt::instant),
            )
        })
    });
    let mut found = Vec::new();
    let mut last: Vec<(RepsExercise, Date, Kg)> = Vec::new();
    for index in order {
        let Some(session) = sessions.get(index) else {
            continue;
        };
        let day = session.occurred().day();
        for (position, lift, loads) in lifts(session) {
            let at = |doubt| Found {
                session: index,
                position,
                exercise: lift,
                doubt,
            };
            if let Some(doubt) = dumbbell_load(lift, &loads) {
                found.push(at(doubt));
            }
            if let Some(heaviest) = loads.iter().map(|load| load.kg).max() {
                let before = last.iter().find(|(held, _, _)| *held == lift).copied();
                if let Some(doubt) =
                    before.and_then(|(_, on, before)| halved_or_doubled(before, on, heaviest, day))
                {
                    found.push(at(doubt));
                }
                last.retain(|(held, _, _)| *held != lift);
                last.push((lift, day, heaviest));
            }
            for switch in SWITCHES {
                if switch.logged == lift && day >= switch.from {
                    found.push(at(Doubt::Switched(switch)));
                }
            }
        }
    }
    found
}

/// Each lift of a session once, at the first place it was performed, with
/// the working loads of every place it was: a sheet may log one lift as
/// several exercises, and a session is not compared with itself.
fn lifts(session: &CanonicalGymSession) -> Vec<(usize, RepsExercise, Vec<Working>)> {
    let mut lifts: Vec<(usize, RepsExercise, Vec<Working>)> = Vec::new();
    for (offset, exercise) in session.exercises().enumerate() {
        let Some((lift, loads)) = working_loads(exercise) else {
            continue;
        };
        match lifts.iter_mut().find(|(_, held, _)| *held == lift) {
            Some((_, _, held)) => held.extend(loads),
            None => lifts.push((offset + 1, lift, loads)),
        }
    }
    lifts
}

/// An exercise done for reps, named, and the loads of its completed working
/// sets that had one.
fn working_loads(exercise: &CanonicalExercise) -> Option<(RepsExercise, Vec<Working>)> {
    let CanonicalExercise::ForReps { identified, sets } = exercise else {
        return None;
    };
    let lift = identified.copied().exercise()?;
    let loads = sets
        .iter()
        .filter(|set| set.kind.as_ref().map(Attributed::copied) != Some(SetKind::Warmup))
        .filter(|set| matches!(set.outcome.value(), Performed::Completed(_)))
        .filter_map(|set| {
            let load = set.load.as_ref()?;
            let Load::Absolute(kg) = load.copied() else {
                return None;
            };
            // Only a watch times a set, so a load from the account that
            // timed it is the watch's.
            let dialled = load.normalised().is_some()
                && load.normalised() == set.began.as_ref().and_then(Attributed::normalised);
            (!kg.is_none()).then_some(Working { kg, dialled })
        })
        .collect();
    Some((lift, loads))
}

/// One completed working set's load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Working {
    kg: Kg,
    /// Dialled into a watch, which takes whole kilogrammes only. The
    /// operator, 2026-11-24: *"We can't trust sessions that were only
    /// recorded by the garmin watch, those weights were literally recorded
    /// using the watch's UI, which didn't allow for decimals. n8kg is almost
    /// certainly n7.5kg, same with n3kg, it's really n2.5kg."* So its steps
    /// say nothing about the implement.
    dialled: bool,
}

/// The loads of a barbell lift on a dumbbell's steps, each once.
fn dumbbell_load(lift: RepsExercise, loads: &[Working]) -> Option<Doubt> {
    if lift.implement() != Implement::Barbell {
        return None;
    }
    let mut read: Vec<Dumbbells> = Vec::new();
    for load in loads.iter().filter(|load| !load.dialled) {
        let load = &load.kg;
        let grams = load.as_grams();
        if grams % BARBELL_STEP == 0 {
            continue;
        }
        let dumbbells = if grams % 4_000 == 0 {
            Dumbbells::Pair {
                total: *load,
                each: Kg::from_grams(grams / 2),
            }
        } else if grams % 2_000 == 0 {
            Dumbbells::One(*load)
        } else {
            continue;
        };
        if !read.contains(&dumbbells) {
            read.push(dumbbells);
        }
    }
    (!read.is_empty()).then_some(Doubt::DumbbellLoad(read))
}

/// Whether `now` is about half or double `before`: within a tenth of it, and
/// never more than one barbell step either way.
fn halved_or_doubled(before: Kg, on: Date, now: Kg, day: Date) -> Option<Doubt> {
    let apart = on.until(day).ok()?.get_days();
    if apart > DAYS_APART {
        return None;
    }
    let near = |expected: u64| {
        let tolerance = (expected / 10).min(BARBELL_STEP);
        now.as_grams().abs_diff(expected) <= tolerance
    };
    let grams = before.as_grams();
    (near(grams / 2) || near(grams.saturating_mul(2))).then_some(Doubt::HalvedOrDoubled {
        before,
        on,
        now,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use jiff::civil::{Date, date};

    use super::{Doubt, Dumbbells, Found, SWITCHES, find};
    use crate::{
        canonical::{Attributed, NormalisedSessionId, Occurred},
        gym::{
            CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
            Performed, SetKind, exercise::RepsExercise,
        },
        measure::{Kg, RepCount},
        sequence::NonEmpty,
    };

    const ACCOUNT: NormalisedSessionId = NormalisedSessionId::FIRST;

    fn kg(whole: u64, grams: u64) -> Kg {
        Kg::from_grams(whole * 1_000 + grams)
    }

    fn set(load: Kg, kind: SetKind) -> CanonicalSet<RepCount> {
        CanonicalSet {
            outcome: Attributed::new(
                Performed::Completed(Some(RepCount::new(6).expect("six"))),
                ACCOUNT,
            ),
            load: Some(Attributed::new(Load::Absolute(load), ACCOUNT)),
            began: None,
            intensity: None,
            kind: Some(Attributed::new(kind, ACCOUNT)),
            rest_after: None,
            sources: BTreeSet::new(),
        }
    }

    fn exercise(lift: RepsExercise, loads: &[Kg]) -> CanonicalExercise {
        CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Recorded(lift), ACCOUNT),
            sets: NonEmpty::new(
                loads
                    .iter()
                    .map(|load| set(*load, SetKind::Working))
                    .collect(),
            )
            .expect("a set"),
        }
    }

    fn session(on: Date, exercises: Vec<CanonicalExercise>) -> CanonicalGymSession {
        CanonicalGymSession::new(
            Attributed::new(Occurred::On(on), ACCOUNT),
            NonEmpty::new(exercises.into_iter().map(CanonicalItem::exercise).collect())
                .expect("an exercise"),
            None,
            None,
            Vec::new(),
        )
    }

    fn doubts(found: &[Found]) -> Vec<(usize, usize, Doubt)> {
        found
            .iter()
            .map(|found| (found.session, found.position, found.doubt.clone()))
            .collect()
    }

    /// **His lying triceps extensions from 23 October 2026**: 36 kg on a
    /// barbell reads as two 18 kg dumbbells, 6 kg of wrist extension as one
    /// dumbbell, and front squat at 92.5 kg as nothing to check.
    #[test]
    fn a_barbell_lift_on_dumbbell_steps_is_found() {
        let found = find(&[session(
            date(2026, 10, 23),
            vec![
                exercise(RepsExercise::FrontSquatBarbell, &[kg(92, 500), kg(80, 0)]),
                exercise(
                    RepsExercise::LyingOverheadTricepsExtensionBarbell,
                    &[kg(36, 0), kg(36, 0), kg(40, 0)],
                ),
                exercise(RepsExercise::SeatedWristExtensionBarbell, &[kg(6, 0)]),
            ],
        )]);

        assert_eq!(
            doubts(&found),
            vec![
                (
                    0,
                    2,
                    Doubt::DumbbellLoad(vec![Dumbbells::Pair {
                        total: kg(36, 0),
                        each: kg(18, 0)
                    }])
                ),
                (0, 3, Doubt::DumbbellLoad(vec![Dumbbells::One(kg(6, 0))])),
            ]
        );
    }

    /// **A warm-up is not read**: an empty-bar 20 kg is on 2.5 kg steps
    /// anyway, but a 14 kg warm-up with a fixed bar would not be.
    #[test]
    fn a_warm_up_is_not_read() {
        let mut lift = exercise(RepsExercise::BenchPressBarbell, &[kg(60, 0)]);
        if let CanonicalExercise::ForReps { sets, .. } = &mut lift {
            *sets = NonEmpty::new(vec![
                set(kg(14, 0), SetKind::Warmup),
                set(kg(60, 0), SetKind::Working),
            ])
            .expect("sets");
        }
        assert_eq!(find(&[session(date(2026, 10, 23), vec![lift])]), vec![]);
    }

    /// **Halved or doubled within six weeks**: his dumbbell preacher curl at
    /// 10 kg on 13 April 2026 and 20 kg on 17 April, and the cable triceps
    /// extension at 36 kg then 17.5 kg; but not back squat at 90 kg in 2019
    /// and 50 kg in 2023, nor 5 kg then 3 kg, which is not near half.
    #[test]
    fn a_load_that_halves_or_doubles_is_found() {
        let curl = RepsExercise::PreacherCurlDumbbell;
        let triceps = RepsExercise::SingleArmOverheadTricepsExtensionCable;
        let squat = RepsExercise::BackSquatBarbell;
        let crunch = RepsExercise::Crunch;
        let found = find(&[
            session(date(2019, 3, 22), vec![exercise(squat, &[kg(90, 0)])]),
            session(date(2023, 10, 16), vec![exercise(squat, &[kg(50, 0)])]),
            session(
                date(2025, 1, 20),
                vec![
                    exercise(triceps, &[kg(36, 0)]),
                    exercise(crunch, &[kg(5, 0)]),
                ],
            ),
            session(
                date(2025, 1, 22),
                vec![
                    exercise(triceps, &[kg(17, 500)]),
                    exercise(crunch, &[kg(3, 0)]),
                ],
            ),
            session(date(2026, 4, 13), vec![exercise(curl, &[kg(10, 0)])]),
            session(date(2026, 4, 17), vec![exercise(curl, &[kg(20, 0)])]),
        ]);

        assert_eq!(
            doubts(&found),
            vec![
                (
                    3,
                    1,
                    Doubt::HalvedOrDoubled {
                        before: kg(36, 0),
                        on: date(2025, 1, 20),
                        now: kg(17, 500)
                    }
                ),
                (
                    5,
                    1,
                    Doubt::HalvedOrDoubled {
                        before: kg(10, 0),
                        on: date(2026, 4, 13),
                        now: kg(20, 0)
                    }
                ),
            ]
        );
    }

    /// **Sessions are read oldest first whatever order they are given in**,
    /// so the session before is the one before in time.
    #[test]
    fn sessions_are_read_in_time_order() {
        let curl = RepsExercise::PreacherCurlDumbbell;
        let found = find(&[
            session(date(2026, 4, 17), vec![exercise(curl, &[kg(20, 0)])]),
            session(date(2026, 4, 13), vec![exercise(curl, &[kg(10, 0)])]),
        ]);
        assert_eq!(
            found.iter().map(|found| found.session).collect::<Vec<_>>(),
            vec![0]
        );
    }

    /// **A load the watch dialled says nothing about the implement**: 2018's
    /// 68 kg back squat, which only the watch recorded, is 67.5 on a bar.
    #[test]
    fn a_load_dialled_into_the_watch_is_not_read_for_dumbbells() {
        let zone =
            crate::normalised::OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        let at = date(2018, 5, 2)
            .at(7, 0, 0, 0)
            .to_zoned_datetime(zone.as_time_zone())
            .expect("a time")
            .timestamp();
        let mut dialled = set(kg(68, 0), SetKind::Working);
        dialled.began = Some(Attributed::new(
            crate::normalised::StartedAt::new(at, zone),
            ACCOUNT,
        ));
        let squat = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Recorded(RepsExercise::BackSquatBarbell),
                ACCOUNT,
            ),
            sets: NonEmpty::new(vec![dialled]).expect("a set"),
        };
        assert_eq!(find(&[session(date(2018, 5, 2), vec![squat])]), vec![]);
    }

    /// **A lift logged twice in one session is one entry**, at its first
    /// place, and is not compared with itself: 2018's bench press at 45 kg
    /// and then 23 kg in one session halves nothing, and its 32 kg is read
    /// with the first.
    #[test]
    fn a_lift_is_read_once_per_session() {
        let bench = RepsExercise::BenchPressBarbell;
        let found = find(&[session(
            date(2018, 7, 30),
            vec![
                exercise(bench, &[kg(45, 0)]),
                exercise(RepsExercise::FrontSquatBarbell, &[kg(60, 0)]),
                exercise(bench, &[kg(23, 0), kg(32, 0)]),
            ],
        )]);
        assert_eq!(
            doubts(&found),
            vec![(
                0,
                1,
                Doubt::DumbbellLoad(vec![Dumbbells::Pair {
                    total: kg(32, 0),
                    each: kg(16, 0)
                }])
            )]
        );
    }

    /// **A pull-up from 1 December 2024 may have been neutral-grip**, and one
    /// before it is not doubted.
    #[test]
    fn a_pull_up_after_the_switch_is_found() {
        let found = find(&[
            session(
                date(2024, 11, 29),
                vec![exercise(RepsExercise::PullUp, &[kg(0, 0)])],
            ),
            session(
                date(2024, 12, 11),
                vec![
                    exercise(RepsExercise::FrontSquatBarbell, &[kg(60, 0)]),
                    exercise(RepsExercise::PullUp, &[kg(0, 0)]),
                ],
            ),
        ]);
        let [switch] = SWITCHES;
        assert_eq!(doubts(&found), vec![(1, 2, Doubt::Switched(switch))]);
    }
}
