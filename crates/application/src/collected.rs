//! How far a run has got with what a page reads, a stage at a time (#666).
//!
//! **One dot per stage, not a line per step.** The operator, 2026-11-09: *"I
//! just want to know, for the high level metrics and prescriptions has the
//! data been fetched and normalised."* What each step found stays in
//! telemetry, for debugging.

use std::collections::BTreeSet;

use domain::planner::Discipline;

use crate::{
    CanonicalLayer, Part,
    next::{Found, Scope, Step, StepFailure},
};

/// A stage of collecting: every part but delivering is one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Fetching,
    Normalising,
    Canonicalising,
}

impl Stage {
    pub const ALL: [Self; 3] = [Self::Fetching, Self::Normalising, Self::Canonicalising];

    /// The stage `part` is in, `None` for delivering.
    pub const fn of(part: &Part) -> Option<Self> {
        match part {
            Part::Fetching(_) => Some(Self::Fetching),
            Part::Normalising(_) => Some(Self::Normalising),
            Part::Canonicalising(_) => Some(Self::Canonicalising),
            Part::Delivering => None,
        }
    }
}

/// Where one stage of what a page reads stands.
///
/// **Ordered from what most needs saying**: a stage is as far on as its
/// furthest-behind part, so one failed part fails it and one unfinished part
/// leaves it unfinished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Collected {
    Failed,
    Unfinished,
    Done,
    /// Not fetched, because nothing new could be there yet.
    NotDue,
    /// No run has reached it: one that stopped before it, or none since the
    /// server started.
    NotRun,
}

/// The parts `step` collects or derives, none for a step that is neither.
pub fn parts(step: &Step) -> BTreeSet<Part> {
    match step {
        Step::Joining(parts) => parts.clone(),
        Step::Fetching(stream) => BTreeSet::from([Part::Fetching(stream.clone())]),
        Step::Normalising(stream) => BTreeSet::from([Part::Normalising(stream.clone())]),
        Step::Canonicalising(Discipline::Gym) => {
            BTreeSet::from([Part::Canonicalising(CanonicalLayer::Gym)])
        }
        Step::Canonicalising(Discipline::Cycling) => {
            BTreeSet::from([Part::Canonicalising(CanonicalLayer::Cycling)])
        }
        Step::CanonicalisingWeighIns => {
            BTreeSet::from([Part::Canonicalising(CanonicalLayer::WeighIns)])
        }
        Step::FetchingCalendar(_) | Step::RefreshingClasses | Step::Committing | Step::Skipping => {
            BTreeSet::new()
        }
    }
}

/// Where `stage` of `scope` stands, from each step of the run in the order it
/// started, with its outcome once it ended. `None` where `scope` has no part
/// in that stage: sleep is never canonicalised.
///
/// **A part no step has reached is unfinished while the run is going**, since
/// it has yet to start, and not run once it has ended.
pub fn collected<'a>(
    scope: &Scope,
    stage: Stage,
    steps: &(impl IntoIterator<Item = (&'a Step, Option<&'a Result<Found, StepFailure>>)> + Clone),
    going: bool,
) -> Option<Collected> {
    scope
        .parts()
        .iter()
        .filter(|part| Stage::of(part) == Some(stage))
        .map(|part| {
            let latest = steps
                .clone()
                .into_iter()
                .filter(|(step, _)| parts(step).contains(part))
                .last();
            match latest {
                None if going => Collected::Unfinished,
                None => Collected::NotRun,
                Some((_, None)) => Collected::Unfinished,
                Some((_, Some(Err(_)))) => Collected::Failed,
                Some((_, Some(Ok(Found::NotDue(_))))) => Collected::NotDue,
                Some((_, Some(Ok(_)))) => Collected::Done,
            }
        })
        .min()
}

#[cfg(test)]
mod tests {
    use domain::landing::LandingStream;

    use super::{Collected, Stage, collected};
    use crate::{
        CanonicalLayer, Part,
        next::{Found, NoAccess, NotDue, Scope, Step, StepFailure},
    };

    fn stream(name: &str) -> LandingStream {
        LandingStream::try_from(name).expect("a stream")
    }

    /// Body weight's parts, as its page collects them in miniature.
    fn body_weight() -> Scope {
        Scope::new([
            Part::Fetching(stream("withings.measurements")),
            Part::Normalising(stream("withings.measurements")),
            Part::Normalising(stream("spreadsheets.files")),
            Part::Canonicalising(CanonicalLayer::WeighIns),
        ])
    }

    fn no_access() -> Result<Found, StepFailure> {
        Err(StepFailure::NoAccess(NoAccess {
            detail: "no withings credential".to_owned(),
        }))
    }

    /// **A stage the scope has no part in is absent**, and one no run has
    /// reached is unfinished while a run goes and not run after.
    #[test]
    fn a_stage_without_parts_is_absent_and_one_unreached_waits_on_the_run() {
        let sleep = Scope::new([
            Part::Fetching(stream("garmin.sleep")),
            Part::Normalising(stream("garmin.sleep")),
        ]);
        let none: [(&Step, Option<&Result<Found, StepFailure>>); 0] = [];
        assert_eq!(collected(&sleep, Stage::Canonicalising, &none, true), None);
        assert_eq!(
            collected(&sleep, Stage::Fetching, &none, true),
            Some(Collected::Unfinished)
        );
        assert_eq!(
            collected(&sleep, Stage::Fetching, &none, false),
            Some(Collected::NotRun)
        );
    }

    /// **One failed part fails the stage, and one going leaves it
    /// unfinished**, however many others are done.
    #[test]
    fn a_stage_is_as_far_on_as_its_furthest_behind_part() {
        let fetching = Step::Fetching(stream("withings.measurements"));
        let spreadsheets = Step::Normalising(stream("spreadsheets.files"));
        let withings = Step::Normalising(stream("withings.measurements"));
        let failed = no_access();
        let written = Ok(Found::Unchanged);
        let steps = [
            (&fetching, Some(&failed)),
            (&spreadsheets, Some(&written)),
            (&withings, None),
        ];
        let scope = body_weight();

        assert_eq!(
            collected(&scope, Stage::Fetching, &steps, true),
            Some(Collected::Failed)
        );
        assert_eq!(
            collected(&scope, Stage::Normalising, &steps, true),
            Some(Collected::Unfinished)
        );
        assert_eq!(
            collected(&scope, Stage::Canonicalising, &steps, true),
            Some(Collected::Unfinished)
        );

        let steps = [
            (&fetching, Some(&failed)),
            (&spreadsheets, Some(&written)),
            (&withings, Some(&written)),
        ];
        assert_eq!(
            collected(&scope, Stage::Normalising, &steps, false),
            Some(Collected::Done)
        );
        assert_eq!(
            collected(&scope, Stage::Canonicalising, &steps, false),
            Some(Collected::NotRun)
        );
    }

    /// **A fetch not due is grey, and a part another run collected is done
    /// when that run ends**: the run joined it rather than doing it twice.
    #[test]
    fn not_due_is_its_own_and_a_joined_part_is_done() {
        let fetching = Step::Fetching(stream("withings.measurements"));
        let joining = Step::Joining(
            [
                Part::Normalising(stream("withings.measurements")),
                Part::Normalising(stream("spreadsheets.files")),
                Part::Canonicalising(CanonicalLayer::WeighIns),
            ]
            .into(),
        );
        let not_due = Ok(Found::NotDue(NotDue::UntilTomorrow));
        let joined = Ok(Found::Joined);
        let steps = [(&fetching, Some(&not_due)), (&joining, Some(&joined))];
        let scope = body_weight();

        assert_eq!(
            collected(&scope, Stage::Fetching, &steps, false),
            Some(Collected::NotDue)
        );
        assert_eq!(
            collected(&scope, Stage::Normalising, &steps, false),
            Some(Collected::Done)
        );
        assert_eq!(
            collected(&scope, Stage::Canonicalising, &steps, false),
            Some(Collected::Done)
        );
    }
}
