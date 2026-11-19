//! Whether a reading is getting closer to expected or further from it (#874).
//!
//! The operator, 2026-11-19: *"am I moving towards or away from that?"* Body
//! weight and composition against the plan's aims, and the response readings
//! against the training done.
//!
//! **Joinpoint regression on each day's reading less expected**: the gap is
//! split where its direction turned, the stretches meeting at each turn, and
//! each stretch is closer, further, or no change that can be told from noise.
//! The stretch since the last turn is the answer now. A fixed window was
//! mocked and rejected: a straight line through a window averages away a
//! turn.
//!
//! **Judged against the line the page draws**, split wherever that line
//! restarts (operator, 2026-11-19), so that a choice of where the line starts
//! (#875) changes what is judged with it.
//!
//! **Not ruled by the operator**: a stretch is at least [`LEAST_DAYS`], at
//! most [`MOST_TURNS`] turns are looked for, and a turn is kept only where the
//! Bayesian information criterion prefers it.

use std::collections::BTreeMap;

use jiff::civil::Date;

/// The shortest stretch, in days.
const LEAST_DAYS: i32 = 7;
/// The most turns looked for along one line.
const MOST_TURNS: usize = 3;
/// How many standard errors a stretch's slope must clear to be told from
/// noise: its 95% band.
const Z: f64 = 1.96;

/// Which way the gap between a reading and its expectation went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moving {
    Closer,
    Further,
    /// Neither, as far as can be told from noise.
    NoChange,
}

/// Days over which the gap went one way, the last day of one the first of
/// the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    pub from: Date,
    pub to: Date,
    pub moving: Moving,
}

/// Each stretch of each line, oldest first: `readings` one figure a day, and
/// `lines` the expectation, one entry per line drawn, each from where it
/// starts.
///
/// A line with fewer than [`LEAST_DAYS`] between its first and last reading
/// is not judged.
pub fn stretches(readings: &BTreeMap<Date, f64>, lines: &[Vec<(Date, f64)>]) -> Vec<Stretch> {
    lines
        .iter()
        .flat_map(|line| judged(readings, line).unwrap_or_default())
        .collect()
}

/// One line's stretches.
fn judged(readings: &BTreeMap<Date, f64>, line: &[(Date, f64)]) -> Option<Vec<Stretch>> {
    let (start, _) = *line.first()?;
    let points: Vec<(i32, f64)> = line
        .iter()
        .filter_map(|(on, expected)| {
            let read = readings.get(on)?;
            Some((days(start, *on)?, read - expected))
        })
        .collect();
    let (first, _) = *points.first()?;
    let (last, _) = *points.last()?;
    if last - first < LEAST_DAYS - 1 {
        return None;
    }
    let sums = Sums::of(&points);
    let candidates: Vec<i32> = (first + LEAST_DAYS..=last - LEAST_DAYS).collect();
    let mut best: Option<(f64, Fit)> = None;
    for turns in sets(&candidates, MOST_TURNS) {
        let Some(fit) = Fit::of(&sums, &turns) else {
            continue;
        };
        let count = sums.count;
        // A slope and a level, and a day and a change of slope for each turn.
        let parameters = turns.iter().fold(2.0, |sum, _| sum + 2.0);
        let criterion = count.mul_add(
            (fit.residual.max(f64::MIN_POSITIVE) / count).ln(),
            parameters * count.ln(),
        );
        if best.as_ref().is_none_or(|(least, _)| criterion < *least) {
            best = Some((criterion, fit));
        }
    }
    let (_, fit) = best?;
    let bounds: Vec<i32> = std::iter::once(first)
        .chain(fit.turns.iter().copied())
        .chain(std::iter::once(last))
        .collect();
    bounds
        .windows(2)
        .map(|pair| {
            let (&from, &to) = (pair.first()?, pair.last()?);
            let (slope, error) = fit.slope(from)?;
            let middle = fit.level(f64::midpoint(f64::from(from), f64::from(to)));
            let closing = if middle > 0.0 { -slope } else { slope };
            let moving = if closing > Z * error {
                Moving::Closer
            } else if closing < -Z * error {
                Moving::Further
            } else {
                Moving::NoChange
            };
            Some(Stretch {
                from: on(start, from)?,
                to: on(start, to)?,
                moving,
            })
        })
        .collect()
}

fn days(from: Date, to: Date) -> Option<i32> {
    from.until(to).ok().map(|span| span.get_days())
}

fn on(start: Date, day: i32) -> Option<Date> {
    start.checked_add(jiff::Span::new().days(day)).ok()
}

/// Every set of up to `most` turns from `candidates`, each at least
/// [`LEAST_DAYS`] after the one before, the empty set first.
fn sets(candidates: &[i32], most: usize) -> Vec<Vec<i32>> {
    fn grow(candidates: &[i32], most: usize, from: &[i32], into: &mut Vec<Vec<i32>>) {
        into.push(from.to_vec());
        if from.len() == most {
            return;
        }
        let after = from.last().map(|last| last + LEAST_DAYS);
        for (index, at) in candidates.iter().enumerate() {
            if after.is_some_and(|after| *at < after) {
                continue;
            }
            let mut next = from.to_vec();
            next.push(*at);
            grow(
                candidates.get(index + 1..).unwrap_or_default(),
                most,
                &next,
                into,
            );
        }
    }
    let mut into = Vec::new();
    grow(candidates, most, &[], &mut into);
    into
}

/// Sums over the points from each one on, so that a fit is a handful of
/// sums rather than a pass over every day.
struct Sums {
    /// The points' days, ascending.
    days: Vec<f64>,
    /// From each point on: its count, Σx, Σx², Σy and Σxy; one more entry,
    /// all zero, past the last.
    from: Vec<[f64; 5]>,
    count: f64,
    squares: f64,
}

impl Sums {
    fn of(points: &[(i32, f64)]) -> Self {
        let mut from = vec![[0.0; 5]];
        for (day, y) in points.iter().rev() {
            let x = f64::from(*day);
            let [count, xs, squares, ys, products] = from.last().copied().unwrap_or_default();
            from.push([
                count + 1.0,
                xs + x,
                x.mul_add(x, squares),
                ys + y,
                x.mul_add(*y, products),
            ]);
        }
        from.reverse();
        let [count, ..] = from.first().copied().unwrap_or_default();
        Self {
            days: points.iter().map(|(x, _)| f64::from(*x)).collect(),
            from,
            count,
            squares: points.iter().map(|(_, y)| y * y).sum(),
        }
    }

    /// The sums over the points after day `after`.
    fn after(&self, after: f64) -> [f64; 5] {
        let index = self.days.partition_point(|x| *x <= after);
        self.from.get(index).copied().unwrap_or_default()
    }
}

/// One column of the fit: a constant, or a hinge rising one a day from a day.
#[derive(Clone, Copy)]
enum Column {
    Constant,
    /// Zero up to its day, then the days since. From day zero, the day itself.
    Hinge(f64),
}

impl Column {
    /// The sum of this column times `other` over every point.
    fn with(self, other: Self, sums: &Sums) -> f64 {
        match (self, other) {
            (Self::Constant, Self::Constant) => sums.count,
            (Self::Constant, Self::Hinge(at)) | (Self::Hinge(at), Self::Constant) => {
                let [count, xs, ..] = sums.after(at);
                at.mul_add(-count, xs)
            }
            (Self::Hinge(one), Self::Hinge(other)) => {
                let [count, xs, squares, ..] = sums.after(one.max(other));
                (one * other).mul_add(count, (one + other).mul_add(-xs, squares))
            }
        }
    }

    /// The sum of this column times the gap over every point.
    fn with_gap(self, sums: &Sums) -> f64 {
        match self {
            Self::Constant => {
                let [_, _, _, ys, _] = sums.after(f64::NEG_INFINITY);
                ys
            }
            Self::Hinge(at) => {
                let [_, _, _, ys, products] = sums.after(at);
                at.mul_add(-ys, products)
            }
        }
    }

    fn at(self, day: f64) -> f64 {
        match self {
            Self::Constant => 1.0,
            Self::Hinge(at) => (day - at).max(0.0),
        }
    }
}

/// Least squares through the gap, continuous, its slope changing at each
/// turn.
struct Fit {
    turns: Vec<i32>,
    columns: Vec<Column>,
    coefficients: Vec<f64>,
    /// The coefficients' covariance, less the residual variance.
    inverse: Vec<Vec<f64>>,
    residual: f64,
    /// The residual variance.
    variance: f64,
}

impl Fit {
    fn of(sums: &Sums, turns: &[i32]) -> Option<Self> {
        let columns: Vec<Column> = [Column::Constant, Column::Hinge(0.0)]
            .into_iter()
            .chain(turns.iter().map(|at| Column::Hinge(f64::from(*at))))
            .collect();
        let free = columns.iter().fold(sums.count, |left, _| left - 1.0);
        if free < 1.0 {
            return None;
        }
        let crossed: Vec<Vec<f64>> = columns
            .iter()
            .map(|one| columns.iter().map(|other| one.with(*other, sums)).collect())
            .collect();
        let inverse = inverted(crossed)?;
        let against: Vec<f64> = columns.iter().map(|one| one.with_gap(sums)).collect();
        let coefficients: Vec<f64> = inverse.iter().map(|row| dot(row, &against)).collect();
        let residual = (sums.squares - dot(&coefficients, &against)).max(0.0);
        Some(Self {
            turns: turns.to_vec(),
            columns,
            coefficients,
            inverse,
            residual,
            variance: residual / free,
        })
    }

    /// The fitted gap on a day.
    fn level(&self, day: f64) -> f64 {
        self.columns
            .iter()
            .zip(&self.coefficients)
            .map(|(column, coefficient)| coefficient * column.at(day))
            .sum()
    }

    /// The slope of the stretch that starts on day `from`, and its standard
    /// error: the day's coefficient and every turn's up to `from`.
    fn slope(&self, from: i32) -> Option<(f64, f64)> {
        let from = f64::from(from);
        let weights: Vec<f64> = self
            .columns
            .iter()
            .map(|column| match column {
                Column::Constant => 0.0,
                Column::Hinge(at) => {
                    if *at <= from {
                        1.0
                    } else {
                        0.0
                    }
                }
            })
            .collect();
        let slope = dot(&weights, &self.coefficients);
        let spread: f64 = self
            .inverse
            .iter()
            .zip(&weights)
            .map(|(row, weight)| weight * dot(row, &weights))
            .sum();
        (spread >= 0.0).then(|| (slope, (spread * self.variance).sqrt()))
    }
}

fn dot(one: &[f64], other: &[f64]) -> f64 {
    one.iter().zip(other).map(|(a, b)| a * b).sum()
}

/// The inverse by Gauss–Jordan elimination; `None` where it is singular.
fn inverted(mut matrix: Vec<Vec<f64>>) -> Option<Vec<Vec<f64>>> {
    let size = matrix.len();
    let mut inverse: Vec<Vec<f64>> = (0..size)
        .map(|row| {
            (0..size)
                .map(|column| f64::from(u8::from(row == column)))
                .collect()
        })
        .collect();
    for column in 0..size {
        let pivot = (column..size).max_by(|one, other| {
            let size = |row: &usize| {
                matrix
                    .get(*row)
                    .and_then(|row| row.get(column))
                    .map_or(0.0, |value| value.abs())
            };
            size(one).total_cmp(&size(other))
        })?;
        matrix.swap(column, pivot);
        inverse.swap(column, pivot);
        let scale = *matrix.get(column)?.get(column)?;
        if scale.abs() < 1e-9 {
            return None;
        }
        for row in [matrix.get_mut(column)?, inverse.get_mut(column)?] {
            for value in row.iter_mut() {
                *value /= scale;
            }
        }
        let (by_matrix, by_inverse) = (matrix.get(column)?.clone(), inverse.get(column)?.clone());
        for row in (0..size).filter(|row| *row != column) {
            let factor = *matrix.get(row)?.get(column)?;
            for (target, by) in [
                (matrix.get_mut(row)?, &by_matrix),
                (inverse.get_mut(row)?, &by_inverse),
            ] {
                target
                    .iter_mut()
                    .zip(by)
                    .for_each(|(value, by)| *value = factor.mul_add(-by, *value));
            }
        }
    }
    Some(inverse)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jiff::civil::{Date, date};

    use super::{Moving, Stretch, stretches};

    const START: Date = date(2026, 9, 28);

    fn day(index: i32) -> Date {
        START
            .checked_add(jiff::Span::new().days(index))
            .expect("a day")
    }

    /// A flat expectation of zero over `length` days, and a reading of
    /// `gap` on each, with a beat of day-to-day noise.
    fn read(length: i32, gap: impl Fn(i32) -> f64) -> (BTreeMap<Date, f64>, Vec<(Date, f64)>) {
        let readings = (0..length)
            .map(|index| {
                let wobble = if index % 2 == 0 { 0.1 } else { -0.1 };
                (day(index), gap(index) + wobble)
            })
            .collect();
        let line = (0..length).map(|index| (day(index), 0.0)).collect();
        (readings, line)
    }

    /// **A turn is found where the gap turned**: away from expected for
    /// three weeks, then back towards it.
    #[test]
    fn a_gap_that_turns_is_two_stretches() {
        let (readings, line) = read(42, |index| {
            if index < 21 {
                0.1 * f64::from(index)
            } else {
                0.1f64.mul_add(-f64::from(index - 21), 2.1)
            }
        });
        let found = stretches(&readings, &[line]);
        assert_eq!(
            found,
            vec![
                Stretch {
                    from: day(0),
                    to: day(21),
                    moving: Moving::Further,
                },
                Stretch {
                    from: day(21),
                    to: day(41),
                    moving: Moving::Closer,
                },
            ]
        );
    }

    /// **Noise alone is no change**, and no turn.
    #[test]
    fn noise_alone_is_no_change() {
        let (readings, line) = read(28, |_| 0.5);
        let found = stretches(&readings, &[line]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found.first().map(|one| one.moving), Some(Moving::NoChange));
    }

    /// **Below expected and rising is closer**: which way is closer depends
    /// on which side of the line the reading is.
    #[test]
    fn closer_is_towards_the_line_from_either_side() {
        let (readings, line) = read(21, |index| 0.1f64.mul_add(f64::from(index), -3.0));
        let found = stretches(&readings, &[line]);
        assert_eq!(found.first().map(|one| one.moving), Some(Moving::Closer));
    }

    /// **Each line is judged on its own**, so a stretch never runs across
    /// a restart, and a line too short to judge has none.
    #[test]
    fn each_line_is_judged_on_its_own() {
        let (readings, line) = read(30, |index| 0.1 * f64::from(index));
        let (early, late) = line.split_at(5);
        let found = stretches(&readings, &[early.to_vec(), late.to_vec()]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found.first().map(|one| one.from), Some(day(5)));
    }
}
