//! One canonical weigh-in per weighing, settled as § 10 says (#409).
//!
//! The days are the operator's: 2016-01-05 is in six spreadsheet copies, one of
//! which differs; 2016-07-04 is in the same six and in MyFitnessPal.

use domain::{
    body::{NormalisedWeighInId, Stated, Unsettled, WeighInAccount, WeighIns, canonical_weigh_ins},
    canonical::Occurred,
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
};
use jiff::{
    Timestamp,
    civil::{Date, date},
};
use proptest::prelude::*;

type Failure = Box<dyn std::error::Error>;

fn id(id: i64) -> Result<NormalisedWeighInId, Failure> {
    Ok(NormalisedWeighInId::try_from(id)?)
}

fn copy(n: i64, on: Date, grams: u64, saved: Option<&str>) -> Result<WeighInAccount, Failure> {
    Ok(WeighInAccount {
        id: id(n)?,
        occurred: Occurred::On(on),
        mass: Kg::from_grams(grams),
        stated: Stated::Copied(saved.map(str::parse).transpose()?),
    })
}

fn entered(n: i64, on: Date, grams: u64, when: &str) -> Result<WeighInAccount, Failure> {
    Ok(WeighInAccount {
        id: id(n)?,
        occurred: Occurred::On(on),
        mass: Kg::from_grams(grams),
        stated: Stated::Entered(when.parse()?),
    })
}

fn measured(n: i64, when: &str, grams: u64) -> Result<WeighInAccount, Failure> {
    Ok(WeighInAccount {
        id: id(n)?,
        occurred: Occurred::At(StartedAt::new(
            when.parse()?,
            OperatorZone::try_from("Europe/London".to_owned())?,
        )),
        mass: Kg::from_grams(grams),
        stated: Stated::Measured,
    })
}

fn ids(ids: &[i64]) -> Result<Vec<NormalisedWeighInId>, Failure> {
    ids.iter().map(|n| id(*n)).collect()
}

/// 2016-01-05 as the six workbooks hold it, each with its document's last
/// save: `Body Weight`, the four `Body Weight 2018`s, and `2018 bodyweight
/// only`, which alone says 66.6.
fn six_copies(on: Date, slipped: u64) -> Result<Vec<WeighInAccount>, Failure> {
    Ok(vec![
        copy(6, on, slipped, Some("2018-05-19T07:51:27Z"))?,
        copy(10, on, 66_300, Some("2017-12-31T16:18:24Z"))?,
        copy(11, on, 66_300, Some("2017-12-31T16:18:24Z"))?,
        copy(12, on, 66_300, Some("2018-06-01T20:30:52Z"))?,
        copy(13, on, 66_300, Some("2018-05-16T05:44:49Z"))?,
        copy(14, on, 66_300, Some("2016-09-17T10:39:10Z"))?,
    ])
}

#[test]
fn a_day_in_six_copies_is_one_weigh_in_taken_from_the_earliest_saved() {
    let WeighIns { weighed, left_out } =
        canonical_weigh_ins(six_copies(date(2016, 1, 5), 66_600).expect("the copies"));

    assert!(left_out.is_empty(), "{left_out:?}");
    let [weigh_in] = weighed.as_slice() else {
        panic!("one weigh-in, not {weighed:?}");
    };
    assert_eq!(weigh_in.mass().value().to_string(), "66.3");
    assert_eq!(
        weigh_in.mass().taken_from(),
        id(14).expect("an id"),
        "Body Weight.xlsx"
    );
    assert_eq!(weigh_in.occurred().value(), &Occurred::On(date(2016, 1, 5)));
    let mut named: Vec<_> = weigh_in.accounts().iter().copied().collect();
    named.sort();
    assert_eq!(named, ids(&[6, 10, 11, 12, 13, 14]).expect("ids"));
}

#[test]
fn an_entry_stamped_when_it_was_typed_beats_every_copy() {
    let on = date(2016, 7, 4);
    let mut accounts = six_copies(on, 63_000).expect("the copies");
    for account in &mut accounts {
        if account.mass == Kg::from_grams(66_300) {
            account.mass = Kg::from_grams(63_600);
        }
    }
    accounts.push(entered(900, on, 63_600, "2016-07-04T06:23:04Z").expect("an account"));

    let WeighIns { weighed, left_out } = canonical_weigh_ins(accounts);

    assert!(left_out.is_empty(), "{left_out:?}");
    let [weigh_in] = weighed.as_slice() else {
        panic!("one weigh-in, not {weighed:?}");
    };
    assert_eq!(weigh_in.mass().value().to_string(), "63.6");
    assert_eq!(
        weigh_in.mass().taken_from(),
        id(900).expect("an id"),
        "MyFitnessPal"
    );
    assert_eq!(weigh_in.accounts().count(), 7);
}

#[test]
fn a_stamped_entry_stands_against_copies_that_all_disagree_with_it() {
    let on = date(2020, 1, 9);
    let WeighIns { weighed, left_out } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(45, on, 75_500, Some("2020-03-10T22:04:58Z"))?,
                entered(901, on, 75_200, "2020-01-10T08:54:23Z")?,
            ])
        })
        .expect("the accounts"),
    );

    assert!(left_out.is_empty());
    assert_eq!(weighed.len(), 1);
    assert_eq!(weighed[0].mass().value().to_string(), "75.2");
}

#[test]
fn copies_that_differ_and_cannot_be_ordered_are_left_out_and_named() {
    let on = date(2016, 1, 5);
    let unread = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(6, on, 66_600, None)?,
                copy(14, on, 66_300, Some("2016-09-17T10:39:10Z"))?,
            ])
        })
        .expect("the accounts"),
    );
    let tied = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(10, on, 66_600, Some("2017-12-31T16:18:24Z"))?,
                copy(11, on, 66_300, Some("2017-12-31T16:18:24Z"))?,
            ])
        })
        .expect("the accounts"),
    );

    for WeighIns { weighed, left_out } in [unread, tied] {
        assert!(weighed.is_empty());
        let [gap] = left_out.as_slice() else {
            panic!("one weighing left out, not {left_out:?}");
        };
        assert_eq!(gap.on, on);
        assert_eq!(gap.why, Unsettled::CopiesDiffer);
        assert_eq!(gap.masses.count(), 2);
    }
}

#[test]
fn copies_that_agree_need_no_save_to_order_them() {
    let on = date(2016, 1, 6);
    let WeighIns { weighed, left_out } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(6, on, 66_000, None)?,
                copy(14, on, 66_000, None)?,
            ])
        })
        .expect("the accounts"),
    );

    assert!(left_out.is_empty());
    assert_eq!(weighed.len(), 1);
}

#[test]
fn stamped_accounts_that_differ_are_left_out() {
    let on = date(2026, 1, 12);
    let WeighIns { weighed, left_out } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                measured(1, "2026-01-12T07:01:00Z", 80_150)?,
                entered(902, on, 80_400, "2026-01-12T07:05:00Z")?,
            ])
        })
        .expect("the accounts"),
    );

    assert!(weighed.is_empty());
    assert_eq!(left_out.len(), 1);
    assert_eq!(left_out[0].why, Unsettled::StatedAccountsDiffer);
}

#[test]
fn an_account_dated_to_a_day_joins_its_only_measured_weighing_and_takes_its_clock() {
    let on = date(2026, 1, 12);
    let WeighIns { weighed, left_out } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(7, on, 80_200, Some("2026-02-01T00:00:00Z"))?,
                measured(1, "2026-01-12T07:01:00Z", 80_150)?,
            ])
        })
        .expect("the accounts"),
    );

    assert!(left_out.is_empty());
    let [weigh_in] = weighed.as_slice() else {
        panic!("one weigh-in, not {weighed:?}");
    };
    assert_eq!(weigh_in.mass().taken_from(), id(1).expect("an id"));
    assert_eq!(weigh_in.occurred().taken_from(), id(1).expect("an id"));
    assert!(weigh_in.occurred().value().instant().is_some());
    assert_eq!(weigh_in.accounts().count(), 2);
}

#[test]
fn a_day_of_two_measured_weighings_leaves_out_an_account_naming_neither() {
    let on = date(2026, 1, 12);
    let WeighIns { weighed, left_out } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                measured(1, "2026-01-12T07:01:00Z", 80_150)?,
                measured(2, "2026-01-12T19:30:00Z", 81_000)?,
                copy(7, on, 80_200, Some("2026-02-01T00:00:00Z"))?,
            ])
        })
        .expect("the accounts"),
    );

    assert_eq!(weighed.len(), 2);
    assert_eq!(left_out.len(), 1);
    assert_eq!(left_out[0].why, Unsettled::NoWeighingNamed);
}

#[test]
fn a_weighing_left_out_says_which_day_what_each_held_and_why() {
    let WeighIns { left_out, .. } = canonical_weigh_ins(
        built(|| {
            Ok(vec![
                copy(10, date(2016, 1, 5), 66_600, Some("2017-12-31T16:18:24Z"))?,
                copy(11, date(2016, 1, 5), 66_300, Some("2017-12-31T16:18:24Z"))?,
            ])
        })
        .expect("the accounts"),
    );

    assert_eq!(
        left_out[0].to_string(),
        "2016-01-05 (66.6kg, 66.3kg): copies differ and none was saved before the others"
    );
}

/// The accounts a closure builds, for the test to unwrap.
fn built(
    build: impl FnOnce() -> Result<Vec<WeighInAccount>, Failure>,
) -> Result<Vec<WeighInAccount>, Failure> {
    build()
}

fn account() -> impl Strategy<Value = WeighInAccount> {
    (
        0_i64..40,
        0_i64..6,
        prop::sample::select(vec![66_000_u64, 66_300, 66_600]),
        0_u8..4,
        prop::option::of(0_i64..3),
    )
        .prop_filter_map("a valid account", |(n, day, grams, kind, saved)| {
            let on = date(2016, 1, 1)
                .checked_add(jiff::Span::new().days(day))
                .ok()?;
            let saved = match saved {
                Some(second) => Some(Timestamp::from_second(1_500_000_000 + second).ok()?),
                None => None,
            };
            let stated = match kind {
                0 => Stated::Entered(Timestamp::from_second(1_451_606_400).ok()?),
                _ => Stated::Copied(saved),
            };
            Some(WeighInAccount {
                id: NormalisedWeighInId::try_from(n).ok()?,
                occurred: Occurred::On(on),
                mass: Kg::from_grams(grams),
                stated,
            })
        })
}

proptest! {
    /// Every account lands somewhere exactly once, and the order they were read
    /// in changes nothing (§ 9).
    #[test]
    fn every_account_is_weighed_or_left_out_whatever_order_it_came_in(
        accounts in prop::collection::vec(account(), 0..30)
            .prop_map(|mut accounts| {
                accounts.sort_by_key(|account| account.id);
                accounts.dedup_by_key(|account| account.id);
                accounts
            })
            .prop_shuffle(),
    ) {
        let mut reversed = accounts.clone();
        reversed.reverse();
        let merged = canonical_weigh_ins(accounts.clone());
        prop_assert_eq!(&merged, &canonical_weigh_ins(reversed));

        let named: usize = merged.weighed.iter().map(|weighed| weighed.accounts().count()).sum();
        let left: usize = merged.left_out.iter().map(|gap| gap.masses.count()).sum();
        prop_assert_eq!(named + left, accounts.len());
    }
}
