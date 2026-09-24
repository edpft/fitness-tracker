//! What the operator answered, and the [`GymMesocycle`] it makes.
//!
//! **This replaced a document format.** Until 2026-09-06 a programme was a TOML
//! file: the wizard asked its questions, wrote one, and the reader parsed it
//! back. The file stood between the answers and the store, and it had to
//! re-check by hand every rule the types already state — a `test` with an
//! anchor, a `linear` with an entry test, an `sbs` with a duration. Eight such
//! checks existed and every one of them is now unrepresentable: [`Shape`] has a
//! variant per template and no field for what that template cannot say.
//!
//! **It is here rather than at an adapter because nothing about it is one.** The
//! format was a vendor surface and lived in `infrastructure` under § 21; what is
//! left is the assembly of a domain type out of domain values, which both
//! driving adapters need and neither owns. A block's calendar prepends a week
//! for its entry test, an SBS cycle is four weeks whatever it is asked, and a
//! test is one — that is knowledge about programmes, and a copy of it in `cli`
//! would be a copy `web` had to keep in step.

use jiff::{civil::Date, tz::TimeZone};

use crate::{
    gym::exercise::Exercise,
    measure::RepCount,
    prescription::{
        anchor::Anchor,
        block::{BlockPeriodisation, EntryTest},
        linear::{Linear, Primary, PrimaryPattern, SlotFills},
        mesocycle::{GymMesocycle, InconsistentMesocycle, Progression},
        parameters::GenerationParameters,
        sbs::{self, Sbs, WEEKS},
        schedule::{Calendar, InvalidCalendar, Skip},
        test::{Test, Tested},
    },
    provider::ProvidedFrom,
    schedule::{SessionRole, TrainingWeek},
};

/// Why a set of answers does not make a programme.
///
/// **Two variants and no third.** With the answers typed, the only things left
/// to go wrong are the two the domain already refuses: a calendar that cannot
/// hold the block, and a programme inconsistent with itself. Everything a
/// document could get wrong — a missing field, an unparseable load, a template
/// nobody has heard of — went with the text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthoringError {
    #[error("these weeks do not make a calendar: {0}")]
    Calendar(#[from] InvalidCalendar),
    #[error(transparent)]
    GymMesocycle(#[from] InconsistentMesocycle),
}

/// What every programme is asked, whatever its template.
///
/// Few and short, which is the point: the seventeen slots are the long part of
/// authoring, and every field here is a line the operator already knows the
/// answer to.
///
/// **The primary is a [`PrimaryPattern`] and an [`Exercise`], not one thing.**
/// The pattern names the slot the programme is built around and the exercise is
/// what fills it; they agree in every valid programme, and
/// [`check_primary`](crate::prescription::check_primary) exists because they can
/// be answered so that they do not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authored {
    pub start: Date,
    pub pattern: PrimaryPattern,
    pub primary_exercise: Exercise,
    /// The operator's week for the gym as of the start date, read from the
    /// diary.
    ///
    /// **Consulted, not kept.** Nothing built from these answers holds it: a
    /// calendar needs it to count training weeks and the checks below need it
    /// to see whether the gating session is one the week offers, and both
    /// questions are asked again from the diary whenever they come up. Storing
    /// it beside the programme is what `gym_weekday` did, once per mesocycle,
    /// for a fact the schedule already held (issue #63).
    pub week: TrainingWeek,
    /// What this template needs beyond the five facts above.
    pub shape: Shape,
}

/// The part of a programme that differs by template.
///
/// **A variant per template, and each carries only what that template has.**
/// This is where the document reader's eight `refuse_unused` calls went: an SBS
/// cycle has no gating role because the chart says which session advances it,
/// and a linear programme has no entry test. None of them is a field that can be
/// set and then refused.
///
/// **Only the test carries a load, and only sometimes.** A progression states
/// shares — sets, repetitions, percentages — and what those are shares of is
/// read off the record when a session is prescribed. The one exception is the
/// test at the front of a sequence, which has nothing behind it to read and so
/// states an asserted anchor.
///
/// **Nor is a duration, except where it is really asked.** A test is one week
/// (decision 0013) and the chart is four (decision 0024); only [`Self::Linear`]
/// and [`Self::Block`] are told how long they run, and a block's number counts
/// its phase weeks with the entry test in front of them rather than among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// One week, measuring.
    Test {
        reps: RepCount,
        /// Which microcycle of which published programme this week is, where a
        /// publisher wrote it. `None` is a week that exists only to measure a
        /// lift before something else begins.
        provided: Option<ProvidedFrom>,
        /// The anchor this week states, where it is the test at the front of a
        /// sequence and there is nothing behind it to read one from. `None`
        /// defers to whatever preceded it.
        asserted: Option<Anchor>,
    },
    /// A top-set ladder climbing at a rate, for a stated span.
    Linear { gating: SessionRole, weeks: u32 },
    /// Phases to a planned endpoint, over a span of phase weeks.
    Block {
        gating: SessionRole,
        /// Phase weeks. An entry test adds one in front of them.
        weeks: u32,
        entry_test: Option<EntryTest>,
    },
    /// A mesocycle taken from an external programme rather than derived here:
    /// four weeks, every set stated, its test the last session (decision 0024).
    Provided {
        /// Which microcycles of which external programme.
        from: ProvidedFrom,
    },
}

impl Shape {
    /// The stable key, matching [`GymMesocycle::template`].
    ///
    /// Here as well as on `GymMesocycle` because the wizard names the template
    /// before it has a programme to ask.
    #[must_use]
    pub const fn template(&self) -> &'static str {
        match self {
            Self::Test { .. } => "test",
            Self::Linear { .. } => "linear",
            Self::Block { .. } => "block",
            Self::Provided { .. } => "sbs",
        }
    }

    /// The calendar weeks this shape occupies, before any interruption.
    ///
    /// **What the schedule is asked about**, so the losses can be read off the
    /// diary and handed back to [`programme`]. Nominal, which is what stops it
    /// being circular: the interruptions come from the window, so the window
    /// cannot come from the interruptions.
    #[must_use]
    pub const fn calendar_weeks(&self) -> u32 {
        match self {
            Self::Test { .. } => Test::WEEKS,
            Self::Provided { .. } => WEEKS,
            Self::Linear { weeks, .. } => *weeks,
            // The entry test is a week in front of the phases rather than one of
            // them (decision 0013), so a nine-week block that measures its own
            // entry occupies ten.
            Self::Block {
                weeks, entry_test, ..
            } => weeks.saturating_add(if entry_test.is_some() { 1 } else { 0 }),
        }
    }
}

impl Authored {
    /// The days this programme runs across, first and last.
    ///
    /// **What the schedule is asked about**, ahead of [`programme`]: what the
    /// gym loses is a question about a span of dates, and the diary is the only
    /// thing that can answer it.
    ///
    /// **The nominal span, before interruptions**, which is what stops it being
    /// circular: the losses are read from the window, so the window cannot be
    /// read from the losses. [`Calendar`] refuses an interruption outside the
    /// block, so asking over exactly this span is also what keeps every derived
    /// skip admissible.
    ///
    /// **A limitation, declared rather than solved.** `Calendar::calendar_weeks`
    /// walks the skips, so a week in which *every* session is lost pushes the
    /// block's real end past this span, and a day lost in that extension is not
    /// consulted. It takes a whole training week going at once to happen.
    ///
    /// `None` only where the arithmetic overflows the calendar, which is a start
    /// date no programme would be authored from.
    #[must_use]
    pub fn window(&self) -> Option<(Date, Date)> {
        let days = i64::from(self.shape.calendar_weeks())
            .saturating_mul(7)
            .saturating_sub(1);
        let last = self.start.checked_add(jiff::Span::new().days(days)).ok()?;
        Some((self.start, last))
    }
}

/// Assemble the programme these answers describe.
///
/// The interruptions are resolved by the caller and passed in rather than looked
/// up: what the block loses is a question for the schedule, and settling it at
/// authoring is what keeps the stored programme complete on its own — a holiday
/// coming off the calendar afterwards cannot retroactively move what it
/// prescribed.
///
/// # Errors
///
/// [`AuthoringError`] where the weeks do not make a calendar, or where the
/// programme is inconsistent with itself.
pub fn programme(
    authored: Authored,
    fills: SlotFills,
    interruptions: &[Skip],
    zone: TimeZone,
    parameters: &GenerationParameters,
) -> Result<GymMesocycle, AuthoringError> {
    let Authored {
        start,
        pattern,
        primary_exercise,
        week,
        shape,
    } = authored;

    match shape {
        Shape::Test {
            reps,
            provided,
            asserted,
        } => {
            if !week.offers(Test::ROLE) {
                return Err(
                    InconsistentMesocycle::TestNeverRunsItsSession { role: Test::ROLE }.into(),
                );
            }
            let calendar = Test::week(start, interruptions, week, zone)?;
            Ok(GymMesocycle::Test(Test::new(
                Tested::new(pattern, primary_exercise, reps),
                fills,
                calendar,
                provided,
                asserted,
            )?))
        }
        Shape::Linear { gating, weeks } => {
            gates_on_a_session_the_week_offers(&week, gating)?;
            let calendar = Calendar::new(start, weeks, interruptions, week, zone)?;
            Ok(GymMesocycle::Progression(Progression::Linear(Linear::new(
                Primary::new(pattern, primary_exercise, gating),
                fills,
                calendar,
                parameters,
            )?)))
        }
        Shape::Block {
            gating,
            weeks,
            entry_test,
        } => {
            gates_on_a_session_the_week_offers(&week, gating)?;
            let calendar = BlockPeriodisation::weeks(
                start,
                weeks,
                entry_test.is_some(),
                interruptions,
                week,
                zone,
            )?;
            Ok(GymMesocycle::Progression(Progression::BlockPeriodisation(
                BlockPeriodisation::new(
                    Primary::new(pattern, primary_exercise, gating),
                    fills,
                    entry_test,
                    calendar,
                )?,
            )))
        }
        Shape::Provided { from } => {
            gates_on_a_session_the_week_offers(&week, sbs::GATING)?;
            let calendar = Calendar::new(start, WEEKS, interruptions, week, zone)?;
            Ok(GymMesocycle::Progression(Progression::Provided {
                from,
                cycle: Sbs::new(pattern, primary_exercise, fills, calendar)?,
            }))
        }
    }
}

/// A programme gating on a role the operator's week never offers would never
/// advance.
///
/// **Asked here rather than by the programme**, and it moved on 2026-09-20. A
/// role belongs to a training slot now (issue #63), so this is a question about
/// a programme and a week together — and the week is superseded whenever the
/// operator's life changes, while the programme stays in the store. Held as a
/// type invariant it would make a mesocycle authored last month fail to load
/// this month, which is a stored fact being refused for something that is not
/// about it.
///
/// For an SBS cycle it is worse than a stalled ladder: the gating day is where
/// the maximum is *set*, so a cycle without one would prescribe every week off
/// the opening maximum for ever.
///
/// # Errors
///
/// [`InconsistentMesocycle::GatingRoleNeverRuns`] where the week offers no
/// session in the gating role.
fn gates_on_a_session_the_week_offers(
    week: &TrainingWeek,
    gating: SessionRole,
) -> Result<(), AuthoringError> {
    if week.offers(gating) {
        return Ok(());
    }
    Err(InconsistentMesocycle::GatingRoleNeverRuns { gating }.into())
}
