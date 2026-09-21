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
    CyclingDeliveryStore as _, CyclingMesocycleStore, DestinationName, DiaryAuthor as _,
    DiaryStore as _, MesocycleStore, PerformedSessionLog, PlanAuthor as _,
    PrescribedWorkoutStore as _, PrescriptionDeliveryStore as _, RiddenSession, RiddenSessionLog,
    StoreError,
    microcycle::{Microcycle, MicrocyclePorts},
    prescribe::Authoring,
    reschedule::{Reschedule, Rescheduled},
};
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, CyclingSession, DeliveredRide,
        Interval, PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    measure::PositiveDuration,
    plan::{Plan, PlanName, Programme},
    planner::{MicrocycleState, SessionState},
    prescription::{WeekIndex, WeekKind},
    provider::ProgrammeName,
    schedule::{
        Absence, AbsenceKind, Alteration, DayPart, Discipline, PartOfDay, Relative, SessionRole,
    },
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore, SqliteDiaryStore,
    SqliteGenerationParameterStore, SqliteGymMesocycleStore, SqlitePlanStore,
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

/// Rides on given days, at given classes.
///
/// A test adapter for [`RiddenSessionLog`]. The classes are what makes a ride
/// answer for the slot holding *its* role rather than for whichever slot it
/// happened to land after (#177), so a session ridden at no class is the case
/// where the record cannot say — a ride normalised before the column existed,
/// or one taken under the operator's own steam.
struct Ridden {
    sessions: Vec<(Date, Vec<RideVenue>)>,
}

impl Ridden {
    const fn nothing() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    const fn of(sessions: Vec<(Date, Vec<RideVenue>)>) -> Self {
        Self { sessions }
    }
}

impl RiddenSessionLog for Ridden {
    async fn ridden_between(&self, from: Date, to: Date) -> Result<Vec<RiddenSession>, StoreError> {
        Ok(self
            .sessions
            .iter()
            .filter(|(date, _)| *date >= from && *date <= to)
            .map(|(date, at)| RiddenSession {
                on: *date,
                at: at.iter().cloned().collect(),
            })
            .collect())
    }
}

/// The harder, shorter ride of the operator's test week: the FTP test.
///
/// Two classes and one session, which is the case a match on class identity has
/// to cover — riding either is riding the test.
fn ftp_test() -> Fallible<PlannedRide> {
    Ok(PlannedRide::assembled(
        CyclingSession::new(
            PositiveDuration::from_seconds(600)?,
            Ride::Effort(PositiveDuration::from_seconds(1200)?),
            None,
        ),
        NonEmpty::of(
            RideVenue::new("1eabf70b", "10 min FTP Warm Up Ride")?,
            vec![RideVenue::new("4d302bef", "20 min FTP Test Ride")?],
        ),
        SessionRole::new(Relative::Higher, Relative::Lower),
    ))
}

/// The easier, longer ride: the one he actually rode, on the Wednesday.
fn endurance() -> Fallible<PlannedRide> {
    Ok(PlannedRide::assembled(
        CyclingSession::new(
            PositiveDuration::from_seconds(300)?,
            Ride::Intervals(NonEmpty::of(
                Interval::new(PowerZone::Two, PositiveDuration::from_seconds(2400)?),
                Vec::new(),
            )),
            Some(PositiveDuration::from_seconds(300)?),
        ),
        NonEmpty::of(
            RideVenue::new("725d6185", "45 min Power Zone Endurance Ride")?,
            Vec::new(),
        ),
        SessionRole::new(Relative::Lower, Relative::Higher),
    ))
}

/// A plan whose cycling week is the operator's test microcycle.
///
/// Cycling-only, because what is under test is which cycling slot a ride
/// answers for; the gym's half of the week has its own cases above.
async fn autumn_on_the_bike() -> Fallible<(SqlitePool, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    programme::record_the_week(&pool).await?;

    let rides = [
        (SessionPosition::new(1)?, endurance()?),
        (SessionPosition::new(2)?, ftp_test()?),
    ]
    .into_iter()
    .collect();
    let week = CyclingMicrocycle::new(rides)?;
    let mesocycle = CyclingMesocycle::new(
        CyclingProvenance::Assembled,
        Date::constant(2026, 9, 14),
        NonEmpty::of(week, Vec::new()),
    )?;

    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &Plan::new(
            PlanName::try_from("2026-autumn-bike".to_owned())?,
            jiff::Timestamp::now(),
            None,
            Some(Programme::new(vec![mesocycle])?),
        )?,
        &programme::parameters()?,
    )
    .await?;

    Ok((pool, directory))
}

/// A concurrent plan from Monday 14 September: the gym's block and the bike's
/// test week, then a second cycling mesocycle from the Monday after.
///
/// **Both disciplines, because the microcycle is the concurrent one** (#177):
/// whether a week is lost is read across the two, and what a lost week moves is
/// both programmes at once.
async fn autumn_together() -> Fallible<(SqlitePool, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    programme::record_the_week(&pool).await?;

    let rides = [
        (SessionPosition::new(1)?, endurance()?),
        (SessionPosition::new(2)?, ftp_test()?),
    ]
    .into_iter()
    .collect::<std::collections::BTreeMap<_, _>>();
    let test_week = CyclingMesocycle::new(
        CyclingProvenance::Assembled,
        Date::constant(2026, 9, 14),
        NonEmpty::of(CyclingMicrocycle::new(rides)?, Vec::new()),
    )?;
    let build = test_week.starting_on(Date::constant(2026, 9, 21));

    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &Plan::new(
            PlanName::try_from("2026-autumn".to_owned())?,
            jiff::Timestamp::now(),
            Some(Programme::new(vec![programme::as_programme(
                programme::programme_from(Date::constant(2026, 9, 14))?,
            )])?),
            Some(Programme::new(vec![test_week, build])?),
        )?,
        &programme::parameters()?,
    )
    .await?;

    Ok((pool, directory))
}

/// Ill from Thursday 17 to Sunday 20 September, as the operator is extending
/// his record to say.
async fn ill_to_the_sunday(pool: &SqlitePool) -> Fallible<()> {
    let over = NonZeroU8::new(4).ok_or("four is not zero")?;
    SqliteDiaryStore::new(pool.clone())
        .record_alteration(&Alteration::new(
            Date::constant(2026, 9, 17),
            over,
            Absence::Illness,
        ))
        .await?;
    Ok(())
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
                cycling_performed: Ridden::of(vec![(Date::constant(2026, 9, 16), Vec::new())]),
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
                .await?
                .sessions,
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
                cycling_performed: Ridden::nothing(),
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
                .await?
                .sessions,
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
                cycling_performed: Ridden::nothing(),
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
                .await?
                .sessions,
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

/// **The ride he actually rode answers for the slot its class belongs to**,
/// which is #177's correction to #185. On Wednesday 16 September the operator
/// rode the 45 min Power Zone Endurance Ride — the week's easier, longer
/// session. The Wednesday slot holds the FTP test.
///
/// Before this, the Wednesday read *performed* on nothing more than the day it
/// fell on, the FTP test went unridden into the first Build mesocycle, and that
/// mesocycle opened against an FTP estimated on 2026-07-22.
#[test]
fn a_ride_answers_for_the_slot_holding_its_class() {
    let sessions = corpus::block_on(async {
        let (pool, _directory) = autumn_on_the_bike().await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: Trained::nothing(),
                cycling_performed: Ridden::of(vec![(
                    Date::constant(2026, 9, 16),
                    vec![RideVenue::new(
                        "725d6185",
                        "45 min Power Zone Endurance Ride",
                    )?],
                )]),
            },
            hevy()?,
            peloton()?,
        );

        Ok::<_, Box<dyn std::error::Error>>(
            standing
                .standing(DayPart::new(
                    Date::constant(2026, 9, 20),
                    PartOfDay::Evening,
                ))
                .await?
                .sessions,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    // The gym's slots are in the week too — the diary holds them whatever the
    // plan programmes — and what is under test is which *cycling* slot the ride
    // answered for.
    let read: Vec<String> = sessions
        .iter()
        .filter(|session| session.slot.discipline == Discipline::Cycling)
        .map(|session| format!("{} — {}", session.slot.date, session.state))
        .collect();

    assert_eq!(
        read,
        vec!["2026-09-16 — not prescribed", "2026-09-20 — performed"],
        "the FTP test is still owed and the endurance ride is done"
    );
}

/// **A ride at a class the plan does not name falls back to whose turn it
/// was.** A ride taken under the operator's own steam, or one normalised
/// before the class was recorded, says only when it was — and the rule that
/// answered before #177 still answers.
#[test]
fn a_ride_the_record_cannot_name_answers_for_whose_turn_it_was() {
    let sessions = corpus::block_on(async {
        let (pool, _directory) = autumn_on_the_bike().await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: Trained::nothing(),
                cycling_performed: Ridden::of(vec![(Date::constant(2026, 9, 16), Vec::new())]),
            },
            hevy()?,
            peloton()?,
        );

        Ok::<_, Box<dyn std::error::Error>>(
            standing
                .standing(DayPart::new(
                    Date::constant(2026, 9, 20),
                    PartOfDay::Evening,
                ))
                .await?
                .sessions,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    let wednesday = sessions
        .iter()
        .find(|session| session.slot.date == Date::constant(2026, 9, 16))
        .expect("the Wednesday is a session of the week");
    assert_eq!(wednesday.state, SessionState::Performed);
}

/// **#177's acceptance, against a real store.** The operator's week of 14
/// September: the Monday lost to Rome, the entry test delivered for the Friday
/// and lost to illness, the Sunday lost too, and the one ride of the week the
/// *easier* one. Neither essential session was performed.
///
/// Run on the Monday after, the week is read as lost, and the plan moves: the
/// week of 21 September is the microcycle of the 14th run again — the gym's
/// first climbing week, the bike's FTP test on the Wednesday — and the next
/// cycling mesocycle starts on the 28th rather than the 21st.
#[test]
fn a_week_that_lost_both_essential_sessions_runs_again() {
    let (standing, gym, wednesday, next_mesocycle) = corpus::block_on(async {
        let (pool, _directory) = autumn_together().await?;
        away_over_the_monday(&pool).await?;
        gym_delivered_for_the_friday(&pool).await?;
        ill_to_the_sunday(&pool).await?;

        let standing = Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                // The last gym session he did, a week before the plan began.
                // It answers for nothing in it.
                gym_performed: Trained::on(vec![Date::constant(2026, 9, 7)]),
                cycling_performed: Ridden::of(vec![(
                    Date::constant(2026, 9, 16),
                    vec![RideVenue::new(
                        "725d6185",
                        "45 min Power Zone Endurance Ride",
                    )?],
                )]),
            },
            hevy()?,
            peloton()?,
        )
        .standing(DayPart::new(
            Date::constant(2026, 9, 21),
            PartOfDay::Evening,
        ))
        .await?;

        let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
        let reschedule = Reschedule::new(standing.lost.clone(), diary);
        let gym = Rescheduled::new(
            SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            SqlitePlanStore::new(pool.clone(), corpus::zone()?),
            reschedule.clone(),
        );
        let bike = Rescheduled::new(
            SqliteCyclingMesocycleStore::new(pool.clone()),
            SqlitePlanStore::new(pool.clone(), corpus::zone()?),
            reschedule,
        );

        let monday = Date::constant(2026, 9, 21);
        let (_, _, block) = MesocycleStore::on(&gym, monday)
            .await?
            .ok_or("the block answers for the Monday")?;
        let gym_week = block.calendar().place(monday)?.0;

        let wednesday = Date::constant(2026, 9, 23);
        let (_, _, riding) = CyclingMesocycleStore::on(&bike, wednesday)
            .await?
            .ok_or("the test week answers for the Wednesday")?;
        let ridden = riding
            .microcycle_of(wednesday)
            .and_then(|number| riding.microcycle(number))
            .and_then(|week| week.for_role(SessionRole::new(Relative::Higher, Relative::Lower)))
            .map(|(_, ride)| ride.at().first().called().to_owned());

        let (_, _, following) = CyclingMesocycleStore::following(&bike, wednesday)
            .await?
            .ok_or("a second cycling mesocycle follows")?;

        Ok::<_, Box<dyn std::error::Error>>((standing, gym_week, ridden, following.start()))
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    assert_eq!(
        standing.weeks,
        vec![(Date::constant(2026, 9, 14), MicrocycleState::Incomplete)]
    );
    assert_eq!(standing.lost, vec![Date::constant(2026, 9, 14)]);
    assert_eq!(
        gym,
        WeekKind::Climbing(WeekIndex::FIRST),
        "the gym's first week runs again"
    );
    assert_eq!(
        wednesday.as_deref(),
        Some("10 min FTP Warm Up Ride"),
        "the FTP test takes the Wednesday"
    );
    assert_eq!(
        next_mesocycle,
        Date::constant(2026, 9, 28),
        "the next mesocycle moves back a week"
    );
}

/// **A week whose essential sessions were performed moves nothing**, whatever
/// else it lost. Rome still takes the Monday; the entry test is done on the
/// Saturday and the FTP test on the Wednesday, so the plan runs as written.
#[test]
fn a_week_that_kept_both_essential_sessions_moves_nothing() {
    let standing = corpus::block_on(async {
        let (pool, _directory) = autumn_together().await?;
        away_over_the_monday(&pool).await?;
        gym_delivered_for_the_friday(&pool).await?;

        Ok::<_, Box<dyn std::error::Error>>(
            Microcycle::new(
                MicrocyclePorts {
                    diary: SqliteDiaryStore::new(pool.clone()),
                    plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
                    gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                    cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                    gym_performed: Trained::on(vec![Date::constant(2026, 9, 19)]),
                    cycling_performed: Ridden::of(vec![(
                        Date::constant(2026, 9, 16),
                        vec![RideVenue::new("4d302bef", "20 min FTP Test Ride")?],
                    )]),
                },
                hevy()?,
                peloton()?,
            )
            .standing(DayPart::new(
                Date::constant(2026, 9, 21),
                PartOfDay::Evening,
            ))
            .await?,
        )
    })
    .expect("a runtime is available")
    .expect("the store authors and answers");

    assert_eq!(
        standing.weeks,
        vec![(Date::constant(2026, 9, 14), MicrocycleState::Completed)]
    );
    assert!(standing.lost.is_empty(), "{:?}", standing.lost);
}
