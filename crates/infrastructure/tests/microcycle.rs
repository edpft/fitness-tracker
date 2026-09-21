//! Issue #185's acceptance, against a real store.
//!
//! The week is the operator's own, 14 to 20 September 2026: Monday and Friday
//! evenings the gym's, Wednesday evening and Sunday morning cycling's, and a
//! holiday covering the Monday. Friday's gym session was delivered to Hevy;
//! the Wednesday ride was ridden.
//!
//! At the adapter's ring because what is under test is the join: the diary,
//! the plan, two delivery tables and two performed logs, read together into
//! one state per session. The rule itself is pinned in `domain`'s
//! `session_state`.
//!
//! **The performed logs are test adapters and the rest are real.** What a Hevy
//! session or a Peloton ride *is* has its own suites; here the question is only
//! which slot a date answers for.

mod support;

use std::num::NonZeroU8;

use application::{
    CyclingDeliveryStore as _, DestinationName, DiaryAuthor as _, PerformedSessionLog,
    PlanAuthor as _, PrescribedWorkoutStore as _, PrescriptionDeliveryStore as _, StoreError,
    microcycle::{Microcycle, MicrocyclePorts},
    prescribe::Authoring,
};
use domain::{
    cycling::{DeliveredRide, RideVenue, SessionPosition},
    planner::SessionState,
    provider::ProgrammeName,
    schedule::{Absence, AbsenceKind, Alteration, DayPart, PartOfDay},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCyclingDeliveryStore, SqliteDiaryStore, SqliteGenerationParameterStore, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, connect,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// Whatever dates it was built with, whatever it is asked.
///
/// A test adapter for [`PerformedSessionLog`]: the question the standing puts
/// to the record is only which days hold a session, and a real reader would
/// need a landed and derived corpus per discipline to answer it.
struct Trained {
    dates: Vec<Date>,
}

impl Trained {
    const fn nothing() -> Self {
        Self { dates: Vec::new() }
    }

    const fn on(dates: Vec<Date>) -> Self {
        Self { dates }
    }
}

impl PerformedSessionLog for Trained {
    async fn dates_between(&self, from: Date, to: Date) -> Result<Vec<Date>, StoreError> {
        Ok(self
            .dates
            .iter()
            .copied()
            .filter(|date| *date >= from && *date <= to)
            .collect())
    }
}

fn hevy() -> Fallible<DestinationName> {
    Ok(DestinationName::try_from("hevy".to_owned())?)
}

fn peloton() -> Fallible<DestinationName> {
    Ok(DestinationName::try_from("peloton".to_owned())?)
}

/// A store holding the operator's week and a plan covering the autumn.
///
/// The plan's *contents* do not matter to a state: what a slot is filled with
/// is the planner's answer and a state is read from the window and the record.
/// What the plan has to do is cover the week, so that there is a microcycle at
/// all.
async fn autumn() -> Fallible<(SqlitePool, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    programme::record_the_week(&pool).await?;

    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &programme::as_plan(programme::programme_from(Date::constant(2026, 9, 14))?)?,
        &programme::parameters()?,
    )
    .await?;

    Ok((pool, directory))
}

/// Away in Rome to the Monday, as his diary records it.
async fn away_over_the_monday(pool: &SqlitePool) -> Fallible<()> {
    let over = NonZeroU8::new(4).ok_or("four is not zero")?;
    SqliteDiaryStore::new(pool.clone())
        .record_alteration(&Alteration::new(
            Date::constant(2026, 9, 11),
            over,
            Absence::Holiday {
                zone: None,
                slots: Some(std::collections::BTreeMap::new()),
                reason: "No holiday, no gym access".to_owned(),
            },
        ))
        .await?;
    Ok(())
}

/// A gym prescription for the Friday, issued and delivered.
async fn gym_delivered_for_the_friday(pool: &SqlitePool) -> Fallible<()> {
    let friday = Date::constant(2026, 9, 18);
    // The parameters' own timestamp, read back rather than guessed: an issued
    // prescription names the version it was derived against, and the column is
    // a foreign key into it.
    let (authored_at, _) = application::GenerationParameterStore::current(
        &SqliteGenerationParameterStore::new(pool.clone()),
    )
    .await?
    .ok_or("the plan authored its parameters")?;

    let prescriptions =
        SqlitePrescribedWorkoutStore::new(pool.clone(), corpus::zone()?.id().to_owned());
    let id = prescriptions
        .issue(&programme::a_workout_for(friday, authored_at)?)
        .await?;

    let destination = hevy()?;
    let reference = application::DeliveryReference::try_from("a-hevy-routine".to_owned())?;
    SqlitePrescriptionDeliveryStore::new(pool.clone())
        .record(
            id,
            &destination,
            &reference,
            jiff::Timestamp::constant(1_758_000_000, 0),
        )
        .await?;
    Ok(())
}

fn a_delivered_ride(on: Date) -> Fallible<DeliveredRide> {
    Ok(DeliveredRide {
        prescribed_for: on,
        destination: peloton()?,
        programme: Some(ProgrammeName::try_from("Power Zone test".to_owned())?),
        microcycle: 1,
        session: SessionPosition::new(1)?,
        classes: NonEmpty::of(
            RideVenue::new("725d6185", "45 min Power Zone Endurance Ride")?,
            vec![RideVenue::new("9cc35942", "5 min Cool Down Ride")?],
        ),
        delivered_at: jiff::Timestamp::constant(1_758_000_000, 0),
    })
}

/// **A delivered ride round-trips**, cool-down and all.
///
/// The cool-down is the half that matters: it is chosen from the instructor at
/// the moment of delivery and is in no authored record, so if it is not stored
/// here it is nowhere.
#[test]
fn a_delivered_ride_keeps_every_class_that_went() {
    let read = corpus::block_on(async {
        let (pool, _directory) = autumn().await?;
        let store = SqliteCyclingDeliveryStore::new(pool.clone());
        let sunday = Date::constant(2026, 9, 20);
        store.record(&a_delivered_ride(sunday)?).await?;
        Ok::<_, Box<dyn std::error::Error>>(store.delivered_for(sunday, &peloton()?).await?)
    })
    .expect("a runtime is available")
    .expect("the store records and answers");

    let held = read.expect("the Sunday holds a delivery");
    assert_eq!(held.classes.iter().count(), 2);
    assert!(held.wrote("725d6185"), "the ride itself");
    assert!(held.wrote("9cc35942"), "and the cool-down chosen for it");
    assert_eq!(
        held.programme.as_ref().map(ProgrammeName::as_str),
        Some("Power Zone test")
    );
}

/// **Delivering again for a date replaces what went.**
///
/// The stack is one list shared with everything else the operator queues, so
/// a second delivery displaces the first rather than joining it — and the
/// record has to say the same, or a comparison would read a class that is no
/// longer there.
#[test]
fn a_second_delivery_for_a_date_replaces_the_first() {
    let read = corpus::block_on(async {
        let (pool, _directory) = autumn().await?;
        let store = SqliteCyclingDeliveryStore::new(pool.clone());
        let sunday = Date::constant(2026, 9, 20);

        store.record(&a_delivered_ride(sunday)?).await?;
        let mut second = a_delivered_ride(sunday)?;
        second.session = SessionPosition::new(2)?;
        second.classes = NonEmpty::of(RideVenue::new("4d302bef", "20 min FTP Test Ride")?, vec![]);
        store.record(&second).await?;

        Ok::<_, Box<dyn std::error::Error>>(store.delivered_for(sunday, &peloton()?).await?)
    })
    .expect("a runtime is available")
    .expect("the store records and answers");

    let held = read.expect("the Sunday holds a delivery");
    assert_eq!(held.session.as_u8(), 2);
    assert_eq!(
        held.classes.iter().count(),
        1,
        "the first delivery's classes are gone"
    );
    assert!(held.wrote("4d302bef"));
}

/// Every session of the week, read on the Saturday evening.
#[test]
fn the_microcycle_reports_a_state_for_every_session() {
    let sessions = corpus::block_on(async {
        let (pool, _directory) = autumn().await?;
        away_over_the_monday(&pool).await?;
        gym_delivered_for_the_friday(&pool).await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: Trained::nothing(),
                cycling_performed: Trained::on(vec![Date::constant(2026, 9, 16)]),
            },
            hevy()?,
            peloton()?,
        );

        Ok::<_, Box<dyn std::error::Error>>(
            standing
                .standing(DayPart::new(
                    Date::constant(2026, 9, 19),
                    PartOfDay::Evening,
                ))
                .await?,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    let read: Vec<String> = sessions
        .iter()
        .map(|session| {
            format!(
                "{} {} {} — {}",
                session.slot.discipline, session.number, session.slot.date, session.state
            )
        })
        .collect();

    assert_eq!(
        read,
        vec![
            "gym 1 2026-09-14 — skipped (holiday)",
            "cycling 1 2026-09-16 — performed",
            "gym 2 2026-09-18 — prescribed",
            "cycling 2 2026-09-20 — to be prescribed",
        ]
    );
}

/// **A ride recorded as delivered is prescribed** — the gap #185 exists to
/// close. Without the record the Sunday reads *to be prescribed* and a second
/// run would send the same session again.
#[test]
fn a_recorded_delivery_makes_a_ride_prescribed() {
    let sessions = corpus::block_on(async {
        let (pool, _directory) = autumn().await?;
        SqliteCyclingDeliveryStore::new(pool.clone())
            .record(&a_delivered_ride(Date::constant(2026, 9, 20))?)
            .await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: Trained::nothing(),
                cycling_performed: Trained::nothing(),
            },
            hevy()?,
            peloton()?,
        );

        Ok::<_, Box<dyn std::error::Error>>(
            standing
                .standing(DayPart::new(
                    Date::constant(2026, 9, 20),
                    PartOfDay::Morning,
                ))
                .await?,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    let sunday = sessions
        .iter()
        .find(|session| session.slot.date == Date::constant(2026, 9, 20))
        .expect("the Sunday is a session of the week");
    assert_eq!(sunday.state, SessionState::Prescribed);
}

/// **An illness over what is left closes the window and is named.**
#[test]
fn an_illness_makes_a_prescribed_session_not_performed() {
    let sessions = corpus::block_on(async {
        let (pool, _directory) = autumn().await?;
        gym_delivered_for_the_friday(&pool).await?;

        let over = NonZeroU8::new(2).ok_or("two is not zero")?;
        SqliteDiaryStore::new(pool.clone())
            .record_alteration(&Alteration::new(
                Date::constant(2026, 9, 18),
                over,
                Absence::Illness,
            ))
            .await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: Trained::nothing(),
                cycling_performed: Trained::nothing(),
            },
            hevy()?,
            peloton()?,
        );

        Ok::<_, Box<dyn std::error::Error>>(
            standing
                .standing(DayPart::new(
                    Date::constant(2026, 9, 19),
                    PartOfDay::Evening,
                ))
                .await?,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    let friday = sessions
        .iter()
        .find(|session| session.slot.date == Date::constant(2026, 9, 18))
        .expect("the Friday is a session of the week");
    assert_eq!(
        friday.state,
        SessionState::NotPerformed {
            absence: Some(AbsenceKind::Illness)
        }
    );
}
