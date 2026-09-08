//! What the operator answered, and the [`Mesocycle`] it makes.
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
    gym::{Kg, exercise::Exercise},
    measure::RepCount,
    prescription::{
        anchor::{Anchor, Anchoring, Entry},
        block::{BlockPeriodisation, EntryTest},
        linear::{Linear, Primary, PrimaryPattern, SlotFills},
        mesocycle::{InconsistentMesocycle, Mesocycle, Progression},
        parameters::GenerationParameters,
        sbs::{Sbs, WEEKS},
        schedule::{Calendar, InvalidCalendar, SessionRole, Skip, Weekdays},
        test::{Test, TestTarget, Tested},
    },
    provider::ProvidedFrom,
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
    Mesocycle(#[from] InconsistentMesocycle),
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
    pub weekdays: Weekdays,
    /// What this template needs beyond the five facts above.
    pub shape: Shape,
}

/// The part of a programme that differs by template.
///
/// **A variant per template, and each carries only what that template has.**
/// This is where the document reader's eight `refuse_unused` calls went: a test
/// has no anchor because measuring is what it is for, an SBS cycle has no gating
/// role because the chart says which session advances it, a linear programme has
/// no entry test and a block has no opening. None of them is a field that can be
/// set and then refused.
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
        /// What the test attempts. [`TestTarget::Inherited`] takes it from the
        /// programme this follows, which is the ordinary case (decision 0013).
        target: TestTarget,
        /// Which microcycle of which published programme this week is, where a
        /// publisher wrote it. `None` is a week that exists only to measure a
        /// lift before something else begins.
        provided: Option<ProvidedFrom>,
    },
    /// A top-set ladder climbing at a rate, for a stated span.
    Linear {
        gating: SessionRole,
        weeks: u32,
        anchor: Anchor,
        /// Where the ladder opens. `None` derives it from the anchor, which is
        /// what the operator means when he does not say.
        opening: Option<Kg>,
    },
    /// Phases to a planned endpoint, over a span of phase weeks.
    Block {
        gating: SessionRole,
        /// Phase weeks. An entry test adds one in front of them.
        weeks: u32,
        anchor: Anchor,
        entry_test: Option<EntryTest>,
    },
    /// A mesocycle taken from an external programme rather than derived here:
    /// four weeks, every set stated, its test the last session (decision 0024).
    Provided {
        /// Which microcycles of which external programme.
        from: ProvidedFrom,
        /// A stated maximum, or the cycle before this one. A plan holding three
        /// of these can only state the first: the other two open from tests that
        /// have not happened when it is authored.
        anchor: Anchoring,
    },
}

impl Shape {
    /// The stable key, matching [`Mesocycle::template`].
    ///
    /// Here as well as on `Mesocycle` because the wizard names the template
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
) -> Result<Mesocycle, AuthoringError> {
    let Authored {
        start,
        pattern,
        primary_exercise,
        weekdays,
        shape,
    } = authored;

    match shape {
        Shape::Test {
            reps,
            target,
            provided,
        } => {
            let calendar = Test::week(start, interruptions, weekdays, zone)?;
            Ok(Mesocycle::Test(Test::new(
                Tested::new(pattern, primary_exercise, reps),
                fills,
                calendar,
                target,
                provided,
            )?))
        }
        Shape::Linear {
            gating,
            weeks,
            anchor,
            opening,
        } => {
            let calendar = Calendar::new(start, weeks, interruptions, weekdays, zone)?;
            Ok(Mesocycle::Progression(Progression::Linear(Linear::new(
                Primary::new(pattern, primary_exercise, gating),
                fills,
                Entry::new(anchor, opening),
                calendar,
                parameters,
            )?)))
        }
        Shape::Block {
            gating,
            weeks,
            anchor,
            entry_test,
        } => {
            let calendar = BlockPeriodisation::weeks(
                start,
                weeks,
                entry_test.is_some(),
                interruptions,
                weekdays,
                zone,
            )?;
            Ok(Mesocycle::Progression(Progression::BlockPeriodisation(
                BlockPeriodisation::new(
                    Primary::new(pattern, primary_exercise, gating),
                    fills,
                    // A block's loads are every one of them a share of its
                    // anchor, so there is no opening to declare and none to
                    // derive.
                    Entry::derived(anchor),
                    entry_test,
                    calendar,
                )?,
            )))
        }
        Shape::Provided { from, anchor } => {
            let calendar = Calendar::new(start, WEEKS, interruptions, weekdays, zone)?;
            Ok(Mesocycle::Progression(Progression::Provided {
                from,
                cycle: Sbs::new(pattern, primary_exercise, fills, anchor, calendar)?,
            }))
        }
    }
}
