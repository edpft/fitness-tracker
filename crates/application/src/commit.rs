//! Committing the next concurrent mesocycle when the one before it ends (#222).
//!
//! **One at a time.** The plan holds what has been committed and nothing past
//! it: each mesocycle is chosen when the one before it ends, because only then
//! is its entry test known. The macrocycle says which
//! ([`Macrocycle::next`]); this reads what it needs to ask, builds both halves
//! and appends them to the plan in force.
//!
//! **Here since #397**, so `fitness next` can be offered by any frontend. It was
//! in `cli` because building a cycling mesocycle read Peloton through the
//! `plan` wizard's code; [`PublishedProgrammes`] is that code's port.

use domain::planner::Arrangement as _;
use std::num::NonZeroU32;

use domain::{
    cycling::CyclingMesocycle,
    macrocycle::{Concurrent, Cycling, Gym, Macrocycle, Whole},
    normalised::OperatorZone,
    plan::{Plan, Span},
    planner,
    planner::Discipline,
    prescription::{
        GymMesocycle, Progression,
        authored::{Authored, Shape},
        schedule::Skip,
    },
    provider::{ExternalProgramme, ProvidedFrom},
    schedule::{DayPart, Diary, Holidays},
};
use jiff::civil::{Date, Weekday};

use crate::{
    Ending, GenerationParameterStore, HoldingRides, PlanStore, ProgressionStore, PublishedCycling,
    PublishedProgrammes, RiddenVenues, SourceError, StoreError,
    holding::NoHoldingRide,
    microcycle::Standing,
    published::{self, BuildError, EASIER, HARDER},
    reschedule::ReschedulingPlans,
};

/// Microcycles and cycling sessions in a committed mesocycle: the shape the
/// `plan` wizard asks for by default.
const MICROCYCLES: usize = 4;
const SESSIONS: usize = 2;

/// Why the mesocycle due could not be committed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommitError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Holding(#[from] NoHoldingRide),
    /// No version of linear progression had been stored when the plan was
    /// made, and a hold's holding weeks climb by it (#192).
    #[error(
        "no version of linear progression had been stored when this plan was made, at \
         {planned_at}. `fitness init` stores the first"
    )]
    NotDesigned { planned_at: jiff::Timestamp },
    /// The plan does not say what to carry on with.
    #[error("{detail}")]
    NothingToCarryOn { detail: String },
    /// The schedule gives a discipline no day to train on.
    #[error("the schedule gives {discipline} no day of the week as of {at}")]
    NoDay { discipline: Discipline, at: Date },
    /// What the macrocycle chose is not a mesocycle the domain accepts.
    #[error("{detail}")]
    Unbuildable { detail: String },
}

fn unbuildable(error: &impl std::fmt::Display) -> CommitError {
    CommitError::Unbuildable {
        detail: error.to_string(),
    }
}

/// A concurrent mesocycle the macrocycle has chosen, before it is built.
///
/// **Deciding reads only the store and the calendar**, so what is due can
/// always be said. Building it reads the published programmes, which may not
/// answer (§ 36).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    plan: Plan,
    diary: Diary,
    /// Whether a lost week ended the plan's last mesocycles early (#732), so
    /// that the store ends them too before anything is added after them.
    ended_early: bool,
    pub concurrent: Concurrent,
    pub span: Span,
}

/// Everything committing reads and writes.
pub struct CommitPorts<P, G, H, R, B> {
    pub plans: P,
    pub parameters: G,
    /// The catalogue a hold's holding weeks are chosen from.
    pub holding: H,
    /// What has been ridden, which a holding week does not offer again.
    pub ridden: R,
    pub published: B,
}

/// The use case.
pub struct Committing<P, G, H, R, B> {
    ports: CommitPorts<P, G, H, R, B>,
    zone: OperatorZone,
}

impl<P, G, H, R, B> Committing<P, G, H, R, B>
where
    P: PlanStore + Clone + Sync,
    G: GenerationParameterStore + ProgressionStore + Sync,
    H: HoldingRides + Sync,
    R: RiddenVenues + Sync,
    B: PublishedProgrammes + Sync,
{
    pub const fn new(ports: CommitPorts<P, G, H, R, B>, zone: OperatorZone) -> Self {
        Self { ports, zone }
    }

    /// The next concurrent mesocycle, if the one before it has ended.
    ///
    /// `None` while the current one is still running, where no plan is in force
    /// this macrocycle, where the plan states no chain, or where the term has
    /// nothing left.
    ///
    /// # Errors
    ///
    /// [`CommitError::Store`] if the plan cannot be read.
    pub async fn due(
        &self,
        now: DayPart,
        holidays: &Holidays,
        standing: &Standing,
        diary: &Diary,
    ) -> Result<Option<Due>, CommitError> {
        let Some(macrocycle) = Macrocycle::on(now.date, holidays) else {
            return Ok(None);
        };
        let plans =
            ReschedulingPlans::new(self.ports.plans.clone(), standing.reschedule(diary.clone()));
        let Some(plan) = in_force(&plans, &macrocycle).await? else {
            return Ok(None);
        };
        let Some(chain) = plan.chain() else {
            return Ok(None);
        };

        // **Due once the plan has run out**, and from the current microcycle if it
        // ran out earlier: a week nothing was committed for is not given back.
        // The current microcycle, not the calendar week, because riding the
        // Sunday session starts the next one that evening (#281).
        let monday = standing
            .commencing
            .unwrap_or_else(|| planner::commencing(now.date));
        let end = plan.span().end();
        if end > monday {
            return Ok(None);
        }
        let start = end.max(monday);

        let previous = plan
            .cycling()
            .and_then(|side| side.mesocycles().last())
            .filter(|last| last.start() >= macrocycle.start())
            .and_then(|last| self.cycling_of(last));
        // **A lost week that started the programme again commits what is left
        // of the term** (#732), chosen from the whole of it, where the boundary
        // would otherwise choose what comes next.
        let restarted_on = |monday: Date| {
            standing
                .restarts
                .iter()
                .any(|restart| restart.monday == monday)
        };
        let ended_early = restarted_on(end);
        let chosen = previous
            .filter(|_| restarted_on(start))
            .and_then(|current| macrocycle.restart(current, start))
            .or_else(|| macrocycle.next(chain, previous, start));
        Ok(chosen.map(|concurrent| Due {
            ended_early,
            plan,
            diary: diary.clone(),
            concurrent,
            span: Span::new(start, concurrent.weeks()),
        }))
    }

    /// Build what is due and add it to the plan.
    ///
    /// # Errors
    ///
    /// [`CommitError`] if a published programme or the store cannot be read, or
    /// if what the macrocycle chose will not build. Nothing is written unless
    /// both halves built.
    pub async fn commit(&self, due: &Due) -> Result<(), CommitError> {
        let start = due.span.start();
        let cycling = self
            .cycling_side(&due.diary, due.concurrent.cycling(), start)
            .await?;
        // **The gym runs the same weeks as the bike** (#732): a selection's
        // positions in the whole programme, each the week of the chart it falls
        // on, so the two reach their troughs together.
        let positions = match due.concurrent.cycling() {
            Cycling::Selection { .. } => cycling
                .first()
                .and_then(CyclingMesocycle::provided_from)
                .map(|provided| provided.microcycles().collect::<Vec<u32>>()),
            _ => None,
        };
        let gym = self
            .gym_side(
                &due.plan,
                &due.diary,
                due.concurrent.gym(),
                start,
                positions.as_deref(),
            )
            .await?;
        let ending = due
            .ended_early
            .then(|| {
                Some(Ending {
                    gym: due.plan.gym()?.mesocycles().last()?,
                    cycling: due.plan.cycling()?.mesocycles().last()?,
                })
            })
            .flatten();
        Ok(self
            .ports
            .plans
            .commit(due.plan.name(), ending, &gym, &cycling)
            .await?)
    }

    /// Which cycling mesocycle a committed one is ([`cycling_kind`]).
    fn cycling_of(&self, mesocycle: &CyclingMesocycle) -> Option<Cycling> {
        cycling_kind(mesocycle, |programme| self.ports.published.which(programme))
    }

    /// The gym's half: SBS, or a hold's linear holding weeks and its test week.
    ///
    /// **What does not change is carried from the mesocycle before**: the lift,
    /// its slot, the accessories filling the rest, and the published programme.
    /// The plan was stated once; nothing here asks again.
    ///
    /// `positions` are the weeks a selection runs, counted through cycles of
    /// the chart taken end to end: 1-2-3-6-7-8 runs chart weeks 1-2-3-2-3-4.
    async fn gym_side(
        &self,
        plan: &Plan,
        diary: &Diary,
        gym: Gym,
        start: Date,
        positions: Option<&[u32]>,
    ) -> Result<Vec<GymMesocycle>, CommitError> {
        let before = plan
            .gym()
            .and_then(|side| side.mesocycles().last())
            .ok_or_else(|| CommitError::NothingToCarryOn {
                detail: "the plan holds no gym mesocycle to carry on from".to_owned(),
            })?;
        let programme = published_from(before).ok_or_else(|| CommitError::NothingToCarryOn {
            detail: "the last gym mesocycle names no published programme to carry on with"
                .to_owned(),
        })?;
        // The plan's own version, because what is committed is part of the plan
        // and was planned with it (#192); the scales are the start's (#450).
        let planned_at = plan.authored_at();
        let (_, linear) = self
            .ports
            .parameters
            .linear_at(planned_at)
            .await?
            .ok_or(CommitError::NotDesigned { planned_at })?;
        let scales = self.ports.parameters.scales_on(start).await?;

        let sbs = Gym::Sbs.weeks();
        let provided = |microcycles: Vec<u32>| {
            ProvidedFrom::new(programme.clone(), microcycles).map_err(|error| unbuildable(&error))
        };
        let shapes: Vec<(Date, Shape)> = match gym {
            Gym::Sbs => vec![(
                start,
                Shape::Provided {
                    from: provided(positions.map_or_else(
                        || (1..=sbs).collect(),
                        |positions| {
                            positions
                                .iter()
                                .map(|position| {
                                    position.saturating_sub(1).checked_rem(sbs).unwrap_or(0) + 1
                                })
                                .collect()
                        },
                    ))?,
                },
            )],
            Gym::Hold { holding } => {
                let test = Shape::Test {
                    reps: domain::measure::RepCount::new(1).map_err(|error| unbuildable(&error))?,
                    provided: Some(provided(vec![sbs])?),
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
            let week = diary
                .training_week(at, Discipline::Gym)
                .ok_or(CommitError::NoDay {
                    discipline: Discipline::Gym,
                    at,
                })?;
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
                domain::prescription::authored::mesocycle(
                    answers,
                    before.fills().clone(),
                    &skips,
                    self.zone.as_time_zone(),
                    &linear,
                    &scales,
                )
                .map_err(|error| unbuildable(&error))?,
            );
        }
        Ok(built)
    }

    /// The cycling half: a published mesocycle, or a hold's holding weeks and its
    /// test week.
    async fn cycling_side(
        &self,
        diary: &Diary,
        cycling: Cycling,
        start: Date,
    ) -> Result<Vec<CyclingMesocycle>, CommitError> {
        let riding_days = cycling_weekdays(diary, start)?;
        let published = &self.ports.published;

        let (which, half) = match cycling {
            Cycling::Hold { holding } => {
                let mut built = Vec::new();
                if let Some(weeks) = NonZeroU32::new(holding) {
                    built.push(
                        crate::holding::weeks(
                            &self.ports.holding,
                            &self.ports.ridden,
                            start,
                            weeks,
                            HARDER,
                            EASIER,
                        )
                        .await?,
                    );
                }
                let testing = published.read(PublishedCycling::Test).await?;
                let sessions = published::test_microcycle(&testing, SESSIONS).ok_or_else(|| {
                    CommitError::Source(SourceError::Malformed {
                        detail: "the Power Zone test holds no microcycle".to_owned(),
                    })
                })?;
                built.push(published::build(
                    weeks_after(start, holding)?,
                    &testing,
                    &[1],
                    &sessions,
                    &riding_days,
                )?);
                return Ok(built);
            }
            Cycling::Selection { of, weeks } => {
                let which = match of {
                    Whole::Base => PublishedCycling::Base,
                    Whole::Peak => PublishedCycling::Peak,
                };
                let read = published.read(which).await?;
                let wanted = usize::try_from(weeks).map_err(|error| unbuildable(&error))?;
                let answer = published::selection_of(&read, wanted, SESSIONS).ok_or_else(|| {
                    CommitError::Source(SourceError::Malformed {
                        detail: format!(
                            "{} offers no {weeks} weeks with one trough",
                            read.from.name()
                        ),
                    })
                })?;
                let built = published::build(
                    start,
                    &read,
                    &answer.microcycles,
                    &answer.sessions,
                    &riding_days,
                )?;
                if of == Whole::Base {
                    let testing = published.read(PublishedCycling::Test).await?;
                    return Ok(vec![published::ending_in_a_test(&built, &testing)?]);
                }
                return Ok(vec![built]);
            }
            Cycling::Base1 => (PublishedCycling::Base, 0),
            Cycling::Base2 => (PublishedCycling::Base, 1),
            Cycling::Build => (PublishedCycling::Build, 0),
            Cycling::Peak1 => (PublishedCycling::Peak, 0),
            Cycling::Peak2 => (PublishedCycling::Peak, 1),
        };

        let read = published.read(which).await?;
        let offered = published::mesocycles_of(&read, MICROCYCLES, SESSIONS);
        let answer = offered
            .get(half)
            .and_then(|one| one.answer.as_ref())
            .ok_or_else(|| {
                CommitError::Source(SourceError::Malformed {
                    detail: format!(
                        "{} offers no {cycling} of {MICROCYCLES} weeks",
                        read.from.name()
                    ),
                })
            })?;
        let built = published::build(
            start,
            &read,
            &answer.microcycles,
            &answer.sessions,
            &riding_days,
        )?;

        if cycling == Cycling::Base2 {
            let testing = published.read(PublishedCycling::Test).await?;
            return Ok(vec![published::ending_in_a_test(&built, &testing)?]);
        }
        Ok(vec![built])
    }
}

/// The plan this macrocycle is following: the latest to start inside it.
async fn in_force<P: PlanStore + Sync>(
    plans: &ReschedulingPlans<P>,
    macrocycle: &Macrocycle,
) -> Result<Option<Plan>, StoreError> {
    let windows = plans.windows().await?;
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
    plans.named(window.name()).await
}

/// Which cycling mesocycle a committed one is, from where it was taken.
///
/// A mesocycle assembled here is a hold's holding weeks; the published test
/// week is a hold with nothing held. `which` names the published programme a
/// provenance points at.
pub fn cycling_kind(
    mesocycle: &CyclingMesocycle,
    which: impl Fn(&ExternalProgramme) -> Option<PublishedCycling>,
) -> Option<Cycling> {
    let Some(provided) = mesocycle.provided_from() else {
        let holding = u32::try_from(mesocycle.duration_weeks()).ok()?;
        return Some(Cycling::Hold { holding });
    };
    let microcycles: Vec<u32> = provided.microcycles().collect();
    let second_half = microcycles.first().is_some_and(|first| *first > 4);
    // Weeks from both halves are a selection of the whole (#732).
    let whole =
        microcycles.iter().any(|week| *week <= 4) && microcycles.iter().any(|week| *week > 4);
    let weeks = u32::try_from(microcycles.len()).ok()?;
    match which(provided.programme())? {
        PublishedCycling::Base if whole => Some(Cycling::Selection {
            of: Whole::Base,
            weeks,
        }),
        PublishedCycling::Peak if whole => Some(Cycling::Selection {
            of: Whole::Peak,
            weeks,
        }),
        PublishedCycling::Test => Some(Cycling::Hold { holding: 0 }),
        PublishedCycling::Build => Some(Cycling::Build),
        PublishedCycling::Base if second_half => Some(Cycling::Base2),
        PublishedCycling::Base => Some(Cycling::Base1),
        PublishedCycling::Peak if second_half => Some(Cycling::Peak2),
        PublishedCycling::Peak => Some(Cycling::Peak1),
    }
}

/// The published programme a gym mesocycle was taken from.
fn published_from(mesocycle: &GymMesocycle) -> Option<ExternalProgramme> {
    match mesocycle {
        GymMesocycle::Test(test) => test.provided().map(|from| from.programme().clone()),
        GymMesocycle::Progression(Progression::Provided { from, .. }) => {
            Some(from.programme().clone())
        }
        GymMesocycle::Progression(_) => None,
    }
}

/// Which weekdays the schedule gives cycling, as of the start date.
///
/// **The schedule says which days are cycling's, and the programme says which
/// session each rides.**
///
/// # Errors
///
/// [`CommitError::NoDay`] where the schedule says nothing about the date, or
/// gives cycling no day.
pub fn cycling_weekdays(diary: &Diary, start: Date) -> Result<Vec<Weekday>, CommitError> {
    diary
        .ordinarily(start, Discipline::Cycling)
        .filter(|days| !days.is_empty())
        .ok_or(CommitError::NoDay {
            discipline: Discipline::Cycling,
            at: start,
        })
}

/// The Monday `weeks` after this one.
fn weeks_after(date: Date, weeks: u32) -> Result<Date, CommitError> {
    date.checked_add(jiff::Span::new().weeks(i64::from(weeks)))
        .map_err(|error| unbuildable(&error))
}
