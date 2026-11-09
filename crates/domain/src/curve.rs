//! Where each week of a macrocycle falls on the volume and intensity curves
//! (#742).
//!
//! **The two axes a programme moves**, read off what the plan prescribes
//! rather than what was performed. The operator, 2026-11-09, on the home page:
//! *"where we stand within the volume and intensity curves"*, in the
//! mesocycle and in the macrocycle.
//!
//! - **The gym is its primary lift only**: *"accessories don't follow a
//!   curve"*. Volume is the primary's working repetitions in the week, and
//!   intensity the percentage day's share of the maximum current that week,
//!   both read off the SBS chart. A programme that is not a published chart
//!   states no share of a maximum, so it has no point on the curves.
//! - **Cycling is minutes ridden and [`ZoneProfile::intensity`]**, from the
//!   zones each microcycle's rides were authored with.

use jiff::civil::Date;

use crate::{
    cycling::{CyclingMesocycle, CyclingMicrocycle, ZoneProfile},
    macrocycle::Macrocycle,
    plan::Plan,
    planner::Discipline,
    prescription::{
        GymMesocycle, Progression, SbsChart,
        sbs::chart::{SbsDay, SbsSession},
        schedule::WeekKind,
    },
};

/// One microcycle's place on the two axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// Working repetitions of the primary lift, or minutes ridden.
    pub volume: f64,
    /// Percent of the current maximum, or of FTP.
    pub intensity: f64,
}

/// What a week of the macrocycle is, for one discipline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OnCurve {
    /// A microcycle of a mesocycle. `point` is `None` where the programme
    /// states no curve; `tests` where its essential session is a test.
    Microcycle { point: Option<Point>, tests: bool },
    /// A week whose microcycle was not trained.
    Lost,
    /// A week the discipline holds while the other reruns its test.
    Holding,
    /// No mesocycle covers it.
    Unplanned,
}

/// One week of the macrocycle, for one discipline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Week {
    /// Its Monday.
    pub monday: Date,
    pub on: OnCurve,
    /// Whether it is in a mesocycle not yet committed, which is what the
    /// plan is expected to be rather than what it is.
    pub expected: bool,
}

/// Both disciplines' weeks across a macrocycle, Monday to Monday.
#[derive(Debug, Clone, PartialEq)]
pub struct Curves {
    pub gym: Vec<Week>,
    pub cycling: Vec<Week>,
}

/// Every week of `macrocycle` on the curves, read from `plan`.
///
/// `committed_until` is the first day no committed mesocycle covers: what
/// `plan` holds from then on is expected. `lost` says whether a discipline's
/// week was lost, which the plan no longer shows once a lost week carries on
/// (#735).
#[must_use]
pub fn curves(
    macrocycle: &Macrocycle,
    plan: &Plan,
    committed_until: Date,
    lost: impl Fn(Discipline, Date) -> bool,
) -> Curves {
    let mondays = mondays(macrocycle);
    let week = |discipline: Discipline, monday: Date| {
        let on = if lost(discipline, monday) {
            OnCurve::Lost
        } else {
            match discipline {
                Discipline::Gym => gym_week(plan, monday),
                Discipline::Cycling => cycling_week(plan, monday),
            }
        };
        Week {
            monday,
            on,
            expected: monday >= committed_until,
        }
    };
    Curves {
        gym: mondays
            .iter()
            .map(|monday| week(Discipline::Gym, *monday))
            .collect(),
        cycling: mondays
            .iter()
            .map(|monday| week(Discipline::Cycling, *monday))
            .collect(),
    }
}

/// The Monday of every week from the first week of term to the last week of
/// the holiday.
fn mondays(macrocycle: &Macrocycle) -> Vec<Date> {
    std::iter::successors(Some(macrocycle.start()), |monday| {
        monday.checked_add(jiff::Span::new().weeks(1)).ok()
    })
    .take_while(|monday| *monday <= macrocycle.last())
    .collect()
}

fn gym_week(plan: &Plan, monday: Date) -> OnCurve {
    let Some((_, mesocycle)) = plan.gym().and_then(|gym| gym.on(monday)) else {
        return OnCurve::Unplanned;
    };
    match mesocycle.calendar().place_week(monday) {
        Ok(WeekKind::Holding) => OnCurve::Holding,
        Ok(kind) => OnCurve::Microcycle {
            point: primary_point(mesocycle, kind),
            tests: mesocycle.tests_in(kind),
        },
        Err(_) => OnCurve::Lost,
    }
}

/// The primary lift's week, where the programme is a published chart.
///
/// **Volume counts both days**, and a back-off taken for a range counts its
/// middle: 3 × 5–6 is 16.5. **Intensity is the percentage day's share**, the
/// one day the chart states a share for; the repetition-maximum day states a
/// count and no load.
fn primary_point(mesocycle: &GymMesocycle, kind: WeekKind) -> Option<Point> {
    let GymMesocycle::Progression(Progression::Provided { .. }) = mesocycle else {
        return None;
    };
    let week = mesocycle.published_week(kind)?;
    let chart = SbsChart::published().ok()?;
    let (first, second) = (
        chart.day(week, SbsSession::First).ok()?,
        chart.day(week, SbsSession::Second).ok()?,
    );
    let SbsDay::Percentage { share, .. } = first else {
        return None;
    };
    Some(Point {
        volume: working_reps(first) + working_reps(second),
        intensity: f64::from(share.as_basis_points()) / 100.0,
    })
}

fn working_reps(day: SbsDay) -> f64 {
    match day {
        SbsDay::Percentage { sets, reps, .. } => f64::from(sets.as_u32() * reps.as_u32()),
        SbsDay::RepMax {
            reps,
            back_off_sets,
            back_off_reps,
        } => {
            let middle =
                f64::from(back_off_reps.minimum().as_u32() + back_off_reps.maximum().as_u32())
                    / 2.0;
            f64::from(back_off_sets.as_u32()).mul_add(middle, f64::from(reps.as_u32()))
        }
        SbsDay::Test { reps } => f64::from(reps.as_u32()),
    }
}

fn cycling_week(plan: &Plan, monday: Date) -> OnCurve {
    let Some((_, mesocycle)) = plan.cycling().and_then(|cycling| cycling.on(monday)) else {
        return OnCurve::Unplanned;
    };
    if mesocycle.holds_on(monday) {
        return OnCurve::Holding;
    }
    microcycle_on(mesocycle, monday).map_or(OnCurve::Lost, |microcycle| OnCurve::Microcycle {
        point: Some(ridden(microcycle)),
        tests: microcycle
            .rides()
            .values()
            .any(|ride| ride.session().is_test()),
    })
}

fn microcycle_on(mesocycle: &CyclingMesocycle, monday: Date) -> Option<&CyclingMicrocycle> {
    mesocycle.microcycle(mesocycle.microcycle_of(monday)?)
}

fn ridden(microcycle: &CyclingMicrocycle) -> Point {
    let zones = ZoneProfile::of(
        microcycle
            .rides()
            .values()
            .map(|ride| ride.session().ride()),
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "seconds of riding; f64 is exact far past any plausible total"
    )]
    let minutes = zones.total() as f64 / 60.0;
    Point {
        volume: minutes,
        intensity: zones.intensity(),
    }
}

#[cfg(test)]
mod tests {
    use super::working_reps;
    use crate::prescription::{SbsChart, SbsSession};

    /// **Both days count, and a back-off range counts its middle**: week 1 is
    /// 5 × 5, then an 8RM and 3 × 5–6, which is 25 + 8 + 16.5.
    #[test]
    fn a_chart_week_s_volume_is_both_days_working_reps() {
        let chart = SbsChart::published().expect("the published chart builds");
        let volumes: Vec<f64> = (1..=4)
            .map(|week| {
                working_reps(
                    chart
                        .day(week, SbsSession::First)
                        .expect("a week of the chart"),
                ) + working_reps(
                    chart
                        .day(week, SbsSession::Second)
                        .expect("a week of the chart"),
                )
            })
            .collect();
        assert_eq!(volumes, vec![49.5, 27.5, 10.5, 10.0]);
    }
}
