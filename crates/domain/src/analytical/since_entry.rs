//! A macrocycle's latest test against its entry test (#517).
//!
//! **Has this macrocycle improved my fitness so far?** (operator,
//! 2026-11-11). For what a test gives, the answer is the latest test against
//! the first the macrocycle holds: *"the test is against the beginning of the
//! macrocycle."*
//!
//! **Read from what was performed**, not from what the plan dated: autumn
//! 2026's entry tests were planned for the week of 14 September and performed
//! on 23 and 25 September, after an illness, and a training mesocycle ends in
//! an exit test the plan's table does not name.
//!
//! **The noise is the test's own**, of two tests either side of the entry
//! test's figure ([`super::ftp_test::noise`], [`super::one_rep_max::noise`]).

use jiff::civil::Date;

/// The entry test, and the latest test after it where there is one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SinceEntry {
    pub entry: (Date, u32),
    /// `None` while the entry test is the only one.
    pub latest: Option<(Date, u32)>,
    /// How far either side of the entry test two tests land by chance, in the
    /// figure's unit.
    pub noise: f64,
}

impl SinceEntry {
    /// The latest figure less the entry test's.
    pub fn by(&self) -> Option<i64> {
        self.latest
            .map(|(_, latest)| i64::from(latest) - i64::from(self.entry.1))
    }
}

/// `tests`, oldest first, read over the macrocycle that `began` on a day.
/// `None` where it holds no test yet.
pub fn since_entry(
    tests: &[(Date, u32)],
    began: Date,
    today: Date,
    noise: impl Fn(u32) -> f64,
) -> Option<SinceEntry> {
    let within: Vec<(Date, u32)> = tests
        .iter()
        .copied()
        .filter(|(on, _)| *on >= began && *on <= today)
        .collect();
    let (entry, later) = within.split_first()?;
    Some(SinceEntry {
        entry: *entry,
        latest: later.last().copied(),
        noise: noise(entry.1),
    })
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::since_entry;
    use crate::analytical::ftp_test;

    const BEGAN: jiff::civil::Date = date(2026, 9, 14);
    const TODAY: jiff::civil::Date = date(2026, 11, 11);

    #[test]
    fn the_latest_test_is_read_against_the_first_the_macrocycle_holds() {
        let tests = [
            (date(2026, 7, 15), 171),
            (date(2026, 9, 23), 177),
            (date(2026, 10, 25), 182),
        ];
        let read = since_entry(&tests, BEGAN, TODAY, ftp_test::noise).expect("a test");
        assert_eq!(read.entry, (date(2026, 9, 23), 177));
        assert_eq!(read.by(), Some(5));
        assert!((read.noise - 14.2).abs() < 0.1, "{}", read.noise);
    }

    #[test]
    fn an_entry_test_alone_has_nothing_to_be_read_against() {
        let tests = [(date(2026, 9, 23), 177)];
        let read = since_entry(&tests, BEGAN, TODAY, ftp_test::noise).expect("a test");
        assert_eq!(read.latest, None);
        assert_eq!(read.by(), None);
    }

    #[test]
    fn a_macrocycle_with_no_test_yet_reads_nothing() {
        assert_eq!(
            since_entry(&[(date(2026, 7, 15), 171)], BEGAN, TODAY, ftp_test::noise),
            None
        );
    }
}
