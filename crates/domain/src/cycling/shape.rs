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

/// How far one selection's composition sits from another's, in percentage points.
///
/// Summed across all seven zones, so a candidate three points light on one zone
/// and three heavy on another scores six rather than zero. Lower is closer, and
/// zero is the same composition at any volume.
///
/// **What this cannot do, and no aggregation of it can.** It charges for a zone
/// in proportion to the *time* that zone occupies, and the zones that say what a
/// programme trains occupy almost none of it — losing every second of Build's Z6
/// costs 1.0 where a five-point wobble in Z2 costs 5. Squaring and dividing by
/// the zone's own share does not fix this: for a zone of share `e`, dropping it
/// entirely costs `e` either way, and dropping *half* of it costs `e/4` squared
/// against `e/2` summed — so the squared form is the *less* sensitive of the two
/// to a zone going missing. It is only harsher on a rare zone being
/// over-represented.
///
/// So a dropped zone is checked structurally by [`zones_lost`] rather than
/// scored here, the way a deload is checked by [`mesocycles`] rather than
/// scored. **Constraints refuse; scores rank what survives.**
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

/// The zones the reference trains that the candidate does not train at all.
///
/// **A structural check, not a score.** A selection that drops a zone outright
/// has stopped training something the programme trains, and that is a different
/// kind of fact from being a few points light on it — Build's µ1-2-3 loses every
/// second of Z6 and Z7, all of its anaerobic and neuromuscular work, and
/// [`diverges`] charges it 1.03 points for that out of 16.9.
///
/// Nothing here decides what to do about it. It says which zones went.
#[must_use]
pub fn zones_lost(candidate: &ZoneProfile, from: &ZoneProfile) -> Vec<PowerZone> {
    let (a, b) = (candidate.shares(), from.shares());
    PowerZone::ALL
        .into_iter()
        .filter(|zone| {
            b.get(zone).copied().unwrap_or_default() > 0.0
                && a.get(zone).copied().unwrap_or_default() <= 0.0
        })
        .collect()
}

/// How far a run climbs: its hardest microcycle over its easiest.
///
/// **A ratio, so it does not care how many microcycles there are** — which is
/// what lets a three-microcycle selection be compared with the four-microcycle
/// programme it was taken from. Comparing the two arcs point by point would need
/// them resampled to a common length, and § II names that as a mistake.
///
/// **Hand it the working microcycles.** A run including its deload reports the
/// depth of the deload rather than the climb, and those are different questions:
/// Build spans 1.30× across its four working microcycles and 2.24× if its
/// deload is included.
///
/// `None` for an empty run, or one whose easiest microcycle scores nothing —
/// there is no ratio to take against zero.
#[must_use]
pub fn span(scores: &[f64]) -> Option<f64> {
    let floor = scores.iter().copied().reduce(f64::min)?;
    let peak = scores.iter().copied().reduce(f64::max)?;
    (floor > 0.0).then(|| peak / floor)
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
        .filter(|(_, run)| is_mesocycle(run))
        .map(|(at, _)| at..at + length)
        .collect()
}

/// Whether a run of microcycles is a mesocycle: it ends at its bottom level,
/// and something in it is above that level.
///
/// The second half is what stops a programme with no hard riding anywhere from
/// reporting a mesocycle at every offset — if everything is at the bottom then
/// nothing in it is working.
#[must_use]
pub fn is_mesocycle(run: &[f64]) -> bool {
    let floor = bottom_level(run);
    floor.last().copied().unwrap_or_default() && floor.iter().any(|at_bottom| !at_bottom)
}

/// The mesocycles a programme is made of, taken in order and without overlap.
///
/// **A provider supplies mesocycles, not programmes** (decision 0036). The
/// operator, 2026-09-05, on why an eight-microcycle programme kept answering a
/// four-microcycle request with a selection straddling both halves of itself:
///
/// > "that's a product of asking programmes of 2x 4 microcycle mesocycles to
/// > give you 1 4 microcycle mesocycle that represents the entire programme"
///
/// Greedy, and shortest-first: take the shortest prefix that is a mesocycle,
/// then start again after it. **The decomposition the operator states falls out
/// of this and did not have to be given** — Base and Peak split µ1-4 and µ5-8,
/// Build stays whole at µ1-5, under TSS and under every two-session selection of
/// it.
///
/// A tail that is no mesocycle is left out rather than forced into one, so the
/// ranges need not cover `scores`. A caller that needs them to can check.
#[must_use]
pub fn partition(scores: &[f64]) -> Vec<std::ops::Range<usize>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < scores.len() {
        let Some(end) =
            (at + 1..=scores.len()).find(|end| scores.get(at..*end).is_some_and(is_mesocycle))
        else {
            break;
        };
        found.push(at..end);
        at = end;
    }
    found
}

/// A published programme as a grid: what each microcycle and session trains.
///
/// **This is what a provider is asked** (decisions 0029, 0036). It holds zone
/// profiles and no identifiers — which class realises a cell is the adapter's
/// business (§ II.3) — so the same type serves any source that can say how long
/// was spent in which zone.
#[derive(Debug, Clone, Default)]
pub struct Programme {
    cells: BTreeMap<(u32, u32), ZoneProfile>,
}

/// What a mesocycle answers when asked for a smaller shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// The microcycles kept, in the programme's own order.
    pub microcycles: Vec<u32>,
    /// The sessions kept, likewise.
    pub sessions: Vec<u32>,
    /// How far its composition sits from the mesocycle taken whole. Lower wins.
    pub composition: f64,
    /// How far its working microcycles climb, hardest over easiest.
    pub span: Option<f64>,
}

/// Why a candidate was refused before it could be scored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// It does not end at its bottom level, so it is a progression rather than
    /// a mesocycle.
    NoDeload,
    /// It stops training something the mesocycle trains.
    StopsTraining(Vec<PowerZone>),
}

impl Programme {
    /// Build from `(microcycle, session)` cells. Absent cells are empty.
    pub fn new(cells: impl IntoIterator<Item = ((u32, u32), ZoneProfile)>) -> Self {
        Self {
            cells: cells.into_iter().collect(),
        }
    }

    fn ordered(&self, of: impl Fn(&(u32, u32)) -> u32) -> Vec<u32> {
        let mut seen: Vec<u32> = self.cells.keys().map(&of).collect();
        seen.sort_unstable();
        seen.dedup();
        seen
    }

    /// Every microcycle, in order.
    #[must_use]
    pub fn microcycles(&self) -> Vec<u32> {
        self.ordered(|(microcycle, _)| *microcycle)
    }

    /// Every session position, in order.
    #[must_use]
    pub fn sessions(&self) -> Vec<u32> {
        self.ordered(|(_, session)| *session)
    }

    /// The zone profile of a selection.
    #[must_use]
    pub fn profile(&self, microcycles: &[u32], sessions: &[u32]) -> ZoneProfile {
        let mut total = ZoneProfile::default();
        for ((microcycle, session), profile) in &self.cells {
            if microcycles.contains(microcycle) && sessions.contains(session) {
                for (zone, seconds) in profile.iter() {
                    *total.0.entry(zone).or_default() += seconds;
                }
            }
        }
        total
    }

    /// The mesocycles this programme is made of, each as its microcycles.
    ///
    /// Scored by [`tss`](ZoneProfile::tss) across every session, and split by
    /// [`partition`].
    #[must_use]
    pub fn mesocycles(&self) -> Vec<Vec<u32>> {
        let (microcycles, sessions) = (self.microcycles(), self.sessions());
        let scores: Vec<f64> = microcycles
            .iter()
            .map(|micro| self.profile(&[*micro], &sessions).tss())
            .collect();
        partition(&scores)
            .into_iter()
            .map(|run| {
                run.filter_map(|index| microcycles.get(index).copied())
                    .collect()
            })
            .collect()
    }

    /// What one mesocycle answers when asked for `microcycles` of `sessions`.
    ///
    /// Every candidate that survives both structural checks, best first. Empty
    /// where none does. **The answer is the first**, and there is no set: the
    /// operator settled on 2026-09-05 that where two score alike the choice is
    /// immaterial, so there is nothing to preserve by offering both (0036).
    #[must_use]
    pub fn answer(&self, mesocycle: &[u32], microcycles: usize, sessions: usize) -> Vec<Answer> {
        let mut admitted: Vec<Answer> = self
            .candidates(mesocycle, microcycles, sessions)
            .into_iter()
            .filter_map(
                |(chosen, taken)| match self.refuse(mesocycle, &chosen, &taken) {
                    Some(_) => None,
                    None => Some(self.score(mesocycle, chosen, taken)),
                },
            )
            .collect();
        admitted.sort_by(|a, b| a.composition.total_cmp(&b.composition));
        admitted
    }

    /// Every candidate, refused or not, with the reason where there is one.
    ///
    /// For showing the work: [`answer`](Self::answer) drops the refusals.
    #[must_use]
    pub fn considered(
        &self,
        mesocycle: &[u32],
        microcycles: usize,
        sessions: usize,
    ) -> Vec<(Answer, Option<Refused>)> {
        self.candidates(mesocycle, microcycles, sessions)
            .into_iter()
            .map(|(chosen, taken)| {
                let refused = self.refuse(mesocycle, &chosen, &taken);
                (self.score(mesocycle, chosen, taken), refused)
            })
            .collect()
    }

    fn candidates(
        &self,
        mesocycle: &[u32],
        microcycles: usize,
        sessions: usize,
    ) -> Vec<(Vec<u32>, Vec<u32>)> {
        let taken = subsets(&self.sessions(), sessions);
        subsets(mesocycle, microcycles)
            .into_iter()
            .flat_map(|chosen| {
                taken
                    .iter()
                    .map(move |sessions| (chosen.clone(), sessions.clone()))
            })
            .collect()
    }

    fn refuse(&self, mesocycle: &[u32], chosen: &[u32], taken: &[u32]) -> Option<Refused> {
        let sessions = self.sessions();
        let scores: Vec<f64> = chosen
            .iter()
            .map(|micro| self.profile(&[*micro], taken).tss())
            .collect();
        if !is_mesocycle(&scores) {
            return Some(Refused::NoDeload);
        }
        let lost = zones_lost(
            &self.profile(chosen, taken),
            &self.profile(mesocycle, &sessions),
        );
        (!lost.is_empty()).then_some(Refused::StopsTraining(lost))
    }

    fn score(&self, mesocycle: &[u32], microcycles: Vec<u32>, sessions: Vec<u32>) -> Answer {
        let reference = self.profile(mesocycle, &self.sessions());
        let composition = diverges(&self.profile(&microcycles, &sessions), &reference);
        let scores: Vec<f64> = microcycles
            .iter()
            .map(|micro| self.profile(&[*micro], &sessions).tss())
            .collect();
        let working = scores.split_last().map_or(&[][..], |(_, rest)| rest);
        Answer {
            span: span(working),
            microcycles,
            sessions,
            composition,
        }
    }
}

/// Every subset of `items` of the given size, keeping the order they are given.
///
/// **Subsets, never permutations** (decision 0035): a microcycle may be dropped
/// but the written order is kept. Recursive rather than index arithmetic,
/// because indexing can panic and panics are forbidden here.
fn subsets(items: &[u32], size: usize) -> Vec<Vec<u32>> {
    if size == 0 {
        return vec![Vec::new()];
    }
    let Some((first, rest)) = items.split_first() else {
        return Vec::new();
    };
    let mut out: Vec<Vec<u32>> = subsets(rest, size - 1)
        .into_iter()
        .map(|mut including| {
            including.insert(0, *first);
            including
        })
        .collect();
    out.extend(subsets(rest, size));
    out
}
