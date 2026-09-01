//! *Peak Your Power Zones*, transcribed.
//!
//! **A seed, never a fallback** (decision 0015), and the same shape as
//! [`prescription::seed`](crate::prescription::seed): shipped, written into the
//! store, and read back from there rather than reached for on the generation
//! path.
//!
//! **It lives in `domain` because it is training, not a vendor's data.** What
//! is here is twenty-five sessions of duration × power zone — the thing the
//! operator will ride, in the vocabulary this crate owns. Peloton's `classId`s
//! are what *realise* each session and live with the Peloton adapter, exactly as
//! Hevy's `exercise_template_id`s do (§ II.3, and `CLAUDE.md`).
//!
//! **Every number here was read off the operator's own screenshots** and
//! reconciled twice: against the app's stated `Cycling` total, and against its
//! stated movement count. The full transcription, its corroborations and the
//! three claims it falsified along the way are in
//! `docs/cycling-peak-your-power-zones.md`. Durations are not round because
//! these are recorded classes and the intervals fall where the instructor put
//! them.

use std::collections::BTreeMap;

use crate::gym::{PositiveDuration, sequence::NonEmpty};

use super::{
    programme::{CycleDay, CyclingProgramme, CyclingProgrammeName, ProgrammeWeek},
    session::{CyclingSession, Interval, Ride},
    zone::PowerZone,
};

/// A value in this file that will not build.
///
/// Every one is written a few lines above, so this is a defect in the build
/// rather than anything an operator did — but panicking is forbidden and so is
/// pretending it cannot happen. [`pinned`](self) is the test that keeps it from
/// happening.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the shipped {what} will not build ({detail}) — this is a defect in this build")]
pub struct InvalidCyclingSeed {
    what: &'static str,
    detail: String,
}

impl InvalidCyclingSeed {
    fn at(what: &'static str, detail: impl std::fmt::Display) -> Self {
        Self {
            what,
            detail: detail.to_string(),
        }
    }
}

fn seconds(what: &'static str, value: u64) -> Result<PositiveDuration, InvalidCyclingSeed> {
    PositiveDuration::from_seconds(value).map_err(|error| InvalidCyclingSeed::at(what, error))
}

fn minutes(what: &'static str, value: u64) -> Result<PositiveDuration, InvalidCyclingSeed> {
    seconds(what, value * 60)
}

/// One class, as the app states it: warm-up minutes, then the ride.
fn class(
    what: &'static str,
    warm_up_minutes: u64,
    intervals: &[(u8, u64)],
    cool_down_seconds: u64,
) -> Result<CyclingSession, InvalidCyclingSeed> {
    let built = intervals
        .iter()
        .map(|(zone, secs)| {
            let zone = PowerZone::try_from(*zone)
                .map_err(|error| InvalidCyclingSeed::at(what, error))?;
            Ok(Interval::new(zone, seconds(what, *secs)?))
        })
        .collect::<Result<Vec<_>, InvalidCyclingSeed>>()?;
    let ride = Ride::Intervals(
        NonEmpty::new(built).map_err(|error| InvalidCyclingSeed::at(what, error))?,
    );
    let cool_down = match cool_down_seconds {
        0 => None,
        value => Some(seconds(what, value)?),
    };
    Ok(CyclingSession::new(
        minutes(what, warm_up_minutes)?,
        ride,
        cool_down,
    ))
}

/// The FTP test: a warm-up class, then twenty minutes with no zone at all.
///
/// **Two Peloton classes, one session.** The app ships the warm-up separately
/// because the test class has none of its own; what the operator rides is one
/// thing. See decision 0025.
fn test(what: &'static str) -> Result<CyclingSession, InvalidCyclingSeed> {
    Ok(CyclingSession::new(
        minutes(what, 10)?,
        Ride::Effort(minutes(what, 20)?),
        None,
    ))
}

fn week(
    days: Vec<(u8, CyclingSession)>,
) -> Result<ProgrammeWeek, InvalidCyclingSeed> {
    let mut sessions = BTreeMap::new();
    for (day, session) in days {
        let day = CycleDay::new(day).map_err(|error| InvalidCyclingSeed::at("a cycle day", error))?;
        sessions.insert(day, session);
    }
    Ok(ProgrammeWeek::new(sessions))
}

/// A minute of Peloton cool-down, which every class but the test carries.
const COOL: u64 = 60;

/// *Peak Your Power Zones* — eight weeks, three sessions a week, twenty-five
/// classes.
///
/// # Errors
///
/// [`InvalidCyclingSeed`] if anything written here is not a legal value. Pinned
/// by a test, so it is a mistake in this file rather than in an invocation.
pub fn peak_your_power_zones() -> Result<CyclingProgramme, InvalidCyclingSeed> {
    let name = CyclingProgrammeName::try_from("Peak Your Power Zones".to_owned())
        .map_err(|error| InvalidCyclingSeed::at("the programme name", error))?;

    let weeks = vec![
        // Week 1 — base. Every session endurance.
        week(vec![
            (1, class("w1d1", 11, &[(3,300),(2,180),(3,420),(2,180),(3,420),(2,180),(3,300)], COOL)?),
            (3, class("w1d3", 10, &[(3,359),(2,120),(3,479),(2,120),(3,481),(2,121),(3,360)], COOL)?),
            // Descending step, 7/7/5/5/3/3 — the class says so itself.
            (6, class("w1d6", 13, &[(3,420),(2,240),(3,420),(2,240),(3,300),(2,180),(3,300),(2,180),(3,180),(2,120),(3,180)], COOL)?),
        ])?,
        // Week 2 — threshold arrives on day 1.
        week(vec![
            (1, class("w2d1", 11, &[(4,240),(3,180),(4,180),(3,180),(4,120),(1,180),(4,240),(3,180),(4,180),(3,180),(4,120)], COOL)?),
            (3, class("w2d3", 12, &[(3,480),(2,180),(3,600),(2,180),(3,480)], COOL)?),
            // Descending step, 8/8/6/6/4/4.
            (6, class("w2d6", 12, &[(3,480),(2,180),(3,480),(2,181),(3,359),(2,120),(3,360),(2,119),(3,240),(2,60),(3,240)], COOL)?),
        ])?,
        // Week 3 — Z5 arrives, as thirty-second bursts.
        week(vec![
            (1, class("w3d1", 12, &[(4,300),(1,120),(5,30),(2,30),(5,30),(2,30),(5,30),(2,30),(5,30),(2,30),(5,30),(1,120),(4,300),(1,120),(5,30),(2,30),(5,30),(2,30),(5,30),(2,30),(5,30),(2,30),(5,30),(1,120),(4,300)], COOL)?),
            (3, class("w3d3", 12, &[(4,120),(3,239),(4,120),(1,180),(4,240),(3,120),(4,240),(1,180),(4,119),(3,240),(4,120)], COOL)?),
            // Descending, 9/9/7/7/5.
            (6, class("w3d6", 12, &[(3,540),(2,180),(3,540),(2,180),(3,420),(2,122),(3,418),(2,119),(3,301)], COOL)?),
        ])?,
        // Week 4 — deload. Nothing above Z3 all week; both mapped classes pyramid.
        week(vec![
            (1, class("w4d1", 12, &[(3,180),(2,120),(3,301),(2,120),(3,420),(2,180),(3,300),(2,120),(3,179)], COOL)?),
            (3, class("w4d3", 10, &[(3,480),(2,180),(3,300),(2,120),(3,480),(2,180),(3,300)], COOL)?),
            // Pyramid, 5/7/9/7/5. Shown `Unavailable` to the operator — see 0025.
            (6, class("w4d6", 13, &[(3,296),(2,180),(3,422),(2,178),(3,540),(2,240),(3,420),(2,180),(3,300)], COOL)?),
        ])?,
        // Week 5 — week 3 amplified; Z5 becomes sustained blocks.
        week(vec![
            (1, class("w5d1", 12, &[(4,300),(1,120),(5,120),(1,120),(4,300),(1,120),(5,180),(1,120),(4,300),(1,120),(5,120)], COOL)?),
            (3, class("w5d3", 11, &[(4,180),(3,180),(4,120),(1,180),(4,240),(3,240),(4,180),(1,180),(4,180),(3,180),(4,120)], COOL)?),
            // Inverted pyramid, 9/7/5/7/9.
            (6, class("w5d6", 11, &[(3,540),(2,180),(3,420),(2,180),(3,300),(2,120),(3,420),(2,180),(3,540)], COOL)?),
        ])?,
        // Week 6 — the threshold and VO2 concentration. Day 1 reaches Z6 and
        // carries no Z4 at all, the only Power Zone ride that does not.
        week(vec![
            (1, class("w6d1", 13, &[(5,180),(1,180),(5,180),(1,180),(5,180),(1,180),(6,60),(1,120),(6,60),(1,120),(6,60),(1,120),(6,60),(1,120),(6,60)], COOL)?),
            (3, class("w6d3", 13, &[(4,300),(5,60),(1,120),(4,240),(5,120),(1,181),(4,299),(5,60),(1,119),(4,240),(5,121)], COOL)?),
            // The only sixty-minute ride that is not an endurance ride.
            (6, class("w6d6", 13, &[(3,240),(4,180),(3,240),(4,180),(1,240),(3,180),(4,120),(3,180),(4,120),(1,240),(3,240),(4,180),(3,240),(4,180)], COOL)?),
        ])?,
        // Week 7 — the peak. The only Z7, and the volume high.
        week(vec![
            (1, class("w7d1", 12, &[
                (6,30),(4,180),(7,15),(1,195),
                (6,30),(4,180),(7,15),(1,195),
                (6,30),(4,180),(7,15),(1,195),
                (5,30),(2,30),(5,30),(2,30),(5,30),(2,30),
                (6,30),(2,30),(6,30),(2,30),(6,30),
                (1,195),
                (7,15),(1,15),(7,15),(1,15),(7,15),(1,15),(7,15),(1,15),(7,15),
            ], COOL)?),
            (3, class("w7d3", 13, &[
                (4,240),(1,60),(5,60),(2,60),(5,60),(2,60),(5,60),(2,60),(5,61),(1,60),
                (6,30),(1,31),(6,30),(1,30),(6,30),(1,30),(6,30),(1,120),
                (4,180),(1,60),(5,60),(2,59),(5,62),(2,60),(5,59),(1,60),
                (6,29),(1,30),(6,29),(1,30),(6,30),
            ], COOL)?),
            // Pyramid, 2x4 / 2x6 / 2x8 / 2x6 / 2x4 — the volume peak.
            (6, class("w7d6", 13, &[(3,240),(2,120),(3,240),(2,120),(3,360),(2,120),(3,360),(2,120),(3,480),(2,180),(3,480),(2,180),(3,360),(2,120),(3,360),(2,120),(3,240),(2,120),(3,240)], COOL)?),
        ])?,
        // Week 8 — taper, then the retest.
        week(vec![
            (1, class("w8d1", 13, &[(3,420),(2,240),(3,540),(2,240),(3,421)], COOL)?),
            (3, class("w8d3", 13, &[(3,180),(2,120),(3,240),(2,180),(3,299),(2,240),(3,240),(2,180),(3,181)], COOL)?),
            (6, test("w8d6")?),
        ])?,
    ];

    Ok(CyclingProgramme::new(
        name,
        NonEmpty::new(weeks)
            .map_err(|error| InvalidCyclingSeed::at("the programme's weeks", error))?,
    ))
}
