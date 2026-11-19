//! An evening's drinking, at whatever accuracy is known (#873).

use std::num::{NonZeroU8, NonZeroU16};

use domain::{
    drinking::{Drink, DrinkName, Drinking, Drinks, Millilitres, StandardServing, Strength},
    sequence::NonEmpty,
};
use proptest::prelude::*;

fn drinks(count: u8, serving: StandardServing) -> Option<Drinks> {
    let own = serving.as_own();
    Some(Drinks {
        count: NonZeroU8::new(count)?,
        name: own.name,
        drink: own.drink,
    })
}

/// Every NHS serving comes to the units the NHS gives it, to one place. The
/// bottle of wine is not in the NHS's table; its prose rounds 10.1 to 10.
#[test]
fn each_standard_serving_comes_to_the_nhs_units() {
    let nhs = [
        (StandardServing::SmallSpirit, "1.0"),
        (StandardServing::LargeSpirit, "1.4"),
        (StandardServing::Alcopop, "1.3"),
        (StandardServing::SmallWine, "1.5"),
        (StandardServing::StandardWine, "2.1"),
        (StandardServing::LargeWine, "3.0"),
        (StandardServing::BottleOfWine, "10.1"),
        (StandardServing::BottleOfBeer, "1.7"),
        (StandardServing::CanOfBeer, "2.4"),
        (StandardServing::PintLowerStrength, "2.0"),
        (StandardServing::PintHigherStrength, "3.0"),
    ];
    assert_eq!(nhs.len(), StandardServing::ALL.len());
    for (serving, units) in nhs {
        assert_eq!(
            serving.drink().units().to_string(),
            units,
            "{}",
            serving.name()
        );
    }
}

/// The NHS's worked example: a pint of 5.2 % lager is 2.95 units.
#[test]
fn a_pint_of_strong_lager_is_two_point_nine_five_units() {
    let pint = StandardServing::PintHigherStrength.drink();
    assert_eq!(pint.units().as_hundredths(), 295);
}

/// Three pints and a large glass of wine, exactly, is their sum; the same
/// counted as "at least" is a floor of the same figure; unknown has none.
#[test]
fn units_are_summed_over_what_was_counted() {
    let pints = drinks(3, StandardServing::PintHigherStrength).expect("three is not zero");
    let wine = drinks(1, StandardServing::LargeWine).expect("one is not zero");
    let counted = NonEmpty::of(pints, vec![wine]);

    let exactly = Drinking::Exactly(counted.clone());
    let at_least = Drinking::AtLeast(counted);

    assert_eq!(
        exactly.units().map(domain::drinking::Units::as_hundredths),
        Some(885 + 300)
    );
    assert_eq!(at_least.units(), exactly.units());
    assert_eq!(Drinking::AmountUnknown.units(), None);
}

#[test]
fn a_strength_reads_as_a_label_writes_it() {
    for (text, tenths) in [
        ("5", 50),
        ("5.2", 52),
        ("13.5%", 135),
        (" 40 % ", 400),
        ("0.5", 5),
    ] {
        let strength: Strength = text.parse().expect("a label's strength");
        assert_eq!(strength.as_tenths(), tenths, "{text}");
    }
}

#[test]
fn a_strength_of_nothing_or_more_than_the_whole_is_refused() {
    for text in ["0", "0.0", "100.1", "5.25", "five", "", ".5", "5."] {
        assert!(text.parse::<Strength>().is_err(), "{text:?} was accepted");
    }
}

#[test]
fn a_drink_name_is_trimmed_and_not_empty() {
    let name: DrinkName = "  house red ".parse().expect("a name");
    assert_eq!(name.as_str(), "house red");
    assert!("   ".parse::<DrinkName>().is_err());
}

fn any_drink() -> impl Strategy<Value = Drink> {
    (1_u16..=u16::MAX, 1_u16..=1_000).prop_filter_map("non-zero", |(ml, tenths)| {
        Some(Drink {
            volume: Millilitres::new(NonZeroU16::new(ml)?),
            strength: Strength::from_tenths(NonZeroU16::new(tenths)?).ok()?,
        })
    })
}

proptest! {
    #[test]
    fn a_strength_round_trips_through_its_text(tenths in 1_u16..=1_000) {
        let strength = Strength::from_tenths(NonZeroU16::new(tenths).expect("above zero"))
            .expect("at most the whole");
        let read: Strength = strength.to_string().parse().expect("its own text");
        prop_assert_eq!(read, strength);
    }

    #[test]
    fn a_volume_round_trips_through_its_number(ml in 1_u16..=u16::MAX) {
        let volume: Millilitres = ml.to_string().parse().expect("a whole number");
        prop_assert_eq!(volume.get(), ml);
    }

    /// Any drink has some alcohol in it, and twice as many is twice the units.
    #[test]
    fn units_scale_with_the_count(drink in any_drink(), count in 1_u8..=u8::MAX) {
        let one = Drinks {
            count: NonZeroU8::MIN,
            name: DrinkName::try_from("a drink").expect("a name"),
            drink,
        };
        let many = Drinks { count: NonZeroU8::new(count).expect("above zero"), ..one.clone() };
        prop_assert_eq!(
            many.units().as_hundredths(),
            one.units().as_hundredths() * u32::from(count)
        );
    }
}
