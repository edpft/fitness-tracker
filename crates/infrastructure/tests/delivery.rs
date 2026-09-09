//! Delivering an issued session, against a real store.
//!
//! The destination is a counting fake rather than a mock HTTP server: what these
//! assert is the *use case's* behaviour — that a session is sent once, that
//! asking again sends nothing, and that a reissue is a session in its own right
//! — and none of that is about HTTP. What goes on the wire is
//! `hevy_routine_contract`.

mod support;

use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use application::{
    Deliverable, Delivered, DeliveryAttempt, DeliveryError, DeliveryReference, DestinationName,
    DestinationReply, Issuance, PlanAuthor as _, PrescribedWorkoutId, PrescriptionDeliverer as _,
    PrescriptionDestination, ReplyStatus, WorkoutPrescriber as _,
    deliver::{Delivering, DeliveryPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use infrastructure::{
    HevySessionAccountReader, HevySessionTranslator, HevyWorkoutLandingStore,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteGymSessionStore, SqliteNormalisationRunLog, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, SqliteRefusalStore, connect,
};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

/// A destination that keeps count and invents a reference per call.
///
/// The counting is the assertion: "delivered once" is not observable from the
/// store alone, because a store that recorded a second delivery and a
/// destination that received one look identical from there.
struct Counting {
    name: DestinationName,
    calls: AtomicUsize,
    /// Counted apart from `calls`, because "was this a create or an update?" is
    /// the question decision 0022 turns on and the store cannot answer it.
    replacements: AtomicUsize,
    titles: Mutex<Vec<String>>,
}

impl Counting {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            name: DestinationName::try_from("hevy".to_owned())?,
            calls: AtomicUsize::new(0),
            replacements: AtomicUsize::new(0),
            titles: Mutex::new(Vec::new()),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn replacements(&self) -> usize {
        self.replacements.load(Ordering::SeqCst)
    }
}

impl PrescriptionDestination for Counting {
    fn name(&self) -> &DestinationName {
        &self.name
    }

    async fn deliver(&self, session: &Deliverable) -> DeliveryAttempt {
        let seen = self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut titles) = self.titles.lock() {
            titles.push(format!(
                "{:02} {}",
                session.ordinal.as_u32(),
                session.workout.session_role()
            ));
        }

        let outcome = DeliveryReference::try_from(format!("routine-{seen}"))
            .map(|reference| Delivered {
                reference,
                unexpressed: Vec::new(),
            })
            .map_err(|error| DeliveryError::Unidentifiable {
                destination: "hevy".to_owned(),
                message: error.to_string(),
            });

        match reply(&format!(r#"{{"routine":{{"id":"routine-{seen}"}}}}"#), true) {
            Ok(reply) => DeliveryAttempt::answered(reply, outcome),
            Err(error) => DeliveryAttempt::unanswered(error),
        }
    }

    /// **Keeps the reference it was given**, which is what a real `PUT` does and
    /// is the whole property under test: the operator's routine changes
    /// contents without changing identity.
    async fn replace(
        &self,
        session: &Deliverable,
        occupying: &DeliveryReference,
    ) -> DeliveryAttempt {
        self.replacements.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut titles) = self.titles.lock() {
            titles.push(format!(
                "{:02} {} (replaced)",
                session.ordinal.as_u32(),
                session.workout.session_role()
            ));
        }

        let outcome = Ok(Delivered {
            reference: occupying.clone(),
            unexpressed: Vec::new(),
        });

        match reply(r#"{"routine":{"id":"replaced"}}"#, true) {
            Ok(reply) => DeliveryAttempt::answered(reply, outcome),
            Err(error) => DeliveryAttempt::unanswered(error),
        }
    }
}

/// A reply a fake destination can hand back.
///
/// Fallible and returning `Result` rather than expecting, because the test
/// exemptions reach a `#[test]` body and not a free function beside it — the
/// callers above turn a failure into an unanswered attempt, which is a state the
/// port already models.
fn reply(body: &str, succeeded: bool) -> Result<DestinationReply, DeliveryError> {
    let status = ReplyStatus::new(
        if succeeded {
            "201 Created"
        } else {
            "400 Bad Request"
        },
        succeeded,
    )
    .map_err(|error| DeliveryError::Unidentifiable {
        destination: "hevy".to_owned(),
        message: error.to_string(),
    })?;

    Ok(DestinationReply::new(status, body.as_bytes().to_vec()))
}

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteGymMesocycleStore,
    SqliteGenerationParameterStore,
    SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore,
>;

struct Ready {
    prescriber: Prescriber,
    pool: SqlitePool,
    /// Carried rather than re-read. Building it can fail, and a free function
    /// here may not panic — the test exemptions reach `#[test]` bodies and not
    /// the helpers beside them.
    zone: domain::normalised::OperatorZone,
    _directory: tempfile::TempDir,
}

/// The corpus, landed and derived, with the fixture programme authored.
async fn ready() -> Result<Ready, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;

    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let run = application::ExtractionRunLog::begin(
        &runs,
        application::LandingStore::stream(&landing),
        domain::landing::FetchedAt::EPOCH,
    )
    .await?;
    let records = corpus::records()?
        .into_iter()
        .map(|landed| landed.record().clone())
        .collect();
    application::LandingStore::append(&landing, run, records).await?;

    let normalisation = application::normalise::Normalisation::new(
        application::normalise::NormalisationPorts {
            raw: HevySessionAccountReader::new(pool.clone())?,
            translator: HevySessionTranslator,
            workouts: SqliteGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        corpus::zone()?,
    );
    application::WorkoutNormaliser::normalise(&normalisation).await?;

    Authoring::new(
        SqlitePlanStore::new(pool.clone(), corpus::zone()?),
        SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &programme::as_plan(programme::programme()?)?,
        &programme::parameters()?,
    )
    .await?;

    Ok(Ready {
        prescriber: Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool.clone()),
            prescriptions: SqlitePrescribedWorkoutStore::new(
                pool.clone(),
                "Europe/London".to_owned(),
            ),
            lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
        }),
        pool,
        zone: corpus::zone()?,
        _directory: directory,
    })
}

fn delivering<'a>(
    ready: &Ready,
    destination: &'a Counting,
) -> Delivering<
    SqlitePrescribedWorkoutStore,
    SqliteGymMesocycleStore,
    SqlitePrescriptionDeliveryStore,
    &'a Counting,
> {
    Delivering::new(DeliveryPorts {
        prescriptions: SqlitePrescribedWorkoutStore::new(
            ready.pool.clone(),
            "Europe/London".to_owned(),
        ),
        programmes: SqliteGymMesocycleStore::new(ready.pool.clone(), ready.zone.clone()),
        deliveries: SqlitePrescriptionDeliveryStore::new(ready.pool.clone()),
        destination,
    })
}

const fn monday() -> Date {
    Date::constant(2026, 8, 10)
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the operation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// **Asked twice is one session.** The destination cannot delete what it has
/// been given, so a second delivery would leave the operator two routines for
/// one date and nothing to say which was in force.
#[test]
fn delivering_twice_sends_once() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let first = run!(async {
        ready.prescriber.prescribe(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>(
            delivering(&ready, &destination).deliver(monday()).await?,
        )
    });

    let second = run!(delivering(&ready, &destination).deliver(monday()));

    assert!(first.freshly_delivered, "the first delivery is fresh");
    assert!(!second.freshly_delivered, "the second is not");
    assert_eq!(
        first.reference, second.reference,
        "and it reports the reference already recorded"
    );
    assert_eq!(destination.calls(), 1, "the destination heard from us once");
}

/// **Deriving again is not reissuing.** Since decision 0021 the ordinary run
/// derives on every call, and a derivation that produces the same workout is the
/// same prescription — so the daily loop can be run as often as the operator
/// likes without a second routine appearing on their phone.
///
/// This is the test that would have caught the defect the decision fixes: the
/// delivery guard is keyed on the prescription's identity, which was sound only
/// while nothing ever re-derived.
#[test]
fn deriving_the_same_session_again_delivers_nothing() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let first = run!(async {
        ready.prescriber.prescribe(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>(
            delivering(&ready, &destination).deliver(monday()).await?,
        )
    });

    let second = run!(async {
        let issued = ready.prescriber.prescribe(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>((
            issued,
            delivering(&ready, &destination).deliver(monday()).await?,
        ))
    });
    let (issued, delivered) = second;

    assert_eq!(
        issued.issuance,
        Issuance::Unchanged,
        "the record has not moved, so neither has the session"
    );
    assert!(!delivered.freshly_delivered);
    assert_eq!(
        first.reference, delivered.reference,
        "the session already sent is the session in force"
    );
    assert_eq!(destination.calls(), 1, "the destination heard from us once");
}

/// **A corrected session replaces the one already delivered, in place.**
///
/// Decision 0022. Before it, this test asserted the opposite — that a reissue
/// was delivered as a session of its own, under its own reference — which was
/// the honest consequence of a destination that could only create. It left the
/// operator two routines for one Monday and no way to delete either.
///
/// The programme is re-authored to start a fortnight later, which puts the same
/// date on a different rung. That is the operator correcting a block, which is
/// the case a reissue exists for.
#[test]
fn a_corrected_session_replaces_the_one_already_delivered() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let first = run!(async {
        let issued = ready.prescriber.prescribe(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>((
            issued,
            delivering(&ready, &destination).deliver(monday()).await?,
        ))
    });
    let (first_issued, first_delivery) = first;

    let (issued, second) = run!(async {
        Authoring::new(
            SqlitePlanStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGymMesocycleStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGenerationParameterStore::new(ready.pool.clone()),
        )
        .author(
            &programme::as_plan(programme::programme_from(Date::constant(2026, 7, 20))?)?,
            &programme::parameters()?,
        )
        .await?;

        let issued = ready.prescriber.prescribe(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>((
            issued,
            delivering(&ready, &destination).deliver(monday()).await?,
        ))
    });

    let Issuance::Superseded { stranded, .. } = &issued.issuance else {
        panic!(
            "the corrected block derives a different session: {:?}",
            issued.issuance
        )
    };
    assert_eq!(
        stranded.as_ref(),
        Some(&first_delivery.reference),
        "and names the delivered session it has left standing"
    );

    assert!(second.freshly_delivered, "the correction is sent");
    assert_eq!(
        second.replaced,
        Some(first_issued.id),
        "as a replacement of the prescription that held the place"
    );
    assert_eq!(
        first_delivery.reference, second.reference,
        "into the same routine, so the operator has one session for the date"
    );
    assert_eq!(destination.calls(), 1, "one create");
    assert_eq!(destination.replacements(), 1, "and one update");
}

/// **The place changes hands, rather than being shared.**
///
/// The store half of the test above, and the reason the hand-over is a delete
/// and an insert: two prescriptions holding one reference would make a single
/// performed workout answer for both of them, and `state_of` could not say
/// which session was trained.
#[test]
fn a_replacement_moves_the_delivery_record() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let superseded = run!(async {
        let issued = ready.prescriber.prescribe(monday()).await?;
        delivering(&ready, &destination).deliver(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>(issued.id)
    });

    let replacing = run!(async {
        Authoring::new(
            SqlitePlanStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGymMesocycleStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGenerationParameterStore::new(ready.pool.clone()),
        )
        .author(
            &programme::as_plan(programme::programme_from(Date::constant(2026, 7, 20))?)?,
            &programme::parameters()?,
        )
        .await?;
        let issued = ready.prescriber.prescribe(monday()).await?;
        delivering(&ready, &destination).deliver(monday()).await?;
        Ok::<_, Box<dyn std::error::Error>>(issued.id)
    });

    let rows: Vec<(i64, String)> = run!(async {
        sqlx::query!(
            r#"SELECT prescription AS "prescription!: i64", reference AS "reference!: String"
               FROM prescription_delivery"#
        )
        .fetch_all(&ready.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|row| (row.prescription, row.reference))
                .collect()
        })
    });

    assert_eq!(rows.len(), 1, "one row, because the place was handed over");
    assert_eq!(
        rows[0].0,
        replacing.as_i64(),
        "held by the prescription in force"
    );
    assert_ne!(
        rows[0].0,
        superseded.as_i64(),
        "and not by the one it superseded"
    );
}

/// Nothing issued is not an error to paper over by issuing one: deriving a
/// session advances a ladder, and doing that as a side effect of a delivery
/// would hide it.
#[test]
fn a_date_with_nothing_issued_delivers_nothing() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let refused = match corpus::block_on(delivering(&ready, &destination).deliver(monday())) {
        Ok(outcome) => outcome,
        Err(error) => panic!("a runtime is available: {error}"),
    };

    match refused {
        Err(DeliveryError::NothingIssued { date }) => assert_eq!(date, monday()),
        Err(other) => panic!("the wrong refusal: {other}"),
        Ok(_) => panic!("a date with no prescription delivered something"),
    }
    assert_eq!(destination.calls(), 0);
}

/// The identity a delivery is recorded against outlives the process: a second
/// invocation reads it back rather than sending again.
#[test]
fn the_reference_is_recorded_against_the_prescription() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let (delivered, recorded) = run!(async {
        let prescription = ready.prescriber.prescribe(monday()).await?;
        let delivered = delivering(&ready, &destination).deliver(monday()).await?;

        let store = SqlitePrescriptionDeliveryStore::new(ready.pool.clone());
        let name = DestinationName::try_from("hevy".to_owned())?;
        let recorded = application::PrescriptionDeliveryStore::reference_for(
            &store,
            PrescribedWorkoutId::new(prescription.id.as_i64()),
            &name,
        )
        .await?;

        Ok::<_, Box<dyn std::error::Error>>((delivered, recorded))
    });

    assert_eq!(
        recorded.as_ref(),
        Some(&delivered.reference),
        "what the store holds is what the destination said"
    );
}

/// **What the destination said is kept, on a delivery that worked** (#124).
///
/// The front-squat case. Hevy's create reply carries the whole routine back,
/// exercises included, and it used to be read for an id and dropped — so a
/// routine that arrived on the phone missing an exercise could only be
/// investigated by asking the live API days later. The bytes are now in the
/// store, against the prescription and destination they were answering, which is
/// what makes that question answerable from the record.
#[test]
fn a_successful_delivery_keeps_what_the_destination_answered() {
    let ready = run!(ready());
    let destination = match Counting::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let kept = run!(async {
        let prescription = ready.prescriber.prescribe(monday()).await?;
        delivering(&ready, &destination).deliver(monday()).await?;

        let id = prescription.id.as_i64();
        let rows = sqlx::query!(
            r#"
            SELECT status    AS "status!: String",
                   succeeded AS "succeeded!: i64",
                   body      AS "body!: Vec<u8>"
            FROM delivery_reply
            WHERE prescription = ? AND destination = 'hevy'
            ORDER BY id
            "#,
            id
        )
        .fetch_all(&ready.pool)
        .await?;

        Ok::<_, Box<dyn std::error::Error>>(
            rows.into_iter()
                .map(|row| {
                    (
                        row.status,
                        row.succeeded == 1,
                        String::from_utf8_lossy(&row.body).into_owned(),
                    )
                })
                .collect::<Vec<_>>(),
        )
    });

    assert_eq!(kept.len(), 1, "one act, one answer: {kept:?}");
    let (status, succeeded, body) = &kept[0];
    assert_eq!(status, "201 Created");
    assert!(succeeded);
    assert!(
        body.contains("routine-0"),
        "verbatim, not summarised: {body}"
    );
}

/// **A delivery that failed keeps its reply, and says so** (#124).
///
/// The case that recorded least and mattered most. The store write happens
/// before the error is looked at, so a refusal leaves as much behind as a
/// success — and the message the operator reads carries the body rather than
/// serde's complaint about a column number.
#[test]
fn a_failed_delivery_keeps_its_reply_and_shows_it() {
    let ready = run!(ready());
    let destination = match Refusing::new() {
        Ok(destination) => destination,
        Err(error) => panic!("the fake destination builds: {error}"),
    };

    let (message, kept) = run!(async {
        let prescription = ready.prescriber.prescribe(monday()).await?;

        let refused = Delivering::new(DeliveryPorts {
            prescriptions: SqlitePrescribedWorkoutStore::new(
                ready.pool.clone(),
                "Europe/London".to_owned(),
            ),
            programmes: SqliteGymMesocycleStore::new(ready.pool.clone(), ready.zone.clone()),
            deliveries: SqlitePrescriptionDeliveryStore::new(ready.pool.clone()),
            destination: &destination,
        })
        .deliver(monday())
        .await;

        let id = prescription.id.as_i64();
        let rows = sqlx::query!(
            r#"
            SELECT succeeded AS "succeeded!: i64",
                   body      AS "body!: Vec<u8>"
            FROM delivery_reply
            WHERE prescription = ? AND destination = 'hevy'
            "#,
            id
        )
        .fetch_all(&ready.pool)
        .await?;

        Ok::<_, Box<dyn std::error::Error>>((
            match refused {
                Err(error) => Some(error.to_string()),
                Ok(_) => None,
            },
            rows.into_iter()
                .map(|row| {
                    (
                        row.succeeded == 1,
                        String::from_utf8_lossy(&row.body).into_owned(),
                    )
                })
                .collect::<Vec<_>>(),
        ))
    });

    let message = message.expect("a refused delivery is an error");
    assert!(
        message.contains("exercise_template_id not found"),
        "the operator is shown what was said, not just that something was: {message}"
    );
    assert!(
        message.contains("400 Bad Request"),
        "and how it was said: {message}"
    );
    assert!(
        message.contains("kept"),
        "and that the same reply is retrievable: {message}"
    );

    assert_eq!(kept.len(), 1, "a refusal leaves a row: {kept:?}");
    let (succeeded, body) = &kept[0];
    assert!(!succeeded);
    assert!(body.contains("exercise_template_id not found"), "{body}");

    assert!(
        run!(async {
            let store = SqlitePrescriptionDeliveryStore::new(ready.pool.clone());
            let name = DestinationName::try_from("hevy".to_owned())?;
            Ok::<_, Box<dyn std::error::Error>>(
                application::PrescriptionDeliveryStore::occupying(&store, monday(), &name)
                    .await?
                    .is_none(),
            )
        }),
        "and nothing is recorded as delivered, because nothing was"
    );
}

/// A destination that answers, and refuses.
///
/// Separate from [`Counting`] rather than a mode on it: what it is for is the
/// arm where the act fails *after* the destination has spoken, and a flag would
/// make every test that uses `Counting` read as though it might take that arm.
struct Refusing {
    name: DestinationName,
}

impl Refusing {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            name: DestinationName::try_from("hevy".to_owned())?,
        })
    }

    fn refused() -> DeliveryAttempt {
        match reply(r#"{"error":"exercise_template_id not found"}"#, false) {
            Ok(answer) => DeliveryAttempt::answered(
                answer,
                Err(DeliveryError::Unreachable {
                    destination: "hevy".to_owned(),
                    // The status alone, as the real adapter now words it: the
                    // body is in the reply beside it rather than said twice.
                    message: "400 Bad Request".to_owned(),
                }),
            ),
            Err(error) => DeliveryAttempt::unanswered(error),
        }
    }
}

impl PrescriptionDestination for Refusing {
    fn name(&self) -> &DestinationName {
        &self.name
    }

    async fn deliver(&self, _session: &Deliverable) -> DeliveryAttempt {
        Self::refused()
    }

    async fn replace(
        &self,
        _session: &Deliverable,
        _occupying: &DeliveryReference,
    ) -> DeliveryAttempt {
        Self::refused()
    }
}
