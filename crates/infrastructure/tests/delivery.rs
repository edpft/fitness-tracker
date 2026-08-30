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
    Deliverable, Delivered, DeliveryError, DeliveryReference, DestinationName, Issuance,
    PrescribedWorkoutId, PrescriptionDeliverer as _, PrescriptionDestination, ProgrammeAuthor as _,
    WorkoutPrescriber as _,
    deliver::{Delivering, DeliveryPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use infrastructure::{
    HevyWorkoutLandingReader, HevyWorkoutLandingStore, HevyWorkoutTranslator,
    SqliteExerciseHistory, SqliteExtractionRunLog, SqliteGenerationParameterStore,
    SqliteGymWorkoutStore, SqliteNormalisationRunLog, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore, SqliteRefusalStore, connect,
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

    async fn deliver(&self, session: &Deliverable) -> Result<Delivered, DeliveryError> {
        let seen = self.calls.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut titles) = self.titles.lock() {
            titles.push(format!(
                "{:02} {}",
                session.ordinal.as_u32(),
                session.workout.session_role()
            ));
        }

        DeliveryReference::try_from(format!("routine-{seen}"))
            .map(|reference| Delivered {
                reference,
                unexpressed: Vec::new(),
            })
            .map_err(|error| DeliveryError::Unidentifiable {
                destination: "hevy".to_owned(),
                message: error.to_string(),
            })
    }

    /// **Keeps the reference it was given**, which is what a real `PUT` does and
    /// is the whole property under test: the operator's routine changes
    /// contents without changing identity.
    async fn replace(
        &self,
        session: &Deliverable,
        occupying: &DeliveryReference,
    ) -> Result<Delivered, DeliveryError> {
        self.replacements.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut titles) = self.titles.lock() {
            titles.push(format!(
                "{:02} {} (replaced)",
                session.ordinal.as_u32(),
                session.workout.session_role()
            ));
        }
        Ok(Delivered {
            reference: occupying.clone(),
            unexpressed: Vec::new(),
        })
    }
}

type Prescriber = Prescribing<
    SqliteExerciseHistory,
    SqliteProgrammeStore,
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
    zone: domain::gym::OperatorZone,
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
            raw: HevyWorkoutLandingReader::new(pool.clone())?,
            translator: HevyWorkoutTranslator,
            workouts: SqliteGymWorkoutStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone())?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        corpus::zone()?,
    );
    application::WorkoutNormaliser::normalise(&normalisation).await?;

    Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(
        &programme::as_programme(programme::programme()?),
        &programme::parameters()?,
    )
    .await?;

    Ok(Ready {
        prescriber: Prescribing::new(PrescriptionPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            programmes: SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
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
    SqliteProgrammeStore,
    SqlitePrescriptionDeliveryStore,
    &'a Counting,
> {
    Delivering::new(DeliveryPorts {
        prescriptions: SqlitePrescribedWorkoutStore::new(
            ready.pool.clone(),
            "Europe/London".to_owned(),
        ),
        programmes: SqliteProgrammeStore::new(ready.pool.clone(), ready.zone.clone()),
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
            SqliteProgrammeStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGenerationParameterStore::new(ready.pool.clone()),
        )
        .author(
            &programme::as_programme(programme::programme_from(Date::constant(2026, 7, 20))?),
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
            SqliteProgrammeStore::new(ready.pool.clone(), ready.zone.clone()),
            SqliteGenerationParameterStore::new(ready.pool.clone()),
        )
        .author(
            &programme::as_programme(programme::programme_from(Date::constant(2026, 7, 20))?),
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
