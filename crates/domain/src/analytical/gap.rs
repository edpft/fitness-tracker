//! Whether a response reading is tracking its expected result, has diverged
//! from it, is diverging from it or is converging on it (#874, #888).
//!
//! The operator, 2026-11-19: *"am I moving towards or away from that?"*, and
//! of the response readings, *"we have the right model for response metrics
//! with tracking, diverging, or converging"*; 2026-11-20: *"we're missing a
//! diverged state where the outcome isn't tracking but also isn't diverging
//! or converging"*. **Neither side of the line is better**: the expectation
//! is a hypothesis, not a goal. Body weight and composition, which are
//! progress towards a goal, are #881.
//!
//! **Judged on the line the page draws**: the mean of the week to each day
//! less where the expected line puts it, split wherever that line restarts
//! (operator, 2026-11-19), so that a choice of where the line starts (#875)
//! changes what is judged with it. Nightly readings were judged until #888,
//! and their noise hid every turn the weekly line shows.
//!
//! **Segmented regression on that gap**: the days are split into stretches,
//! each its own straight line, by the least squares over every way of
//! splitting them (dynamic programming, so no limit on the turns), and the
//! number of stretches is the one the Bayesian information criterion
//! prefers. The criterion finds where the line bends; whether a bend is told
//! from noise is each stretch's slope, and a week's mean shares its days with
//! the six before it, so a slope's noise counts a week of readings as one.
//! Counting them as one in the criterion too found no turn at all in
//! overnight HRV's weekly line. Then each stretch:
//!
//! - **tracking** where the line is within the band drawn about the reading
//!   at both its ends: a movement inside the band is noise;
//! - **diverging** where it leaves the band, and **converging** where it
//!   comes back inside it;
//! - off the line at both ends, **converging** where it ends nearer the line
//!   than it began by more than its slope's noise, **diverging** where it ends
//!   further from it, **diverged** where it holds on one side, and
//!   **tracking** where it crossed the line and ends about as far from it as
//!   it began.
//!
//! So converging ends on the line or nearer it, and diverging leaves it
//! (operator, 2026-11-20: stress went tracking to converging without
//! diverging, when only the gap's movement inside its band was judged).
//!
//! Neighbouring stretches in the same state are one stretch. The stretch since
//! the last turn is the answer now. A fixed window was mocked and rejected: a
//! straight line through a window averages away a turn.
//!
//! **Not ruled by the operator**: a stretch is at least [`LEAST_DAYS`], and
//! its slope must clear its 95% band to be told from noise.

use jiff::civil::Date;

/// The shortest stretch, in days.
const LEAST_DAYS: i32 = 7;
/// How many standard errors a stretch's slope must clear to be told from
/// noise: its 95% band.
const Z: f64 = 1.96;

/// How a reading moved against its expectation over a stretch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moving {
    /// On the line within noise, the gap neither growing nor shrinking.
    Tracking,
    /// Off the line by more than noise, the gap neither growing nor
    /// shrinking.
    Diverged,
    /// The gap grew.
    Diverging,
    /// The gap shrank.
    Converging,
}

/// One day's reading against where the expected line puts it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gap {
    pub on: Date,
    /// The reading less expected.
    pub by: f64,
    /// How far either side of the reading its band runs.
    pub margin: f64,
}

/// Days over which the gap went one way, the last day of one the first of
/// the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    pub from: Date,
    pub to: Date,
    pub moving: Moving,
}

/// Each stretch of each line, oldest first: `lines` one gap a day, one entry
/// per line drawn, each from where it starts, and each reading the mean of
/// `spanning` days to its own.
///
/// A line with fewer than [`LEAST_DAYS`] between its first and last reading
/// is not judged.
pub fn stretches(lines: &[Vec<Gap>], spanning: i32) -> Vec<Stretch> {
    lines
        .iter()
        .flat_map(|line| merged(judged(line, spanning).unwrap_or_default()))
        .collect()
}

/// Neighbouring stretches in the same state as one.
fn merged(stretches: Vec<Stretch>) -> Vec<Stretch> {
    let mut into: Vec<Stretch> = Vec::with_capacity(stretches.len());
    for stretch in stretches {
        match into.last_mut() {
            Some(last) if last.moving == stretch.moving => last.to = stretch.to,
            _ => into.push(stretch),
        }
    }
    into
}

/// One line's stretches, before merging.
fn judged(line: &[Gap], spanning: i32) -> Option<Vec<Stretch>> {
    let start = line.first()?.on;
    let points: Vec<(f64, &Gap)> = line
        .iter()
        .map(|gap| Some((f64::from(days(start, gap.on)?), gap)))
        .collect::<Option<_>>()?;
    let (first, _) = *points.first()?;
    let (last, _) = *points.last()?;
    if last - first < f64::from(LEAST_DAYS - 1) {
        return None;
    }
    let sums = Sums::of(&points);
    let count = points.len();
    let shortest = f64::from(LEAST_DAYS - 1);
    // A stretch from point `from` to point `to`, both included, where it is
    // long enough.
    let fits = |from: usize, to: usize| {
        let (&(begin, _), &(end, _)) = (points.get(from)?, points.get(to)?);
        (end - begin >= shortest).then(|| sums.between(from, to).residual())
    };

    // best[k][to]: the least residual splitting points 0..=to into k + 1
    // stretches, and where the last of them starts.
    let mut best: Vec<Vec<Option<(f64, usize)>>> = Vec::new();
    best.push((0..count).map(|to| fits(0, to).map(|r| (r, 0))).collect());
    loop {
        let before = best.last()?;
        let next: Vec<Option<(f64, usize)>> = (0..count)
            .map(|to| {
                (1..=to)
                    .filter_map(|from| {
                        let (earlier, _) = (*before.get(from - 1)?)?;
                        Some((earlier + fits(from, to)?, from))
                    })
                    .min_by(|(one, _), (other, _)| one.total_cmp(other))
            })
            .collect();
        if next.last()?.is_none() {
            break;
        }
        best.push(next);
    }

    // A level and a slope for each stretch, and a day for each turn.
    let criterion = |stretches: usize, residual: f64| {
        let parameters = f64::from(u32::try_from(3 * stretches - 1).ok()?);
        Some(sums.count.mul_add(
            (residual.max(f64::MIN_POSITIVE) / sums.count).ln(),
            parameters * sums.count.ln(),
        ))
    };
    let (chosen, residual) = best
        .iter()
        .enumerate()
        .filter_map(|(k, row)| {
            let (residual, _) = (*row.last()?)?;
            Some((k, residual, criterion(k + 1, residual)?))
        })
        .min_by(|(_, _, one), (_, _, other)| one.total_cmp(other))
        .map(|(k, residual, _)| (k, residual))?;

    let mut bounds = vec![count];
    let mut to = count - 1;
    for row in best.iter().take(chosen + 1).rev() {
        let (_, from) = (*row.get(to)?)?;
        bounds.push(from);
        to = from.saturating_sub(1);
    }
    bounds.reverse();

    let stretches = chosen + 1;
    let free = sums.count - f64::from(u32::try_from(2 * stretches).ok()?);
    let variance = if free > 0.0 { residual / free } else { 0.0 } * f64::from(spanning.max(1));
    let mut met = start;
    bounds
        .windows(2)
        .map(|pair| {
            let (&from, &until) = (pair.first()?, pair.last()?);
            let to = until - 1;
            let segment = sums.between(from, to);
            let (&(begin, open), &(end, close)) = (points.get(from)?, points.get(to)?);
            let (level, slope) = segment.line()?;
            let (opened, closed) = (slope.mul_add(begin, level), slope.mul_add(end, level));
            // How much further from the line the stretch ends than it began,
            // against the noise of its slope over its days: a reading that
            // crosses the line ends about as far from it as it began.
            let grew = closed.abs() - opened.abs();
            let noise = Z * (variance / segment.spread()).sqrt() * (end - begin);
            let (left, back) = (opened.abs() <= open.margin, closed.abs() <= close.margin);
            let moving = match (left, back) {
                (true, false) => Moving::Diverging,
                (false, true) => Moving::Converging,
                (false, false) if grew < -noise => Moving::Converging,
                (false, false) if grew > noise => Moving::Diverging,
                (false, false) if opened.signum() == closed.signum() => Moving::Diverged,
                // Inside the band at both ends, or across the line.
                _ => Moving::Tracking,
            };
            let stretch = Stretch {
                from: if from == 0 { open.on } else { met },
                to: close.on,
                moving,
            };
            met = close.on;
            Some(stretch)
        })
        .collect()
}

fn days(from: Date, to: Date) -> Option<i32> {
    from.until(to).ok().map(|span| span.get_days())
}

/// Running sums of the points, so that any stretch's fit is a difference of
/// two entries rather than a pass over its days.
struct Sums {
    /// Up to each point: its count, Σx, Σx², Σy, Σxy and Σy²; one more
    /// entry, all zero, before the first.
    upto: Vec<Moments>,
    count: f64,
}

/// The sums one stretch's straight line is fitted from.
#[derive(Clone, Copy, Default)]
struct Moments {
    count: f64,
    xs: f64,
    xxs: f64,
    ys: f64,
    xys: f64,
    yys: f64,
}

impl Sums {
    fn of(points: &[(f64, &Gap)]) -> Self {
        let mut upto = vec![Moments::default()];
        let mut running = Moments::default();
        for (x, gap) in points {
            let y = gap.by;
            running = Moments {
                count: running.count + 1.0,
                xs: running.xs + x,
                xxs: x.mul_add(*x, running.xxs),
                ys: running.ys + y,
                xys: x.mul_add(y, running.xys),
                yys: y.mul_add(y, running.yys),
            };
            upto.push(running);
        }
        Self {
            upto,
            count: running.count,
        }
    }

    /// The sums over points `from` to `to`, both included.
    fn between(&self, from: usize, to: usize) -> Moments {
        let (Some(after), Some(before)) = (self.upto.get(to + 1), self.upto.get(from)) else {
            return Moments::default();
        };
        Moments {
            count: after.count - before.count,
            xs: after.xs - before.xs,
            xxs: after.xxs - before.xxs,
            ys: after.ys - before.ys,
            xys: after.xys - before.xys,
            yys: after.yys - before.yys,
        }
    }
}

impl Moments {
    /// Σ(x − x̄)².
    fn spread(&self) -> f64 {
        self.xs.mul_add(-self.xs / self.count, self.xxs)
    }

    /// The least-squares line: its level at day zero and its slope.
    fn line(&self) -> Option<(f64, f64)> {
        let spread = self.spread();
        if self.count < 2.0 || spread <= 0.0 {
            return None;
        }
        let slope = self.xs.mul_add(-self.ys / self.count, self.xys) / spread;
        Some((slope.mul_add(-self.xs, self.ys) / self.count, slope))
    }

    /// The squares left about the least-squares line.
    fn residual(&self) -> f64 {
        let spread = self.spread();
        let across = self.ys.mul_add(-self.ys / self.count, self.yys);
        let explained = self.xs.mul_add(-self.ys / self.count, self.xys);
        if spread > 0.0 {
            (across - explained * explained / spread).max(0.0)
        } else {
            across.max(0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Gap, Moving, Stretch, merged, stretches};

    const START: Date = date(2026, 9, 28);

    fn day(index: i32) -> Date {
        START
            .checked_add(jiff::Span::new().days(index))
            .expect("a day")
    }

    /// `gap` on each of `length` days, with a beat of day-to-day noise, and a
    /// band of `margin` either side.
    fn read(length: i32, margin: f64, gap: impl Fn(i32) -> f64) -> Vec<Gap> {
        (0..length)
            .map(|index| {
                let wobble = if index % 2 == 0 { 0.1 } else { -0.1 };
                Gap {
                    on: day(index),
                    by: gap(index) + wobble,
                    margin,
                }
            })
            .collect()
    }

    fn states(found: &[Stretch]) -> Vec<Moving> {
        found.iter().map(|one| one.moving).collect()
    }

    /// **A turn is found where the gap turned**: away from expected for
    /// three weeks, then back towards it, the two stretches meeting there.
    #[test]
    fn a_gap_that_turns_is_two_stretches() {
        let line = read(42, 0.3, |index| {
            if index < 21 {
                0.1f64.mul_add(f64::from(index), 0.5)
            } else {
                0.1f64.mul_add(-f64::from(index - 21), 2.6)
            }
        });
        let found = stretches(&[line], 1);
        assert_eq!(
            states(&found),
            vec![Moving::Diverging, Moving::Converging],
            "{found:?}"
        );
        let turn = found.first().map(|one| one.to).expect("a turn");
        assert!((day(20)..=day(22)).contains(&turn), "{found:?}");
        assert_eq!(found.get(1).map(|one| one.from), Some(turn));
        assert_eq!(found.last().map(|one| one.to), Some(day(41)));
    }

    /// **A gap that holds within the band is tracking**, and noise alone
    /// finds no turn.
    #[test]
    fn holding_on_the_line_is_tracking() {
        let found = stretches(&[read(28, 1.0, |_| 0.5)], 1);
        assert_eq!(states(&found), vec![Moving::Tracking], "{found:?}");
    }

    /// **A movement inside the band is tracking**: stress fell to 4 below
    /// its line and came back, all within its band of 5.5, and read as
    /// converging after tracking with no diverging between.
    #[test]
    fn moving_inside_the_band_is_tracking() {
        let line = read(42, 5.0, |index| {
            if index < 21 {
                -0.2 * f64::from(index)
            } else {
                0.2f64.mul_add(f64::from(index - 21), -4.2)
            }
        });
        let found = stretches(&[line], 1);
        assert_eq!(states(&found), vec![Moving::Tracking], "{found:?}");
    }

    /// **A gap that holds beyond the band has diverged**: off the line and
    /// neither moving away from it nor back.
    #[test]
    fn holding_off_the_line_has_diverged() {
        let found = stretches(&[read(28, 1.0, |_| -3.0)], 1);
        assert_eq!(states(&found), vec![Moving::Diverged], "{found:?}");
    }

    /// **Below expected and rising is converging**: which way converges
    /// depends on which side of the line the reading is.
    #[test]
    fn converging_is_towards_the_line_from_either_side() {
        let found = stretches(
            &[read(21, 0.3, |index| {
                0.1f64.mul_add(f64::from(index), -3.0)
            })],
            1,
        );
        assert_eq!(states(&found), vec![Moving::Converging], "{found:?}");
    }

    /// **A reading that crosses the line and ends as far from it is
    /// tracking it**, however steep: stress wavering about its expected
    /// result read as converging on it.
    #[test]
    fn crossing_the_line_is_tracking_it() {
        let found = stretches(
            &[read(21, 0.3, |index| {
                0.1f64.mul_add(f64::from(index), -1.0)
            })],
            1,
        );
        assert_eq!(states(&found), vec![Moving::Tracking], "{found:?}");
    }

    /// **A reading that overshoots the line has still converged**: resting
    /// heart rate coming back after 4 November ends just past its line.
    #[test]
    fn overshooting_the_line_is_converging() {
        let found = stretches(
            &[read(21, 0.3, |index| {
                0.2f64.mul_add(-f64::from(index), 3.0)
            })],
            1,
        );
        assert_eq!(states(&found), vec![Moving::Converging], "{found:?}");
    }

    /// **As many turns as the gap took**: the operator's reading of
    /// overnight HRV is five stretches, one more than three turns allowed.
    #[test]
    fn every_turn_is_found() {
        let line = read(75, 0.5, |index| {
            let at = f64::from(index);
            match index {
                0..15 => 0.0,
                15..30 => 0.4 * (at - 15.0),
                30..45 => 0.4f64.mul_add(-(at - 30.0), 6.0),
                45..60 => 0.4 * (at - 45.0),
                _ => 0.4f64.mul_add(-(at - 60.0), 6.0),
            }
        });
        let found = stretches(&[line], 1);
        assert_eq!(
            states(&found),
            vec![
                Moving::Tracking,
                Moving::Diverging,
                Moving::Converging,
                Moving::Diverging,
                Moving::Converging,
            ],
            "{found:?}"
        );
    }

    /// **Neighbours in the same state are one stretch**: stress was two
    /// stretches of tracking meeting on 2 November.
    #[test]
    fn neighbours_in_one_state_are_one_stretch() {
        let stretch = |from, to, moving| Stretch {
            from: day(from),
            to: day(to),
            moving,
        };
        let found = merged(vec![
            stretch(0, 10, Moving::Tracking),
            stretch(10, 20, Moving::Tracking),
            stretch(20, 30, Moving::Diverging),
        ]);
        assert_eq!(
            found,
            vec![
                stretch(0, 20, Moving::Tracking),
                stretch(20, 30, Moving::Diverging),
            ]
        );
    }

    /// **Each line is judged on its own**, so a stretch never runs across
    /// a restart, and a line too short to judge has none.
    #[test]
    fn each_line_is_judged_on_its_own() {
        let line = read(30, 0.3, |index| 0.1f64.mul_add(f64::from(index), 0.5));
        let (early, late) = line.split_at(5);
        let found = stretches(&[early.to_vec(), late.to_vec()], 1);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found.first().map(|one| one.from), Some(day(5)));
    }
}
