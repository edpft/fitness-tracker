//! **A run straight after a run does nothing** (#830), end to end: the run a
//! page starts, over this build's adapters and a real store, against a stub
//! Hevy that lands a workout and takes a delivery.
//!
//! On 2026-11-17 the first page opened delivered a session, and every page
//! opened in the next four minutes started a run that fetched again. Each of
//! #830's earlier checks ran on a copy of the store with no credentials, where
//! nothing is fetched or delivered, so none of them could see it.
//!
//! **The run goes in a child process**: a source's root is read from the
//! environment, as it is for the installed build, and the environment is set
//! where the child is started rather than in this process.

use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Output},
};

use application::{
    DiaryAuthor as _, EquipmentStore as _, PlanAuthor as _, ProgressionStore as _,
    WorkoutStore as _,
    next::{Found, GymPrescribed, Prescribed, Report, Scope, StepFailure, WhatIsNext},
    ports::Placed,
    prescribe::Authoring,
};
use domain::{
    gym::exercise::{DurationExercise, Exercise, Implement, RepsExercise},
    measure::{Kg, RepCount},
    plan::{Mesocycles, Plan, PlanName},
    planner::{Relative, SessionRole, TrainingWeek},
    prescription::{
        Anchor, AnchorProvenance, ByIntensity, GymMesocycle, LoadSteps, Skip, Test, Tested,
        linear::{Fill, PrimaryPattern, SlotFills, StaticFill},
    },
    schedule::{DayPart, PartOfDay, SlotLength, TrainingPattern, TrainingSlot},
};
use jiff::civil::{Date, Weekday};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const ZONE: &str = "Europe/London";

/// The Monday the plan's week begins, and the afternoon both runs are asked
/// at: the gym's session that evening is what is next.
const MONDAY: Date = Date::constant(2026, 11, 16);

/// What the child is asked to do, and where the store is.
const ACT: &str = "RUN_AFTER_RUN_ACT";
const STORE: &str = "RUN_AFTER_RUN_STORE";

const CHILD: &str = "run_after_run::the_run_in_a_child";

/// **A run straight after a run does nothing**: the second fetches nothing,
/// derives nothing and delivers nothing, and a page opened after the first
/// starts no run at all.
#[test]
fn a_run_straight_after_a_run_does_nothing() {
    let directory = tempfile::TempDir::new().expect("a temporary directory");
    let store = directory.path().join("fitness.db");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let hevy = runtime.block_on(hevy()).expect("the stub answers");
    let calendars = runtime.block_on(calendars()).expect("the stub answers");
    runtime.block_on(seed(&store)).expect("the store is seeded");

    let first = child(&store, &hevy.uri(), &calendars.uri(), "first").expect("the child starts");
    assert!(first.status.success(), "the first run: {}", said(&first));
    let after_first = runtime.block_on(asked(&hevy));
    assert!(
        after_first
            .iter()
            .any(|line| line == "GET /v1/workouts/events"),
        "the first run fetched Hevy: {after_first:#?}"
    );
    assert!(
        after_first.iter().any(|line| line == "POST /v1/routines"),
        "the first run delivered: {after_first:#?}\n{}",
        said(&first)
    );

    let again = child(&store, &hevy.uri(), &calendars.uri(), "again").expect("the child starts");
    assert!(again.status.success(), "the second run: {}", said(&again));
    let after_again = runtime.block_on(asked(&hevy));
    assert_eq!(
        after_again[after_first.len()..],
        [] as [String; 0],
        "the second run asked Hevy nothing"
    );
    let calendars_asked = runtime.block_on(asked(&calendars));
    assert_eq!(
        calendars_asked.len(),
        2,
        "each calendar was asked once, by the first run: {calendars_asked:#?}"
    );
}

/// The run, as the child the test starts: `first` runs once, and `again`
/// asks whether a run is wanted and then runs anyway, asserting that it
/// found nothing to do.
///
/// **A source with no access is nothing to do**: it is asked nothing.
///
/// Ignored, so it runs only where the test above starts it.
#[test]
#[ignore = "started by a_run_straight_after_a_run_does_nothing, in the stub's environment"]
fn the_run_in_a_child() {
    let act = std::env::var(ACT).expect("the act is named");
    let store = std::env::var(STORE).expect("the store is named");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let (wanted, report) = runtime
        .block_on(run(Path::new(&store), act == "again"))
        .expect("the run answers");
    println!("wanted by a page, and Hevy by the terminal: {wanted:?}");
    for done in &report.run {
        println!("{:?}: {:?}", done.step, done.outcome);
    }
    println!("next: {:#?}", report.next);

    if act == "again" {
        assert_eq!(
            wanted,
            Some((false, false)),
            "a page opened now starts no run, and the terminal would not fetch Hevy \
             for a session delivered moments ago"
        );
        let did: Vec<String> = report
            .run
            .iter()
            .filter(|done| {
                !matches!(
                    done.outcome,
                    Ok(Found::NotDue(_) | Found::Unchanged) | Err(StepFailure::NoAccess(_))
                )
            })
            .map(|done| format!("{:?}: {:?}", done.step, done.outcome))
            .collect();
        assert_eq!(did, [] as [String; 0], "every step found nothing to do");
        assert!(
            matches!(
                &report.next,
                WhatIsNext::Session {
                    prescribed: Prescribed::Gym(Ok(GymPrescribed::Issued {
                        delivery: Ok(delivery),
                        ..
                    })),
                    ..
                } if delivery.placed == Placed::Unchanged
            ),
            "nothing was delivered again"
        );
    }
}

/// Whether a page would want a run, and whether the terminal would fetch
/// Hevy, asked only before the second; then the run a page starts.
///
/// **The terminal, because it fetches whenever a stream is due**: a page
/// fetches nothing fetched within the shortest visit, which would hide
/// whether a session just delivered is due.
async fn run(store: &Path, ask_first: bool) -> Fallible<(Option<(bool, bool)>, Report)> {
    let store = configurator::store::Store::open(store).await?;
    let zone = domain::normalised::OperatorZone::try_from(ZONE)?;
    let held = configurator::next::Afresh {
        store: &store,
        zone: &zone,
    };
    let credentials = infrastructure::Credentials::default();
    let now = DayPart::new(MONDAY, PartOfDay::Afternoon);
    let wanted = if ask_first {
        let hevy = Scope::new([application::Part::Fetching(
            domain::landing::LandingStream::try_from("hevy.workouts")?,
        )]);
        Some((
            configurator::next::wanted(
                &held,
                &credentials,
                now,
                None,
                configurator::telemetry::Frontend::Web,
            )
            .await?,
            configurator::next::wanted(
                &held,
                &credentials,
                now,
                Some(&hevy),
                configurator::telemetry::Frontend::Cli,
            )
            .await?,
        ))
    } else {
        None
    };
    let report = configurator::next::run(
        &held,
        &credentials,
        now,
        None,
        configurator::telemetry::Run::everything(configurator::telemetry::Frontend::Web),
        &Silent,
    )
    .await?;
    Ok((wanted, report))
}

/// Nothing to report to.
struct Silent;

impl application::next::Progress for Silent {
    fn started(&self, _step: &application::next::Step) {}
    fn estimated(
        &self,
        _step: &application::next::Step,
        _outstanding: u64,
        _takes: std::time::Duration,
    ) {
    }
    fn finished(&self, _done: &application::next::Done) {}
    fn collected(&self, _parts: &[application::Part]) {}
}

/// The child, with Hevy at `hevy`, both calendars at `calendars`, and
/// nothing else to reach.
fn child(store: &Path, hevy: &str, calendars: &str, act: &str) -> std::io::Result<Output> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([CHILD, "--exact", "--ignored", "--nocapture"])
        .env_clear()
        .env(ACT, act)
        .env(STORE, store)
        .env("HEVY_API_KEY", "key")
        .env("HEVY_API_BASE_URL", hevy)
        .env("SCHOOL_CALENDAR_URL", format!("{calendars}/school.ics"))
        .env(
            "PUBLIC_CALENDAR_URL",
            format!("{calendars}/bank-holidays.json"),
        );
    // The TLS backend initialises from it, though nothing here is TLS.
    if let Ok(certificates) = std::env::var("SSL_CERT_FILE") {
        command.env("SSL_CERT_FILE", certificates);
    }
    command.output()
}

/// What a child printed, for a failure to show.
fn said(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Each request the stub has answered, as `METHOD /path`.
async fn asked(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

/// A Hevy holding one workout from the week before, and taking whatever
/// routine it is sent.
async fn hevy() -> Fallible<MockServer> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/workouts/events"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "page": 1,
            "page_count": 1,
            "events": [{ "type": "updated", "workout": workout() }],
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/routine_folders"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({ "routine_folders": [] })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/routine_folders"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "routine_folder": { "id": 7, "title": "fixture" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/routines"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "routine": { "id": "b459cba5-cd6d-463c-abd6-54f8eafcadcb" }
        })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex("^/v1/routines/.+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "routine": { "id": "b459cba5-cd6d-463c-abd6-54f8eafcadcb" }
        })))
        .mount(&server)
        .await;
    Ok(server)
}

/// Both calendars: the school's Christmas holidays and Christmas Day.
async fn calendars() -> Fallible<MockServer> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/school.ics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261221\r\n\
             DTEND;VALUE=DATE:20270105\r\nSUMMARY:Christmas holidays\r\nEND:VEVENT\r\n\
             END:VCALENDAR\r\n",
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/bank-holidays.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "england-and-wales": {
                "division": "england-and-wales",
                "events": [{
                    "title": "Christmas Day", "date": "2026-12-25",
                    "notes": "", "bunting": true,
                }],
            },
        })))
        .mount(&server)
        .await;
    Ok(server)
}

/// A front squat on the Monday before.
fn workout() -> serde_json::Value {
    let set = |index: u32| {
        serde_json::json!({
            "index": index, "type": "normal", "weight_kg": 60, "reps": 5,
            "distance_meters": null, "duration_seconds": null, "rpe": null,
            "custom_metric": null,
        })
    };
    serde_json::json!({
        "id": "0eedcd17-f7ec-44a2-b2a6-3e6d4230151f",
        "title": "Light",
        "routine_id": null,
        "description": "",
        "start_time": "2026-11-09T18:00:00+00:00",
        "end_time": "2026-11-09T19:00:00+00:00",
        "updated_at": "2026-11-09T19:00:05.000Z",
        "created_at": "2026-11-09T19:00:05.000Z",
        "exercises": [{
            "index": 0, "title": "Front Squat", "notes": "",
            "exercise_template_id": "5046D0A9", "superset_id": null,
            "sets": [set(0), set(1)],
        }],
    })
}

/// A store as the wizards leave it: the zone, the week, the versions and the
/// equipment, and a plan whose gym week is an entry test.
async fn seed(path: &Path) -> Fallible<()> {
    let store = configurator::store::Store::open(path).await?;
    store.state_timezone(jiff::Timestamp::now(), ZONE).await?;
    let zone = domain::normalised::OperatorZone::try_from(ZONE)?;

    let evening = |day| TrainingSlot::new(day, PartOfDay::Evening);
    store
        .diary()
        .record_pattern(&TrainingPattern::new(
            Date::constant(2026, 1, 1),
            zone.clone(),
            BTreeMap::from([
                (evening(Weekday::Monday), SlotLength::Shorter),
                (evening(Weekday::Friday), SlotLength::Shorter),
            ]),
        ))
        .await?;

    let at = jiff::Timestamp::UNIX_EPOCH;
    let versions = store.parameters();
    let seed = domain::prescription::seed::workout()?;
    versions.author_workout(Relative::Higher, at, &seed).await?;
    versions.author_workout(Relative::Lower, at, &seed).await?;
    versions
        .author_linear(at, &domain::prescription::seed::linear()?)
        .await?;
    versions
        .author_sbs(at, &domain::prescription::seed::sbs()?)
        .await?;
    let equipment = store.equipment();
    for implement in [
        Implement::Barbell,
        Implement::Cable,
        Implement::Machine,
        Implement::Dumbbell,
    ] {
        equipment
            .record(
                &domain::gym::equipment::Statement::Kind(
                    implement,
                    LoadSteps::uniform(Kg::try_from("2.5".to_owned())?)?,
                ),
                None,
                at,
                None,
            )
            .await?;
    }

    let plan = Plan::new(
        PlanName::try_from("fixture".to_owned())?,
        jiff::Timestamp::now(),
        Some(Mesocycles::new(vec![entry_test(
            jiff::tz::TimeZone::get(ZONE)?,
        )?])?),
        None,
    )?;
    Authoring::new(store.plans(&zone)).author(&plan).await?;
    store.close().await;
    Ok(())
}

/// The week from [`MONDAY`] as a 1RM test of the front squat, Monday easier
/// and Friday harder, opening from an asserted 90 kg.
fn entry_test(zone: jiff::tz::TimeZone) -> Fallible<GymMesocycle> {
    let week = TrainingWeek::new(vec![
        (
            Weekday::Monday,
            SessionRole::new(Relative::Lower, Relative::Higher),
        ),
        (
            Weekday::Friday,
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
    ])?;
    Ok(GymMesocycle::Test(Test::new(
        Tested::new(
            PrimaryPattern::KneeDominant,
            Exercise::Reps(RepsExercise::FrontSquatBarbell),
            RepCount::new(1)?,
        ),
        fills()?,
        Test::week(MONDAY, &[] as &[Skip], week, zone)?,
        None,
        Some(Anchor::new(
            Kg::try_from("90".to_owned())?,
            Some(Kg::try_from("95".to_owned())?),
            AnchorProvenance::Tested,
            Date::constant(2026, 7, 3),
        )?),
    )?))
}

/// What fills each slot, as the operator's record has them.
fn fills() -> Fallible<SlotFills> {
    let (three, five, twenty) = (RepCount::new(3)?, RepCount::new(5)?, RepCount::new(20)?);
    let same = |exercise| Fill::Same(Exercise::Reps(exercise));
    let hold = |exercise| Fill::Same(Exercise::Duration(exercise));
    Ok(SlotFills {
        plyometric: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::Pogo),
            sets: three,
            reps: twenty,
        }),
        power: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::BoxJump),
            sets: three,
            reps: five,
        }),
        knee_dominant: same(RepsExercise::FrontSquatBarbell),
        upper_push: same(RepsExercise::ChestDip),
        upper_pull: same(RepsExercise::NeutralGripPullUp),
        hip_dominant: Fill::Alternating(ByIntensity {
            lower: Exercise::Reps(RepsExercise::BackExtensionMachine),
            higher: Exercise::Reps(RepsExercise::NordicHamstringsCurls),
        }),
        biceps: same(RepsExercise::PreacherCurlBarbell),
        triceps: same(RepsExercise::OverheadTricepsExtensionCable),
        wrist_flexion: same(RepsExercise::WristFlexionDumbbell),
        wrist_extension: same(RepsExercise::WristExtensionDumbbell),
        core: same(RepsExercise::BentOverCableChop),
        handstand_hold: hold(DurationExercise::HandstandHold),
        dead_hang: hold(DurationExercise::DeadHang),
        hip_flexor_stretch: hold(DurationExercise::CouchStretch),
        hip_external_rotator_stretch: hold(DurationExercise::NinetyNinety),
        hamstring_stretch: hold(DurationExercise::StandingStraddleFold),
        groin_stretch: hold(DurationExercise::SquattingGroinStretch),
    })
}
