//! The days of a canonical layer a change to the normalised one reaches (#610).
//!
//! **A canonical layer is rebuilt where something changed, not whole.** A
//! normalised writer records the days whose accounts it removed or wrote, and
//! the next derivation rebuilds the entries those days can have changed. How
//! far that is belongs to each layer's derivation: a gym visit can take in a
//! sheet dated two days away, and a ride or a weighing takes in nothing beyond
//! its own day.

use std::collections::BTreeSet;

use jiff::civil::Date;

/// The days a change reaches, and the days it takes to rebuild them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reach {
    changed: BTreeSet<Date>,
    rebuilt: BTreeSet<Date>,
    read: BTreeSet<Date>,
}

impl Reach {
    /// What changing the accounts on these days reaches, in a layer whose
    /// accounts change entries up to `either_side` days from their own.
    ///
    /// The entries on each day rebuilt are decided by the accounts within the
    /// same distance of it, so that is what is read.
    pub fn of(changed: impl IntoIterator<Item = Date>, either_side: i64) -> Self {
        let changed: BTreeSet<Date> = changed.into_iter().collect();
        let rebuilt = around(&changed, either_side);
        let read = around(&rebuilt, either_side);
        Self {
            changed,
            rebuilt,
            read,
        }
    }

    /// The days whose accounts changed.
    pub const fn changed(&self) -> &BTreeSet<Date> {
        &self.changed
    }

    /// The days whose entries may have changed with them.
    pub const fn rebuilt(&self) -> &BTreeSet<Date> {
        &self.rebuilt
    }

    /// The days whose accounts decide the entries on the days rebuilt.
    pub const fn read(&self) -> &BTreeSet<Date> {
        &self.read
    }

    /// Whether nothing changed, so nothing is rebuilt.
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }
}

/// Every day within `days` of one of these.
fn around(these: &BTreeSet<Date>, days: i64) -> BTreeSet<Date> {
    these
        .iter()
        .flat_map(|day| {
            (-days..=days)
                .filter_map(move |offset| day.checked_add(jiff::Span::new().days(offset)).ok())
        })
        .collect()
}
