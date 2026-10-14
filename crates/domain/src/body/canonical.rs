//! The canonical weigh-in: one per real weighing, whatever recorded it (#409).
//!
//! **One weighing a day, unless a clock says otherwise.** No source dated to a
//! day records two weigh-ins on one: the spreadsheets hold a day once per file
//! and MyFitnessPal once in all. So every account of a day is one weighing,
//! except where an instrument stamped the instant it was read: a Body Scan
//! weigh-in is its own weighing, and an account dated to that day joins it
//! where it is the day's only one. A day with two stamped weighings and an
//! account that names neither is left out, because nothing says which it was.
//! No day in the record has both kinds.
//!
//! **What the merge settles by is § 10's**, as amended in constitution 6.0.0:
//!
//! 1. An account stating when it was entered beats one that does not. A Body
//!    Scan states when it measured, and MyFitnessPal when the weight was typed.
//!    Where several state it and they agree, their value stands.
//! 2. Every spreadsheet copy of a day is one entry, typed once and carried
//!    forward (the operator, 2026-10-13). Where copies differ, the copy whose
//!    document was last saved earliest stands.
//! 3. Otherwise the weighing is left out, and named.
//!
//! **§ 10's majority is not here.** It counts independent accounts with no
//! stated time, and every such account in this build is a spreadsheet copy, so
//! there is never more than one. A source that adds a second kind adds the rule.

use std::fmt;

use jiff::civil::Date;

use crate::{
    canonical::{Attributed, Occurred},
    measure::Kg,
    sequence::NonEmpty,
};

/// The normalised weigh-in a canonical one stands on: the store's id for it.
///
/// An id the store assigns, for the reason
/// [`crate::canonical::NormalisedSessionId`] is one: a cell names a copy, and
/// a Body Scan step is several records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalisedWeighInId(i64);

/// Why an id could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a normalised weigh-in id must not be negative")]
pub struct NegativeNormalisedWeighInId;

impl NormalisedWeighInId {
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for NormalisedWeighInId {
    type Error = NegativeNormalisedWeighInId;

    fn try_from(id: i64) -> Result<Self, Self::Error> {
        if id < 0 {
            return Err(NegativeNormalisedWeighInId);
        }
        Ok(Self(id))
    }
}

impl fmt::Display for NormalisedWeighInId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many weigh-ins a derivation read or wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct WeighInCount(usize);

impl WeighInCount {
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for WeighInCount {
    fn from(count: usize) -> Self {
        Self(count)
    }
}

impl fmt::Display for WeighInCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What an account says about when it was entered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stated {
    /// An instrument read the mass at the instant the account is dated to.
    Measured,
    /// Typed in, and stamped when: MyFitnessPal.
    Entered(jiff::Timestamp),
    /// A spreadsheet copy, stating nothing; its document was last saved then.
    ///
    /// **`None` for a copy derived before the save was read**, which a store
    /// written by an earlier build holds until the spreadsheets are derived
    /// again. It orders nothing, so copies it would have to be ordered against
    /// are left out rather than guessed between.
    Copied(Option<jiff::Timestamp>),
}

/// One normalised weigh-in, as the canonical layer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeighInAccount {
    pub id: NormalisedWeighInId,
    pub occurred: Occurred,
    pub mass: Kg,
    pub stated: Stated,
}

/// One real weighing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalWeighIn {
    occurred: Attributed<Occurred, NormalisedWeighInId>,
    mass: Attributed<Kg, NormalisedWeighInId>,
    accounts: NonEmpty<NormalisedWeighInId>,
}

impl CanonicalWeighIn {
    pub const fn occurred(&self) -> &Attributed<Occurred, NormalisedWeighInId> {
        &self.occurred
    }

    pub const fn mass(&self) -> &Attributed<Kg, NormalisedWeighInId> {
        &self.mass
    }

    /// Every normalised weigh-in this one stands for, the ones not taken
    /// included.
    pub const fn accounts(&self) -> &NonEmpty<NormalisedWeighInId> {
        &self.accounts
    }
}

impl fmt::Display for CanonicalWeighIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {}kg, {} accounts",
            self.occurred.value(),
            self.mass.value(),
            self.accounts.count()
        )
    }
}

/// Why a weighing was left out of the canonical layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsettled {
    /// Accounts that each state when they were entered, and differ.
    StatedAccountsDiffer,
    /// Copies that differ, and no single one was saved earliest.
    CopiesDiffer,
    /// Dated to a day with more than one measured weighing, and naming none.
    NoWeighingNamed,
}

impl fmt::Display for Unsettled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::StatedAccountsDiffer => "accounts stamped when they were entered differ",
            Self::CopiesDiffer => "copies differ and none was saved before the others",
            Self::NoWeighingNamed => "the day has several measured weighings and these name none",
        })
    }
}

/// A weighing the merge could not settle, named so the gap is visible (§ 37).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    pub on: Date,
    /// What each account held, in the order the merge read them.
    pub masses: NonEmpty<Kg>,
    pub why: Unsettled,
}

impl fmt::Display for LeftOut {
    /// `2016-01-05 (66.3kg, 66.6kg): copies differ and none was saved before
    /// the others`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (", self.on)?;
        for (at, mass) in self.masses.iter().enumerate() {
            if at > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{mass}kg")?;
        }
        write!(f, "): {}", self.why)
    }
}

/// The canonical weigh-ins, and the weighings left out of them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WeighIns {
    pub weighed: Vec<CanonicalWeighIn>,
    pub left_out: Vec<LeftOut>,
}

/// The canonical weigh-in layer the normalised one gives.
///
/// Deterministic (§ 9): weighings come out in day order, and within a day a
/// measured one in the order of its instant. Ties between accounts are broken
/// by id, so the result does not depend on the order `accounts` came in.
pub fn canonical_weigh_ins(mut accounts: Vec<WeighInAccount>) -> WeighIns {
    accounts.sort_by_key(|account| {
        (
            account.occurred.day(),
            account
                .occurred
                .instant()
                .map(|at| at.instant().as_second()),
            account.id,
        )
    });

    let mut out = WeighIns::default();
    let mut rest = accounts.as_slice();
    while let Some(first) = rest.first() {
        let day = first.occurred.day();
        let end = rest
            .iter()
            .position(|account| account.occurred.day() != day)
            .unwrap_or(rest.len());
        let (today, later) = rest.split_at(end);
        weigh_day(day, today, &mut out);
        rest = later;
    }
    out.weighed.sort_by_key(|weighed| {
        (
            weighed.occurred.value().day(),
            weighed
                .occurred
                .value()
                .instant()
                .map(|at| at.instant().as_second()),
            weighed.occurred.taken_from(),
        )
    });
    out
}

/// One day's accounts, sorted, into its weighings.
fn weigh_day(day: Date, accounts: &[WeighInAccount], out: &mut WeighIns) {
    let (measured, dated): (Vec<&WeighInAccount>, Vec<&WeighInAccount>) = accounts
        .iter()
        .partition(|account| account.occurred.instant().is_some());

    match measured.as_slice() {
        [] => settle(day, &dated, out),
        [only] => {
            let mut weighing = vec![*only];
            weighing.extend(dated);
            settle(day, &weighing, out);
        }
        several => {
            for account in several {
                settle(day, &[*account], out);
            }
            if !dated.is_empty() {
                leave_out(day, &dated, Unsettled::NoWeighingNamed, out);
            }
        }
    }
}

/// One weighing's accounts, merged or left out.
fn settle(day: Date, accounts: &[&WeighInAccount], out: &mut WeighIns) {
    let Some(&first) = accounts.first() else {
        return;
    };
    let mut stated: Vec<&WeighInAccount> = accounts
        .iter()
        .copied()
        .filter(|account| !matches!(account.stated, Stated::Copied(_)))
        .collect();
    // Of accounts that agree, the instrument's is named: it is the reading.
    stated.sort_by_key(|account| !matches!(account.stated, Stated::Measured));

    let taken = match stated.first() {
        None => earliest_copy(accounts),
        Some(&head) if stated.iter().all(|account| account.mass == head.mass) => Ok(head),
        Some(_) => Err(Unsettled::StatedAccountsDiffer),
    };
    let taken = match taken {
        Ok(taken) => taken,
        Err(why) => return leave_out(day, accounts, why, out),
    };

    // **The clock is the measured account's, where there is one**: a day and an
    // instant on that day are one fact at two precisions (`Occurred`).
    let clocked = accounts
        .iter()
        .find(|account| account.occurred.instant().is_some())
        .copied()
        .unwrap_or(taken);
    let accounts = NonEmpty::of(
        first.id,
        accounts.iter().skip(1).map(|account| account.id).collect(),
    );
    out.weighed.push(CanonicalWeighIn {
        occurred: Attributed::new(clocked.occurred.clone(), clocked.id),
        mass: Attributed::new(taken.mass, taken.id),
        accounts,
    });
}

/// Of copies of one entry, the one in the document saved earliest.
///
/// Copies that agree need no ordering, so a copy whose save was never read
/// costs nothing until it disagrees.
fn earliest_copy<'a>(copies: &[&'a WeighInAccount]) -> Result<&'a WeighInAccount, Unsettled> {
    let Some(&first) = copies.first() else {
        return Err(Unsettled::CopiesDiffer);
    };
    if copies.iter().all(|copy| copy.mass == first.mass) {
        return Ok(first);
    }
    let saved = |copy: &&WeighInAccount| match copy.stated {
        Stated::Copied(saved) => saved,
        Stated::Measured | Stated::Entered(_) => None,
    };
    let mut ordered = copies
        .iter()
        .map(|copy| saved(copy).map(|at| (at, *copy)))
        .collect::<Option<Vec<_>>>()
        .ok_or(Unsettled::CopiesDiffer)?;
    ordered.sort_by_key(|(at, copy)| (*at, copy.id));
    match ordered.as_slice() {
        [(earliest, taken), (next, _), ..] if earliest < next => Ok(taken),
        [(_, taken)] => Ok(taken),
        _ => Err(Unsettled::CopiesDiffer),
    }
}

fn leave_out(day: Date, accounts: &[&WeighInAccount], why: Unsettled, out: &mut WeighIns) {
    let masses = accounts.iter().map(|account| account.mass).collect();
    if let Ok(masses) = NonEmpty::new(masses) {
        out.left_out.push(LeftOut {
            on: day,
            masses,
            why,
        });
    }
}
