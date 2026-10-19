//! Which days a chart shows (#252, #497).
//!
//! **The current mesocycle or macrocycle from its start to today, or
//! everything recorded** (operator, 2026-10-14 and 2026-10-19). No microcycle:
//! a week is not long enough to read a trend over. A range only chooses which
//! days are shown; whatever a series derives, it derives over the whole record.

use jiff::civil::Date;

use crate::plan::Span;

/// The days a range runs over, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub from: Date,
    pub to: Date,
}

impl Range {
    #[must_use]
    pub fn covers(self, day: Date) -> bool {
        day >= self.from && day <= self.to
    }

    /// From the earliest day to the latest; `None` for no days.
    pub fn spanning(days: impl IntoIterator<Item = Date>) -> Option<Self> {
        days.into_iter().fold(None, |range, day| {
            Some(
                range.map_or(Self { from: day, to: day }, |range: Self| Self {
                    from: range.from.min(day),
                    to: range.to.max(day),
                }),
            )
        })
    }
}

/// The days of each run that fall in `range`, dropping a run with none: a
/// trend is cut at the range's edges, never joined across them.
pub(super) fn runs_within<T: Copy>(
    runs: &[Vec<T>],
    range: Range,
    on: impl Fn(&T) -> Date,
) -> Vec<Vec<T>> {
    runs.iter()
        .map(|run| {
            run.iter()
                .filter(|day| range.covers(on(day)))
                .copied()
                .collect::<Vec<_>>()
        })
        .filter(|run| !run.is_empty())
        .collect()
}

/// Which range a chart is read over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Over {
    Mesocycle,
    Macrocycle,
    All,
}

impl Over {
    pub const ALL: [Self; 3] = [Self::Mesocycle, Self::Macrocycle, Self::All];
}

/// The current mesocycle and macrocycle on a day, each from its start to that
/// day (operator, 2026-10-14, #252): what has happened within each so far, not
/// a look-back of its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ranges {
    /// `None` on a day no mesocycle is planned for.
    pub mesocycle: Option<Range>,
    /// `None` on a day no plan is in force.
    pub macrocycle: Option<Range>,
}

impl Ranges {
    /// **A macrocycle is a planned one** (operator, 2026-10-14, #443): it runs
    /// from the first day of the plan in force, and a school term with no plan
    /// in it is not a macrocycle.
    pub fn on(today: Date, mesocycles: &[Span], plans: &[Span]) -> Self {
        let current = |spans: &[Span]| {
            spans
                .iter()
                .find(|span| span.covers(today))
                .map(|span| Range {
                    from: span.start(),
                    to: today,
                })
        };
        Self {
            mesocycle: current(mesocycles),
            macrocycle: current(plans),
        }
    }

    /// The days `over` names, where `recorded` is every day the series has
    /// a figure on; `None` where no such cycle is planned, or for everything
    /// when nothing was recorded.
    #[must_use]
    pub const fn over(self, over: Over, recorded: Option<Range>) -> Option<Range> {
        match over {
            Over::Mesocycle => self.mesocycle,
            Over::Macrocycle => self.macrocycle,
            Over::All => recorded,
        }
    }
}
