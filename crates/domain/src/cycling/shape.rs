//! What a set of rides adds up to, and how close one selection is to another.
//!
//! **The method is the operator's and predates this code.** It was worked out by
//! hand for Peak (`docs/cycling-peak-your-power-zones.md`) and applied again to
//! Build (decision 0032): express a candidate as proportions of timed ride, and
//! score it by the summed absolute difference in percentage points against the
//! whole programme's proportions. Lower is closer.
//!
//! **Two rules, and only the first is arithmetic.**
//!
//! - A **mesocycle is three working microcycles and a deload** — 3:1. That is the
//!   operator's, and it is the criterion by which two programmes are said to
//!   coincide. [`hard_share`] is what makes it checkable.
//! - Among candidates, the closest is the one that [`diverges`] least.
//!
//! **Two axes, and a programme moves them independently.**
//! [`total`](ZoneProfile::total) is how much riding there is and
//! [`intensity`](ZoneProfile::intensity) is how hard it is, and
//! [`tss`](ZoneProfile::tss) is the two multiplied. Carrying all three is not
//! redundancy: *Boost Your Base* raises volume at a flat intensity, *Build*
//! raises intensity at a flat volume, and a gym cycle sheds volume to buy
//! intensity — and the product alone cannot tell those apart. It is also why a
//! deload is easier to find than a peak. **A deload drops both axes at once**,
//! so every metric agrees where it is; a peak moves one axis, so metrics that
//! weight the axes differently disagree.
//!
//! **A microcycle is weighed two ways, and they can disagree.**
//! [`hard_share`](ZoneProfile::hard_share) thresholds at zone four;
//! [`tss`](ZoneProfile::tss) multiplies time by intensity across every zone. The
//! first is what 3:1 was stated in and cannot see a programme built entirely
//! below threshold — *Boost Your Base* is eight microcycles of flat zeros to it.
//! The second sees that programme's structure and disagrees with the first about
//! where *Build*'s mesocycle starts. **Which one bounds a mesocycle is a
//! training judgement and is not settled here**. [`mesocycles`] takes whichever
//! is handed to it, and the two agree wherever both can see: on Build and on
//! Peak. Only TSS can see *Boost Your Base* at all.
//!
//! **The divergence score is a heuristic and is not defended here.** It ranks by
//! zone profile alone: it knows nothing about how the sessions are spaced, about
//! which of them carries a test, or about what the other discipline is doing that
//! week. It has agreed with the operator's judgement twice. Twice is not a proof.

use std::collections::BTreeMap;

use super::{PowerZone, session::Ride};

/// Time at each zone across some rides, in seconds.
///
/// A profile is *not* normalised: two profiles of the same shape and different
/// volume are different facts, and [`shares`] is where volume is discarded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneProfile(BTreeMap<PowerZone, u64>);

impl ZoneProfile {
    /// Sum the time in zone of every ride given.
    ///
    /// **A ride with no zones contributes nothing**, which is right rather than
    /// convenient: an FTP test measures the number the zones are shares of, so
    /// it has no share of its own to add.
    pub fn of<'a>(rides: impl IntoIterator<Item = &'a Ride>) -> Self {
        let mut totals: BTreeMap<PowerZone, u64> = BTreeMap::new();
        for ride in rides {
            for (zone, seconds) in ride.time_in_zone() {
                *totals.entry(zone).or_default() += seconds;
            }
        }
        Self(totals)
    }

    #[must_use]
    pub fn seconds_at(&self, zone: PowerZone) -> u64 {
        self.0.get(&zone).copied().unwrap_or_default()
    }

    /// Every zone with time in it, lightest first.
    pub fn iter(&self) -> impl Iterator<Item = (PowerZone, u64)> + '_ {
        self.0.iter().map(|(zone, seconds)| (*zone, *seconds))
    }

    /// Total timed riding.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.0.values().sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Each zone as a percentage of timed riding.
    ///
    /// Empty where nothing was ridden — a week of rest has no shape, and
    /// dividing by its zero would invent one.
    #[must_use]
    pub fn shares(&self) -> BTreeMap<PowerZone, f64> {
        let total = self.total();
        if total == 0 {
            return BTreeMap::new();
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "seconds of riding; f64 is exact far past any plausible total"
        )]
        let ratio = |seconds: u64| 100.0 * seconds as f64 / total as f64;
        self.0
            .iter()
            .map(|(zone, seconds)| (*zone, ratio(*seconds)))
            .collect()
    }

    /// How hard this riding is, independent of how much of it there is.
    ///
    /// **The second of the two axes a programme moves** — [`total`](Self::total)
    /// is the first. Coggan's intensity factor as the zone plan implies it: the
    /// time-weighted quadratic mean of the zones' midpoints, in percent of FTP.
    /// An hour of zone two and a fortnight of it score the same, which is the
    /// point.
    ///
    /// **Quadratic rather than arithmetic, and not as a choice.** It is the mean
    /// that makes the identity below hold, and squaring is how TSS weights
    /// intensity in the first place. An arithmetic mean would under-report any
    /// ride that mixes hard and easy.
    ///
    /// Zero where nothing was ridden — no riding has no intensity, and inventing
    /// one would make an empty week look easy rather than absent.
    #[must_use]
    pub fn intensity(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "seconds of riding; f64 is exact far past any plausible total"
        )]
        let weighted: f64 = self
            .0
            .iter()
            .map(|(zone, seconds)| {
                let midpoint = zone.band().midpoint_percent();
                *seconds as f64 * midpoint * midpoint
            })
            .sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "seconds of riding; f64 is exact far past any plausible total"
        )]
        let mean = weighted / total as f64;
        mean.sqrt()
    }

    /// Coggan's Training Stress Score for this riding, from the zone plan alone.
    ///
    /// `TSS = Σ (seconds × IF²) / 36`, where `IF` is each zone's
    /// [midpoint](super::ZoneBand::midpoint_percent) as a share of FTP. An hour
    /// held exactly at threshold scores 100, which is the definition rather than
    /// a calibration.
    ///
    /// **Why this exists beside [`hard_share`](Self::hard_share).** Hard share
    /// thresholds at Z4, and *Boost Your Base* contains no Z4 at all — so it
    /// reports a flat row of zeros across eight microcycles and finds no
    /// structure in a programme that plainly has some. The operator, 2026-09-05:
    ///
    /// > "percentage of Z4 is too coarse... it also increases intensity, it's
    /// > just that it increases intensity from Z2 to Z3."
    ///
    /// TSS sees that, because it multiplies time by intensity rather than
    /// thresholding intensity and counting time.
    ///
    /// **No FTP and no heart rate are needed**, so a class scores before anyone
    /// rides it. That is what makes this a property of the *programme* — a fact
    /// about what was prescribed, not a measurement of what was performed.
    ///
    /// **It is the two axes multiplied**, and exactly so:
    ///
    /// ```text
    /// tss  ==  total() × (intensity() / 100)²  /  36
    /// ```
    ///
    /// So nothing is lost by carrying [`total`](Self::total) and
    /// [`intensity`](Self::intensity) beside it — and something is gained, because
    /// a programme moves the two independently and the product hides which.
    /// *Boost Your Base* raises volume at a flat intensity; *Build* raises
    /// intensity at a flat volume; an SBS cycle sheds volume to buy intensity.
    /// All three can look alike in TSS alone.
    ///
    /// A ride with no zones contributes nothing, for the same reason it
    /// contributes no share: the FTP test measures the number the zones are
    /// shares of, so it has no intensity of its own to score.
    #[must_use]
    pub fn tss(&self) -> f64 {
        #[expect(
            clippy::cast_precision_loss,
            reason = "seconds of riding; f64 is exact far past any plausible total"
        )]
        let score = |zone: PowerZone, seconds: u64| {
            let intensity = zone.band().midpoint_percent() / 100.0;
            seconds as f64 * intensity * intensity / 36.0
        };
        self.0
            .iter()
            .map(|(zone, seconds)| score(*zone, *seconds))
            .sum()
    }

    /// Time at zone four and above, as a percentage of timed riding.
    ///
    /// **What makes 3:1 checkable.** A deload microcycle is one where this is
    /// zero while real riding still happens — Peak's fourth week reproduces its
    /// first with every hard zone removed, and Build's fifth does the same.
    #[must_use]
    pub fn hard_share(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        let hard: u64 = self
            .0
            .iter()
            .filter(|(zone, _)| **zone >= PowerZone::Four)
            .map(|(_, seconds)| *seconds)
            .sum();
        #[expect(
            clippy::cast_precision_loss,
            reason = "seconds of riding; f64 is exact far past any plausible total"
        )]
        let share = 100.0 * hard as f64 / total as f64;
        share
    }
}

/// How far one selection's shape sits from another's, in percentage points.
///
/// Summed across all seven zones, so a candidate that is three points light on
/// one zone and three heavy on another scores six rather than zero. Lower is
/// closer, and zero is the same shape at any volume.
#[must_use]
pub fn diverges(candidate: &ZoneProfile, from: &ZoneProfile) -> f64 {
    let (a, b) = (candidate.shares(), from.shares());
    PowerZone::ALL
        .into_iter()
        .map(|zone| {
            let mine = a.get(&zone).copied().unwrap_or_default();
            let theirs = b.get(&zone).copied().unwrap_or_default();
            (mine - theirs).abs()
        })
        .sum()
}

/// How far above a run's lightest microcycle still counts as its bottom level.
///
/// **Checked rather than chosen** (decision 0034). Across the five mesocycles of
/// Base, Build and Peak, everything at the floor's level sits 0–2% above it and
/// everything a level up sits 10–162% above, so any tolerance between 3% and 9%
/// gives the same answer everywhere. Five is the middle of a fivefold berth.
const BOTTOM_LEVEL_TOLERANCE: f64 = 5.0;

/// Which of a run's microcycles sit at its bottom level.
///
/// **A shape is levels, not ranks** (decision 0034). The operator, 2026-09-05,
/// on Peak's first mesocycle:
///
/// > "Peak µ1-4 are clearly a 1-2-2-1 pattern, if the numbers were the other way
/// > around and they went 113, 126, 124, 114, they still would be"
///
/// So 113 and 114 are one level whichever comes first, and asking which is the
/// strict minimum asks the wrong question. The floor is the run's lightest and
/// everything within [`BOTTOM_LEVEL_TOLERANCE`] of it shares that level.
///
/// **Metric-agnostic on purpose.** It is handed scores, not rides, because the
/// same question is asked of a gym cycle scored by INOL as of a cycling
/// mesocycle scored by [`tss`](ZoneProfile::tss). It lives here because cycling
/// is the only caller today.
#[must_use]
pub fn bottom_level(run: &[f64]) -> Vec<bool> {
    let Some(floor) = run.iter().copied().reduce(f64::min) else {
        return Vec::new();
    };
    if floor <= 0.0 {
        // A microcycle that scores nothing gives no floor to take a share of —
        // an FTP test week is exactly this, since a ride with no zone scores no
        // TSS. Everything at nothing is the bottom, and nothing else is.
        return run.iter().map(|score| *score <= 0.0).collect();
    }
    run.iter()
        .map(|score| 100.0 * (score - floor) / floor <= BOTTOM_LEVEL_TOLERANCE)
        .collect()
}

/// Every run of `length` microcycles that ends in a deload.
///
/// **What makes a mesocycle checkable** (decisions 0032 and 0034). The
/// operator's description is three working microcycles and a deload, and what
/// makes it decidable is that the *last* sits at the run's bottom level —
/// **not** that the ones before it do not. Peak's first mesocycle opens at the
/// bottom level as well, reading `1-2-2-1`, and is a mesocycle nonetheless.
///
/// **A run that is entirely bottom level is not one**, because then nothing in
/// it is working. That is what keeps a programme with no hard riding at all from
/// reporting a mesocycle everywhere, and it is the whole of the guard.
///
/// This supersedes an earlier `is_three_to_one`, which took hard shares and
/// asked whether the last was zero. That could not see *Boost Your Base*, which
/// contains no zone four at all and so is eight zeros to it — the failure that
/// issue #71 opened on.
#[must_use]
pub fn mesocycles(scores: &[f64], length: usize) -> Vec<std::ops::Range<usize>> {
    if length == 0 {
        return Vec::new();
    }
    scores
        .windows(length)
        .enumerate()
        .filter(|(_, run)| {
            let floor = bottom_level(run);
            let ends_low = floor.last().copied().unwrap_or_default();
            let has_working = floor.iter().any(|at_bottom| !at_bottom);
            ends_low && has_working
        })
        .map(|(at, _)| at..at + length)
        .collect()
}
