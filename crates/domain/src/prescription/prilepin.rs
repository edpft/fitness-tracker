//! Prilepin's chart: how much work an intensity admits.
//!
//! A. S. Prilepin read the training logs of a great many weightlifters and
//! tabulated, for each band of intensity, how many repetitions a set should hold
//! and how many lifts a session should total. It is the closest thing strength
//! training has to a published constant, and it is what places a phase running
//! many sets: the repetitions-per-set column says where its heaviest rung
//! belongs and the total-lifts column caps how many sets each rung runs. See
//! research D11 and D12.
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
//!
//! **The repetitions-per-set column is load-bearing too**, and it is what
//! anchors a phase rather than merely checking one. Read downwards it says where
//! a set of a given size belongs: threes and up are admissible at any intensity,
//! but a double first appears at 80% and a single at 90%. So a phase descending
//! towards a double has a floor the chart names, which is what
//! [`floor_for_sets_of`] returns.

use super::parameters::Percentage;

/// What one band of the chart admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    /// Basis points, inclusive, of the one-rep maximum.
    pub from: i32,
    /// The repetitions-per-set column, inclusive at both ends.
    pub fewest_reps: u32,
    pub most_reps: u32,
    pub fewest_lifts: u32,
    pub most_lifts: u32,
    pub optimal_lifts: u32,
}

/// The chart, heaviest band last.
pub const CHART: [Band; 4] = [
    Band {
        from: 0,
        fewest_reps: 3,
        most_reps: 6,
        fewest_lifts: 18,
        most_lifts: 30,
        optimal_lifts: 24,
    },
    Band {
        from: 7_000,
        fewest_reps: 3,
        most_reps: 6,
        fewest_lifts: 12,
        most_lifts: 24,
        optimal_lifts: 18,
    },
    Band {
        from: 8_000,
        fewest_reps: 2,
        most_reps: 4,
        fewest_lifts: 10,
        most_lifts: 20,
        optimal_lifts: 15,
    },
    Band {
        from: 9_000,
        fewest_reps: 1,
        most_reps: 2,
        fewest_lifts: 4,
        most_lifts: 10,
        optimal_lifts: 7,
    },
];

/// The lightest load at which the chart calls for sets of `reps`.
///
/// `None` where the chart's lightest band already admits them and there is
/// therefore no floor to speak of: threes and above are admissible at any
/// intensity, and only twos and singles have one. That case falls out of the
/// lightest band starting at zero, which is not a percentage.
///
/// **This is what anchors a descending phase.** A phase whose repetitions
/// descend to a double is heaviest at that double, and the chart says the
/// lightest load a double belongs at is 80% — so the phase's top rung is placed
/// by a published table rather than chosen.
#[must_use]
pub fn floor_for_sets_of(reps: u32) -> Option<Percentage> {
    CHART
        .into_iter()
        .find(|band| reps >= band.fewest_reps && reps <= band.most_reps)
        .and_then(|band| Percentage::from_basis_points(band.from).ok())
}

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
