//! Committing the next concurrent mesocycle when the one before it ends (#222).
//!
//! **One at a time.** The plan holds what has been committed and nothing past
//! it: each mesocycle is chosen when the one before it ends, because only then
//! is its entry test known. The macrocycle says which
//! ([`Macrocycle::next`]); this reads what it needs to ask, builds both halves
//! and appends them to the plan in force.
//!
//! **Here rather than in `application`, for now**, because building a cycling
//! mesocycle reads Peloton through the same code the `plan` wizard does, and
//! that code is this crate's.

use std::num::NonZeroU32;

use application::{
    DiaryStore as _, PlanStore as _,
    reschedule::{Reschedule, ReschedulingPlans},
};
use domain::{
    cycling::{CyclingMesocycle, CyclingMicrocycle, PlannedRide},
    macrocycle::{Concurrent, Cycling, Gym, Macrocycle},
    normalised::OperatorZone,
    plan::{Plan, Span},
    planner,
    prescription::{
        GymMesocycle, Progression,
        authored::{Authored, Shape},
        schedule::Skip,
    },
    provider::{ExternalProgramme, ProvidedFrom},
    schedule::{DayPart, Diary, Discipline, Relative, SessionRole},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteDiaryStore, SqliteGenerationParameterStore, SqlitePlanStore, SqlitePool,
    SqliteRiddenVenues,
    peloton::{PelotonClasses, PelotonHoldingRides, skeleton},
};
use jiff::civil::Date;

use crate::{Failure, exit, holidays, plan, rescheduling, wizard};

/// The harder session of a week, and so the one a test replaces.
const HARDER: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);
/// The easier one.
const EASIER: SessionRole = SessionRole::new(Relative::Lower, Relative::Higher);

/// Published cycling programmes, by the names Peloton gives them.
const BASE: &str = "Boost Your Base";
const BUILD: &str = "Build Your Power Zones";
const PEAK: &str = "Peak Your Power Zones";

/// Microcycles and cycling sessions in a committed mesocycle: the shape the
/// `plan` wizard asks for by default.
const MICROCYCLES: usize = 4;
const SESSIONS: usize = 2;

/// A concurrent mesocycle the macrocycle has chosen, before it is built.
///
/// **Deciding reads only the store and the calendar**, so what is due can
/// always be said. Building it reads Peloton, which may not answer (§ 36).
pub struct Due {
    plan: Plan,
    diary: Diary,
    pub concurrent: Concurrent,
    pub span: Span,
}

/// The next concurrent mesocycle, if the one before it has ended.
///
/// `None` while the current one is still running, where no plan is in force
/// this macrocycle, where the plan states no chain, or where the term has
/// nothing left.
///
/// # Errors
///
/// [`Failure`] if the store or the calendar cannot be read.
pub async fn due(
    pool: &SqlitePool,
    zone: &OperatorZone,
    now: DayPart,
) -> Result<Option<Due>, Failure> {
    let holidays = holidays::read().await?;
    let Some(macrocycle) = Macrocycle::on(now.date, &holidays) else {
        return Ok(None);
    };

    let standing = rescheduling::standing(pool, zone, now).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    let plans = ReschedulingPlans::new(
        SqlitePlanStore::new(pool.clone(), zone.clone()),
        Reschedule::new(standing.reruns, diary.clone()),
    );
    let Some(plan) = in_force(&plans, &macrocycle).await? else {
        return Ok(None);
    };
    let Some(chain) = plan.chain() else {
        return Ok(None);
    };

    // **Due once the plan has run out**, and from this Monday if it ran out
    // earlier: a week nothing was committed for is not given back.
    let monday = planner::commencing(now.date);
    let end = plan.span().end();
    if end > monday {
        return Ok(None);
    }
    let start = end.max(monday);

    let previous = plan
        .cycling()
        .and_then(|programme| programme.mesocycles().last())
        .filter(|last| last.start() >= macrocycle.start())
        .and_then(cycling_of);
    Ok(macrocycle
        .next(chain, previous, start)
        .map(|concurrent| Due {
            plan,
            diary,
            concurrent,
            span: Span::new(start, concurrent.weeks()),
        }))
}

/// Build what is due and add it to the plan.
///
/// # Errors
///
/// [`Failure`] if Peloton or the store cannot be reached, or if what the
/// macrocycle chose will not build. Nothing is written unless both halves
/// built.
pub async fn commit(
    pool: &SqlitePool,
    zone: &OperatorZone,
    due: &Due,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let start = due.span.start();
    let gym = gym_side(
        pool,
        zone,
        &due.plan,
        &due.diary,
        due.concurrent.gym(),
        start,
    )
    .await?;
    let (classes, _) = plan::peloton(credentials)?;
    let cycling = cycling_side(pool, &classes, due.concurrent.cycling(), start).await?;

    SqlitePlanStore::new(pool.clone(), zone.clone())
        .commit(due.plan.name(), &gym, &cycling)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))
}

/// The plan this macrocycle is following: the latest to start inside it.
async fn in_force(
    plans: &ReschedulingPlans<SqlitePlanStore>,
    macrocycle: &Macrocycle,
) -> Result<Option<Plan>, Failure> {
    let store = |error: application::StoreError| Failure::message(error.to_string(), exit::STORE);
    let windows = plans.windows().await.map_err(store)?;
    let Some(window) = windows
        .iter()
        .filter(|window| {
            let start = window.span().start();
            start >= macrocycle.start() && start <= macrocycle.last()
        })
        .max_by_key(|window| window.span().start())
    else {
        return Ok(None);
    };
    plans.named(window.name()).await.map_err(store)
}

/// Which cycling mesocycle a committed one is, from where it was taken.
///
/// **By the publisher's name**, which is Peloton's word and so lives here with
/// the other Peloton names rather than in `domain`. A mesocycle assembled here
/// is a hold's holding weeks; the published test week is a hold with nothing
/// held.
fn cycling_of(mesocycle: &CyclingMesocycle) -> Option<Cycling> {
    let Some(provided) = mesocycle.provided_from() else {
        let holding = u32::try_from(mesocycle.duration_weeks()).ok()?;
        return Some(Cycling::Hold { holding });
    };
    let second_half = provided.microcycles().next().is_some_and(|first| first > 4);
    match provided.programme().name().as_str() {
        skeleton::POWER_ZONE_TEST => Some(Cycling::Hold { holding: 0 }),
        BUILD => Some(Cycling::Build),
        BASE if second_half => Some(Cycling::Base2),
        BASE => Some(Cycling::Base1),
        PEAK if second_half => Some(Cycling::Peak2),
        PEAK => Some(Cycling::Peak1),
        _ => None,
    }
}

/// The published programme a gym mesocycle was taken from.
fn published(mesocycle: &GymMesocycle) -> Option<ExternalProgramme> {
    match mesocycle {
        GymMesocycle::Test(test) => test.provided().map(|from| from.programme().clone()),
        GymMesocycle::Progression(Progression::Provided { from, .. }) => {
            Some(from.programme().clone())
        }
        GymMesocycle::Progression(_) => None,
    }
}

/// The gym's half: SBS, or a hold's linear holding weeks and its test week.
///
/// **What does not change is carried from the mesocycle before**: the lift,
/// its slot, the accessories filling the rest, and the published programme.
/// The plan was stated once; nothing here asks again.
async fn gym_side(
    pool: &SqlitePool,
    zone: &OperatorZone,
    plan: &Plan,
    diary: &Diary,
    gym: Gym,
    start: Date,
) -> Result<Vec<GymMesocycle>, Failure> {
    let before = plan
        .gym()
        .and_then(|programme| programme.mesocycles().last())
        .ok_or_else(|| {
            Failure::message(
                "the plan holds no gym mesocycle to carry on from",
                exit::STORE,
            )
        })?;
    let programme = published(before).ok_or_else(|| {
        Failure::message(
            "the last gym mesocycle names no published programme to carry on with",
            exit::STORE,
        )
    })?;
    let parameters = wizard::ready(&SqliteGenerationParameterStore::new(pool.clone())).await?;

    let provided = |microcycles: Vec<u32>| {
        ProvidedFrom::new(programme.clone(), microcycles).map_err(|error| Failure::usage(&error))
    };
    let shapes: Vec<(Date, Shape)> = match gym {
        Gym::Sbs => vec![(
            start,
            Shape::Provided {
                from: provided((1..=plan::SBS_MICROCYCLES).collect())?,
            },
        )],
        Gym::Hold { holding } => {
            let test = Shape::Test {
                reps: domain::measure::RepCount::new(1).map_err(|error| Failure::usage(&error))?,
                provided: Some(provided(vec![plan::SBS_MICROCYCLES])?),
                asserted: None,
            };
            match holding {
                0 => vec![(start, test)],
                weeks => vec![
                    (
                        start,
                        Shape::Linear {
                            gating: HARDER,
                            weeks,
                        },
                    ),
                    (weeks_after(start, weeks)?, test),
                ],
            }
        }
    };

    let mut built = Vec::with_capacity(shapes.len());
    for (at, shape) in shapes {
        let Some(week) = diary.training_week(at, Discipline::Gym) else {
            return Err(Failure::message(
                format!("the schedule gives the gym no day of the week as of {at}"),
                exit::USAGE,
            ));
        };
        let answers = Authored {
            start: at,
            pattern: before.primary(),
            primary_exercise: before.primary_exercise(),
            week,
            shape,
        };
        let skips: Vec<Skip> = answers.window().map_or_else(Vec::new, |(from, until)| {
            diary
                .unavailable(from, until, Discipline::Gym)
                .into_iter()
                .map(Skip::day)
                .collect()
        });
        built.push(
            domain::prescription::authored::programme(
                answers,
                before.fills().clone(),
                &skips,
                zone.as_time_zone(),
                &parameters,
            )
            .map_err(|error| Failure::usage(&error))?,
        );
    }
    Ok(built)
}

/// The cycling half: a published mesocycle, or a hold's holding weeks and its
/// test week.
async fn cycling_side(
    pool: &SqlitePool,
    classes: &PelotonClasses,
    cycling: Cycling,
    start: Date,
) -> Result<Vec<CyclingMesocycle>, Failure> {
    let diary = SqliteDiaryStore::new(pool.clone());
    let riding_days = plan::cycling_weekdays(&diary, start).await?;
    let source = |error: &dyn std::fmt::Display| Failure::message(error.to_string(), exit::SOURCE);

    let (name, half) = match cycling {
        Cycling::Hold { holding } => {
            let mut built = Vec::new();
            if let Some(weeks) = NonZeroU32::new(holding) {
                built.push(
                    application::holding::weeks(
                        &PelotonHoldingRides::new(classes),
                        &SqliteRiddenVenues::new(pool.clone()),
                        start,
                        weeks,
                        HARDER,
                        EASIER,
                    )
                    .await
                    .map_err(|error| source(&error))?,
                );
            }
            let testing = plan::fetch(classes, skeleton::POWER_ZONE_TEST).await?;
            let sessions = plan::test_microcycle(&testing, SESSIONS)
                .ok_or_else(|| source(&"the Power Zone test holds no microcycle"))?;
            built.push(plan::build(
                weeks_after(start, holding)?,
                &testing,
                &[1],
                &sessions,
                &riding_days,
            )?);
            return Ok(built);
        }
        Cycling::Base1 => (BASE, 0),
        Cycling::Base2 => (BASE, 1),
        Cycling::Build => (BUILD, 0),
        Cycling::Peak1 => (PEAK, 0),
        Cycling::Peak2 => (PEAK, 1),
    };

    let read = plan::fetch(classes, name).await?;
    let offered = plan::mesocycles_of(&read, MICROCYCLES, SESSIONS);
    let answer = offered
        .get(half)
        .and_then(|one| one.answer.as_ref())
        .ok_or_else(|| {
            source(&format!(
                "{name} offers no {cycling} of {MICROCYCLES} weeks"
            ))
        })?;
    let built = plan::build(
        start,
        &read,
        &answer.microcycles,
        &answer.sessions,
        &riding_days,
    )?;

    if cycling == Cycling::Base2 {
        let testing = plan::fetch(classes, skeleton::POWER_ZONE_TEST).await?;
        return Ok(vec![ending_in_a_test(&built, &testing)?]);
    }
    Ok(vec![built])
}

/// Our Base 2: the last microcycle's shorter, more intense ride replaced by
/// the FTP test (the operator, 2026-09-25). Published Base has no test of its
/// own, and Build opens from one.
///
/// **The replaced ride keeps its place in Base**: its published coordinate is
/// the session it stands in for, which is the way back to what was not ridden.
fn ending_in_a_test(
    mesocycle: &CyclingMesocycle,
    testing: &plan::Read,
) -> Result<CyclingMesocycle, Failure> {
    let source = |detail: String| Failure::message(detail, exit::SOURCE);
    let test_session = testing
        .programme
        .sessions()
        .into_iter()
        .find(|session| plan::measures(testing, 1, &[*session]))
        .ok_or_else(|| source("the Power Zone test holds no FTP test".to_owned()))?;
    let classes = testing
        .fetched
        .get(&(1, test_session))
        .ok_or_else(|| source("the FTP test was not read".to_owned()))?;
    let (ride, at) = infrastructure::peloton::provider::session((1, test_session), classes)
        .map_err(|error| source(error.to_string()))?;

    let count = mesocycle.duration_weeks();
    let mut weeks = Vec::with_capacity(count);
    for (index, week) in mesocycle.microcycles().iter().enumerate() {
        if index.saturating_add(1) < count {
            weeks.push(week.clone());
            continue;
        }
        let mut rides = week.rides().clone();
        let (position, replaced) = week
            .for_role(HARDER)
            .map(|(position, ride)| (position, ride.clone()))
            .ok_or_else(|| source("Base's last week has no harder ride to replace".to_owned()))?;
        let published = replaced
            .published()
            .ok_or_else(|| source("Base's last week names no published session".to_owned()))?;
        rides.insert(
            position,
            PlannedRide::provided(ride.clone(), at.clone(), published, HARDER),
        );
        weeks.push(CyclingMicrocycle::new(rides).map_err(|error| Failure::usage(&error))?);
    }

    CyclingMesocycle::new(
        mesocycle.provenance().clone(),
        mesocycle.start(),
        NonEmpty::new(weeks).map_err(|error| Failure::usage(&error))?,
    )
    .map_err(|error| Failure::usage(&error))
}

/// The Monday `weeks` after this one.
fn weeks_after(date: Date, weeks: u32) -> Result<Date, Failure> {
    date.checked_add(jiff::Span::new().weeks(i64::from(weeks)))
        .map_err(|error| Failure::usage(&error))
}
