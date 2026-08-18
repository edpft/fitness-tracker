//! Prilepin's chart: how much work an intensity admits.
//!
//! A. S. Prilepin read the training logs of a great many weightlifters and
//! tabulated, for each band of intensity, how many repetitions a set should hold
//! and how many lifts a session should total. It is the closest thing strength
//! training has to a published constant, and it is what tells a phase running
//! many sets how far below a maximum to sit — see research D11.
//!
//! ```text
//! %1RM     reps/set   total lifts   optimal
//! < 70        3–6        18–30        24
//! 70–79       3–6        12–24        18
//! 80–89       2–4        10–20        15
//! 90+         1–2         4–10         7
//! ```
//!
//! **Its provenance is weightlifting**, and a squat spends far longer under load
//! than a snatch does, so these totals are an upper bound here rather than a
//! target. The chart is used that way: the band's maximum caps the sets a week
//! runs, and nothing tries to reach its optimum.

use super::parameters::Percentage;

/// What one band of the chart admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    /// Basis points, inclusive, of the one-rep maximum.
    pub from: i32,
    pub fewest_lifts: u32,
    pub most_lifts: u32,
    pub optimal_lifts: u32,
}

/// The chart, heaviest band last.
pub const CHART: [Band; 4] = [
    Band {
        from: 0,
        fewest_lifts: 18,
        most_lifts: 30,
        optimal_lifts: 24,
    },
    Band {
        from: 7_000,
        fewest_lifts: 12,
        most_lifts: 24,
        optimal_lifts: 18,
    },
    Band {
        from: 8_000,
        fewest_lifts: 10,
        most_lifts: 20,
        optimal_lifts: 15,
    },
    Band {
        from: 9_000,
        fewest_lifts: 4,
        most_lifts: 10,
        optimal_lifts: 7,
    },
];

/// The band a load falls in.
///
/// Total rather than optional: every load sits in some band, and the lightest
/// one starts at zero.
#[must_use]
pub fn band(load: Percentage) -> Band {
    let mut found = CHART[0];
    for candidate in CHART {
        if load.as_basis_points() >= candidate.from {
            found = candidate;
        }
    }
    found
}

/// How many sets of `reps` this load admits, up to a ceiling the caller sets.
///
/// The ceiling is what the programme would run if the chart did not object; this
/// returns the smaller of the two. Never zero — a week prescribing no sets is
/// not a lighter week, it is a missing one, and a repetition count the chart
/// cannot fit at all is a question for the caller rather than something to
/// answer with silence.
#[must_use]
pub fn sets_across(load: Percentage, reps: u32, ceiling: u32) -> u32 {
    if reps == 0 {
        return ceiling;
    }
    let admitted = band(load).most_lifts / reps;
    admitted.min(ceiling).max(1)
}
