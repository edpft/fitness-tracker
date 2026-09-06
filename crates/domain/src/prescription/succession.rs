//! When a measurement is too old to speak for a programme.
//!
//! **Two relations, and they are not the same** (decision 0012). The same plan
//! re-authored to correct it supersedes its earlier version, and `authored_at`
//! settles which version wins. A *different* plan, later in time, succeeds it:
//! both are real, and which one answers a question depends on the date being
//! asked about.
//!
//! One mechanism carried both until 2026-08-22, which is why authoring the
//! autumn block would have replaced the summer one rather than followed it.
//!
//! **What is left here is the recency rule.** The names and windows that carried
//! those two relations moved on 2026-09-06: identity is a plan's, so
//! [`PlanName`](crate::plan::PlanName) and [`PlanWindow`](crate::plan::PlanWindow)
//! hold it, and the name of a *published* programme went to
//! [`ProgrammeName`](crate::provider::ProgrammeName) beside the provider that
//! published it.

use jiff::civil::Date;

/// How many whole weeks separate two dates, counting Mondays.
///
/// **Not [`Calendar`](super::schedule::Calendar)'s offset, and this is the
/// reason** (decision 0013). That one divides a day difference by seven and Rust
/// truncates toward zero, so a test on Friday 18 September against a Monday 21
/// September start is −3 days → `0`, reading as the same week as the start; and
/// Friday 11 September is −10 days → `−1` where the rule wants `−2`. Reusing it
/// would pass a happy-path test and be wrong in the middle of every week.
///
/// Both dates are bucketed to their Monday first, so what is being counted is
/// calendar weeks rather than sevenths of a day span.
#[must_use]
pub fn weeks_between(from: Date, to: Date) -> i64 {
    i64::from((monday_of(to) - monday_of(from)).get_days()) / 7
}

/// The Monday of a date's week.
fn monday_of(date: Date) -> Date {
    let back = i64::from(date.weekday().to_monday_zero_offset());
    date.checked_sub(jiff::Span::new().days(back))
        .unwrap_or(date)
}

/// The weeks before a programme in which a test still speaks for it.
///
/// **The week before, or the week before that** — the operator's rule, stated on
/// 2026-08-22 (decision 0013). A maximum older than that is evidence about a
/// month ago rather than about the block it would be anchoring, whatever the
/// number says.
pub const RECENT_WEEKS: i64 = 2;

/// Whether a test taken on one date still anchors a programme starting on
/// another.
///
/// Says nothing about *which lift* was tested: that is the caller's question,
/// and a maximum in the wrong lift is not a stale test but a different one.
#[must_use]
pub fn is_recent_enough(tested: Date, start: Date) -> bool {
    let weeks = weeks_between(tested, start);
    (1..=RECENT_WEEKS).contains(&weeks)
}
