//! Drinking, reported through the use case over the real store (#873).

use std::num::{NonZeroU8, NonZeroU16};

use application::{DrinkingError, Node, Revisions, drinking::Evenings};
use domain::{
    drinking::{
        Drink, DrinkName, Drinking, Drinks, Evening, Millilitres, OwnDrink, StandardServing,
        Strength,
    },
    sequence::NonEmpty,
};
use infrastructure::{SqliteDrinkingStore, SqliteRevisions, connect};
use jiff::civil::date;

/// `None` only for a count of none.
fn pints(count: u8) -> Option<Drinks> {
    let pint = StandardServing::PintHigherStrength.as_own();
    Some(Drinks {
        count: NonZeroU8::new(count)?,
        name: pint.name,
        drink: pint.drink,
    })
}

/// `None` only if the domain refused a 175 ml glass at 13.5 %.
fn house_red() -> Option<OwnDrink> {
    Some(OwnDrink {
        name: DrinkName::try_from("house red").ok()?,
        drink: Drink {
            volume: Millilitres::new(NonZeroU16::new(175)?),
            strength: Strength::from_tenths(NonZeroU16::new(135)?).ok()?,
        },
    })
}

/// `None` only if the domain refused 14.5 %.
fn stronger(drink: OwnDrink) -> Option<OwnDrink> {
    Some(OwnDrink {
        drink: Drink {
            strength: Strength::from_tenths(NonZeroU16::new(145)?).ok()?,
            ..drink.drink
        },
        ..drink
    })
}

/// **Each of the three forms comes back as it was reported; reporting an
/// evening again replaces it, and removing it leaves no entry.** The drinks
/// written move the drinking node and nothing else.
#[test]
fn an_evening_is_kept_replaced_and_removed() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let pool = connect(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let evenings = Evenings::new(SqliteDrinkingStore::new(pool.clone()));
            let revisions = SqliteRevisions::new(pool);
            let today = date(2026, 11, 19);
            let red = house_red().expect("a glass of house red");

            let christmas_eve = Evening {
                date: date(2025, 12, 24),
                drinking: Drinking::AmountUnknown,
            };
            let friday = Evening {
                date: date(2026, 11, 14),
                drinking: Drinking::AtLeast(NonEmpty::of(pints(3).expect("three"), Vec::new())),
            };
            let tonight = Evening {
                date: today,
                drinking: Drinking::Exactly(NonEmpty::of(
                    pints(2).expect("two"),
                    vec![Drinks {
                        count: NonZeroU8::MIN,
                        name: red.name.clone(),
                        drink: red.drink,
                    }],
                )),
            };
            for evening in [&christmas_eve, &friday, &tonight] {
                evenings
                    .record(evening.clone(), today)
                    .await
                    .expect("recorded");
            }
            assert_eq!(
                evenings.evenings().await.expect("read"),
                vec![tonight.clone(), friday.clone(), christmas_eve.clone()],
                "latest first, each as reported"
            );

            let corrected = Evening {
                date: friday.date,
                drinking: Drinking::Exactly(NonEmpty::of(pints(4).expect("four"), Vec::new())),
            };
            evenings
                .record(corrected.clone(), today)
                .await
                .expect("recorded again");
            evenings.remove(christmas_eve.date).await.expect("removed");
            assert_eq!(
                evenings.evenings().await.expect("read"),
                vec![tonight, corrected]
            );

            assert_eq!(
                revisions
                    .revision(&Node::DRINKING)
                    .await
                    .expect("read")
                    .value(),
                5
            );
            assert_eq!(
                revisions
                    .revision(&Node::DIARY)
                    .await
                    .expect("read")
                    .value(),
                0
            );
        });
}

/// **An evening that has not come yet is refused**, and nothing is written.
#[test]
fn tomorrow_is_refused() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let pool = connect(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let evenings = Evenings::new(SqliteDrinkingStore::new(pool));

            let refused = evenings
                .record(
                    Evening {
                        date: date(2026, 11, 20),
                        drinking: Drinking::AmountUnknown,
                    },
                    date(2026, 11, 19),
                )
                .await;

            assert!(
                matches!(refused, Err(DrinkingError::NotYet { .. })),
                "{refused:?}"
            );
            assert!(evenings.evenings().await.expect("read").is_empty());
        });
}

/// **A drink is offered once, in the first group that has it**: the pint
/// drunk on Friday is recent and so not offered again under the NHS's, and
/// his own house red, renamed to a stronger pour, keeps one entry.
#[test]
fn the_choices_offer_recent_then_own_then_the_nhs() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(async {
            let directory = tempfile::tempdir().expect("a directory");
            let pool = connect(&directory.path().join("test.db"))
                .await
                .expect("a store");
            let evenings = Evenings::new(SqliteDrinkingStore::new(pool));

            evenings
                .record(
                    Evening {
                        date: date(2026, 11, 14),
                        drinking: Drinking::Exactly(NonEmpty::of(
                            pints(3).expect("three"),
                            Vec::new(),
                        )),
                    },
                    date(2026, 11, 19),
                )
                .await
                .expect("recorded");
            let red = house_red().expect("a glass of house red");
            evenings.name(red.clone()).await.expect("named");
            let stronger = stronger(red).expect("a stronger pour");
            evenings.name(stronger.clone()).await.expect("named again");

            let choices = evenings.choices().await.expect("read");

            assert_eq!(
                choices.recent,
                vec![StandardServing::PintHigherStrength.as_own()]
            );
            assert_eq!(choices.own, vec![stronger]);
            assert_eq!(choices.standard.len(), StandardServing::ALL.len() - 1);
            assert!(
                !choices
                    .standard
                    .contains(&StandardServing::PintHigherStrength.as_own())
            );
        });
}
