//! How far a measured trend may sit from the level it estimates (#616).
//!
//! **A trend here is the mean of a window of days, and its band is the 95%
//! confidence interval of that mean**: 1.96 × σ × √(inflation ÷ n), where n is
//! the days in the window with a figure. The operator, 2026-11-02: one method,
//! its numbers measured from each series' own record.
//!
//! - **σ is how far a day sits from its level**: the standard deviation of
//!   each day's figure about the mean of the window centred on it, the same
//!   length and with the same least count as the trend's, over the whole
//!   record.
//! - **The inflation is how far a day follows the days before it**:
//!   1 + 2 Σ (1 − k/n) ρₖ, where ρₖ is the correlation between those
//!   deviations k days apart, summed until the first that is not positive.
//!   Where a day does not follow the one before, it is 1 and the band is
//!   σ/√n; where it does, the window holds fewer independent days than it
//!   has days, and the band is wider by its square root.
//!
//! **Fewer days widen the band rather than hide it**: the least count that
//! starts the trend starts the band too, and n is whatever the window holds.
//!
//! It does not reach the instrument's systematic error, which no mean shrinks.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::composition::{CONFIDENCE, mean};

/// What the band is, as a page says it.
pub const BAND: &str = "the 95% confidence interval of the trend";

/// One series' noise, as its own record measures it.
#[derive(Debug, Clone, PartialEq)]
pub struct Noise {
    window: i64,
    sigma: f64,
    /// ρ₁, ρ₂, … up to the last positive one.
    correlation: Vec<f64>,
}

impl Noise {
    /// Measured from one figure a day, for a trend that is the mean of
    /// `window` days with at least `least` of them recorded. `None` with
    /// fewer than two days that have a centred window.
    pub fn of(days: &BTreeMap<Date, f64>, window: i64, least: usize) -> Option<Self> {
        let before = window / 2;
        let after = window - before - 1;
        let deviations: BTreeMap<Date, f64> = days
            .iter()
            .filter_map(|(&on, &value)| {
                let from = on.checked_sub(jiff::Span::new().days(before)).ok()?;
                let to = on.checked_add(jiff::Span::new().days(after)).ok()?;
                let around: Vec<f64> = days.range(from..=to).map(|(_, value)| *value).collect();
                (around.len() >= least).then_some(())?;
                Some((on, value - mean(&around)?))
            })
            .collect();
        let count = u32::try_from(deviations.len())
            .ok()
            .filter(|count| *count >= 2)?;
        let variance = deviations.values().map(|d| d.powi(2)).sum::<f64>() / f64::from(count - 1);
        let correlation = (1..window)
            .map_while(|lag| {
                let products: Vec<f64> = deviations
                    .iter()
                    .filter_map(|(on, deviation)| {
                        let later = on.checked_add(jiff::Span::new().days(lag)).ok()?;
                        Some(deviation * deviations.get(&later)?)
                    })
                    .collect();
                (products.len() >= 2)
                    .then(|| mean(&products).map(|product| product / variance))
                    .flatten()
                    .filter(|rho| *rho > 0.0)
            })
            .collect();
        Some(Self {
            window,
            sigma: variance.sqrt(),
            correlation,
        })
    }

    /// How far either side of the trend on `on` its band runs, from the days
    /// its window holds. `None` where the window holds none.
    pub fn margin(&self, days: &BTreeMap<Date, f64>, on: Date) -> Option<f64> {
        let from = on
            .checked_sub(jiff::Span::new().days(self.window - 1))
            .ok()?;
        let n = f64::from(
            u32::try_from(days.range(from..=on).count())
                .ok()
                .filter(|n| *n > 0)?,
        );
        let carried: f64 = self
            .correlation
            .iter()
            .zip(1..)
            .map(|(rho, lag)| (1.0 - f64::from(lag) / n).max(0.0) * rho)
            .sum();
        let inflation = carried.mul_add(2.0, 1.0);
        Some(CONFIDENCE * self.sigma * (inflation / n).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jiff::civil::{Date, date};

    use super::Noise;

    fn days(values: impl IntoIterator<Item = f64>) -> BTreeMap<Date, f64> {
        date(2026, 1, 1)
            .series(jiff::Span::new().days(1))
            .zip(values)
            .collect()
    }

    /// The same 0, 1, 0, 1 … pattern, so each day's deviation is ± 0.5.
    fn alternating(count: usize) -> BTreeMap<Date, f64> {
        days((0..count).map(|index| if index % 2 == 0 { 0.0 } else { 1.0 }))
    }

    #[test]
    fn a_day_that_does_not_follow_the_last_gives_sigma_over_root_n() {
        let days = alternating(200);
        let noise = Noise::of(&days, 7, 4).expect("enough days");
        assert!(noise.correlation.is_empty(), "{:?}", noise.correlation);
        let margin = noise
            .margin(&days, date(2026, 3, 1))
            .expect("a full window");
        let expected = 1.96 * noise.sigma / 7.0_f64.sqrt();
        assert!(
            (margin - expected).abs() < 1e-9,
            "{margin} against {expected}"
        );
    }

    #[test]
    fn fewer_days_widen_the_band() {
        let mut days = alternating(200);
        let full = Noise::of(&days, 7, 4).expect("enough days");
        let wide = full.margin(&days, date(2026, 3, 1)).expect("seven days");
        for on in [date(2026, 2, 24), date(2026, 2, 26), date(2026, 2, 28)] {
            days.remove(&on);
        }
        let narrow = full.margin(&days, date(2026, 3, 1)).expect("four days");
        assert!(narrow > wide, "{narrow} against {wide}");
        assert!((narrow / wide - (7.0_f64 / 4.0).sqrt()).abs() < 1e-9);
    }

    #[test]
    fn days_that_follow_each_other_widen_the_band() {
        // Fortnight-long swells, so neighbouring days sit on the same side of
        // the level: fewer independent days than days.
        let swelling = days((0..365).map(|index| if (index / 14) % 2 == 0 { 0.0 } else { 1.0 }));
        let noise = Noise::of(&swelling, 90, 39).expect("enough days");
        assert!(noise.correlation.len() > 5, "{:?}", noise.correlation);
        let margin = noise
            .margin(&swelling, date(2026, 9, 1))
            .expect("a full window");
        let independent = 1.96 * noise.sigma / 90.0_f64.sqrt();
        assert!(margin > 2.0 * independent, "{margin} against {independent}");
    }

    #[test]
    fn too_few_days_measure_nothing() {
        assert_eq!(Noise::of(&alternating(3), 7, 4), None);
    }
}
