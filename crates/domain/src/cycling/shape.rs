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
//! **A microcycle is weighed two ways, and they can disagree.**
//! [`hard_share`](ZoneProfile::hard_share) thresholds at zone four;
//! [`tss`](ZoneProfile::tss) multiplies time by intensity across every zone. The
//! first is what 3:1 was stated in and cannot see a programme built entirely
//! below threshold — *Boost Your Base* is eight microcycles of flat zeros to it.
//! The second sees that programme's structure and disagrees with the first about
//! where *Build*'s mesocycle starts. **Which one bounds a mesocycle is a
//! training judgement and is not settled here**; both are offered and neither is
//! wired into [`is_three_to_one`].
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

/// Whether a run of microcycles is three working and then a deload.
///
/// **The coincidence criterion** (decision 0032). Takes the hard share of each
/// microcycle in order and answers whether the last is a deload and the ones
/// before it are not — which is what distinguishes a mesocycle from a
/// progression that merely rises.
#[must_use]
pub fn is_three_to_one(hard_shares: &[f64]) -> bool {
    let Some((last, working)) = hard_shares.split_last() else {
        return false;
    };
    working.len() == 3 && *last == 0.0 && working.iter().all(|share| *share > 0.0)
}
