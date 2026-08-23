//! What this adapter sends, and what it does with the answer.
//!
//! Against a mock rather than the live source, so it can assert on the exact
//! bytes of a request — which is the point. The one defect worth fearing here
//! produces a well-formed routine instructing the opposite of what was
//! prescribed, and only the request body shows it.
//!
//! **What no test here can catch is a wrong default**, because every one of them
//! points the adapter at a local stub. The composed URL is pinned by a unit test
//! beside the adapter instead; see `hevy::destination`.

mod support;

use application::{Deliverable, PrescriptionDestination as _};
use domain::{
    gym::{
        Duration, Kg, Load, RepCount, SignedKg,
        exercise::RepsExercise,
        sequence::{AtLeastTwo, NonEmpty},
    },
    prescription::{
        DerivedFrom, PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset,
        PrescribedWorkout, ProgrammeId, SessionOrdinal, SessionRole, SlotId, SupersetMember,
        Target, WeekIndex, WeekKind, WorkoutShape,
    },
};
use infrastructure::HevyRoutines;
use serde_json::Value;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// A dip at seven kilograms of assistance, supersetted with a bodyweight
/// neutral-grip pull-up. The pairing in the next real session, and the two cases
/// that exercise both sides of the load axis.
fn session() -> Result<Deliverable, Box<dyn std::error::Error>> {
    let assisted_dip = PrescribedExercise::ForReps {
        exercise: RepsExercise::ChestDip,
        sets: NonEmpty::new(vec![PrescribedSet::fixed(
            Load::Relative(SignedKg::from_grams(-7_000)),
            Target::spanning(RepCount::new(4)?, RepCount::new(2)?),
        )])?,
    };

    let bodyweight_pull_up = PrescribedExercise::ForReps {
        exercise: RepsExercise::NeutralGripPullUp,
        sets: NonEmpty::new(vec![PrescribedSet::fixed(
            Load::BODYWEIGHT,
            Target::spanning(RepCount::new(4)?, RepCount::new(2)?),
        )])?,
    };

    let shape = WorkoutShape::new(NonEmpty::new(vec![PrescribedItem::Superset(
        PrescribedSuperset {
            members: AtLeastTwo::new(vec![
                SupersetMember {
                    slot: SlotId::UpperPush,
                    exercise: assisted_dip,
                },
                SupersetMember {
                    slot: SlotId::UpperPull,
                    exercise: bodyweight_pull_up,
                },
            ])?,
        },
    )])?);

    let workout = PrescribedWorkout::new(
        shape,
        "2026-08-24".parse()?,
        SessionRole::Light,
        WeekKind::Climbing(WeekIndex::new(4)?),
        DerivedFrom::Anchor(support::programme::anchor()?),
        support::programme::parameters()?,
        "2026-08-01T00:00:00Z".parse()?,
        ProgrammeId::new(1),
        "2026-08-23T18:00:00Z".parse()?,
    );

    Ok(Deliverable {
        workout,
        programme: support::programme::name("summer-2026-front-squat")?,
        ordinal: SessionOrdinal::new(7)?,
    })
}

/// The defect this whole feature has to not have.
///
/// Seven kilograms of *assistance* must reach the source as the assisted
/// template carrying a positive seven — never as the plain template carrying
/// seven, which instructs a dip fourteen kilograms harder than the one
/// prescribed and looks entirely reasonable on the phone.
#[test]
fn assistance_is_sent_as_the_assisted_template() {
    let sent = support::corpus::block_on(async {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/routine_folders"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "routine_folders": [{ "id": 42, "title": "summer-2026-front-squat" }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/routines"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "routine": [{ "id": "b459cba5-cd6d-463c-abd6-54f8eafcadcb" }]
            })))
            .mount(&server)
            .await;

        let hevy =
            HevyRoutines::new(server.uri(), "key").expect("the destination is constructible");
        let delivered = hevy
            .deliver(&session().expect("the fixture session builds"))
            .await
            .expect("the session is delivered");

        let requests = server
            .received_requests()
            .await
            .expect("the mock recorded its requests");
        let posted = requests
            .iter()
            .find(|request| request.url.path() == "/v1/routines")
            .expect("a routine was posted");
        let body: Value = serde_json::from_slice(&posted.body).expect("the request body is json");

        (body, delivered.reference.to_string())
    })
    .expect("the runtime runs");

    let (body, reference) = sent;
    let exercises = body["routine"]["exercises"]
        .as_array()
        .expect("the routine carries exercises");

    let dip = &exercises[0];
    assert_eq!(
        dip["exercise_template_id"], "E9E4089F",
        "an assisted dip goes to the assisted template"
    );
    assert_eq!(
        dip["sets"][0]["weight_kg"], 7,
        "and the assistance is written as a positive number"
    );

    let pull_up = &exercises[1];
    assert_eq!(
        pull_up["exercise_template_id"], "72f032e8-d574-4dab-9bb3-b76377b973f8",
        "a neutral-grip pull-up is its own exercise, not an assisted plain one"
    );
    assert_eq!(
        pull_up["sets"][0]["weight_kg"], 0,
        "bodyweight is zero assistance"
    );

    // The pairing survives, and both members carry the same superset.
    assert_eq!(dip["superset_id"], pull_up["superset_id"]);
    assert!(!dip["superset_id"].is_null(), "a superset is identified");

    // A range crosses as a range rather than as a lie about its low bound.
    assert_eq!(dip["sets"][0]["rep_range"]["start"], 4);
    assert_eq!(dip["sets"][0]["rep_range"]["end"], 6);

    assert_eq!(
        body["routine"]["title"], "07 Light",
        "zero-padded and ordered, short enough to read on a phone"
    );
    assert_eq!(
        body["routine"]["folder_id"], 42,
        "the programme's existing folder is found rather than duplicated"
    );
    assert_eq!(reference, "b459cba5-cd6d-463c-abd6-54f8eafcadcb");
}

/// A programme with no folder yet gets one, and the session goes into it.
#[test]
fn a_programme_without_a_folder_has_one_made_for_it() {
    let folder_id = support::corpus::block_on(async {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/routine_folders"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "routine_folders": []
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/routine_folders"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "routine_folder": { "id": 7, "title": "summer-2026-front-squat" }
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/routines"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "routine": [{ "id": "created" }]
            })))
            .mount(&server)
            .await;

        let hevy =
            HevyRoutines::new(server.uri(), "key").expect("the destination is constructible");
        hevy.deliver(&session().expect("the fixture session builds"))
            .await
            .expect("the session is delivered");

        let requests = server
            .received_requests()
            .await
            .expect("the mock recorded its requests");
        let posted = requests
            .iter()
            .find(|request| request.url.path() == "/v1/routines")
            .expect("a routine was posted");
        let body: Value = serde_json::from_slice(&posted.body).expect("the request body is json");
        body["routine"]["folder_id"].clone()
    })
    .expect("the runtime runs");

    assert_eq!(folder_id, 7);
}

/// § 36: a credential the source refuses degrades the system rather than
/// failing it, and says which system said no.
#[test]
fn a_refused_credential_is_reported_rather_than_swallowed() {
    let message = support::corpus::block_on(async {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/routine_folders"))
            .respond_with(ResponseTemplate::new(401).set_body_string("InvalidApiKey"))
            .mount(&server)
            .await;

        let hevy =
            HevyRoutines::new(server.uri(), "key").expect("the destination is constructible");
        let refused = hevy
            .deliver(&session().expect("the fixture session builds"))
            .await;

        match refused {
            Ok(_) => None,
            Err(error) => Some(error.to_string()),
        }
    })
    .expect("the runtime runs");

    let message = message.expect("a refused credential is an error");
    assert!(message.contains("hevy"), "{message}");
    assert!(message.contains("API key"), "{message}");
}

/// A primary's ramp, rested the way generation rests one: nothing between the
/// warm-ups, the bottom of the range into the working set, and the whole range
/// between working sets.
fn ramped_primary() -> Result<Deliverable, Box<dyn std::error::Error>> {
    let range = Target::between(Duration::from_seconds(120), Duration::from_seconds(180))
        .ok_or("two to three minutes spans")?;
    let into_the_single = Target::Exactly(Duration::from_seconds(120));

    let rested = |mut set: PrescribedSet<RepCount>, rest| {
        set.rest_after = rest;
        set
    };

    let squat = PrescribedExercise::ForReps {
        exercise: RepsExercise::FrontSquat,
        sets: NonEmpty::new(vec![
            rested(
                PrescribedSet::warmup(
                    Load::Absolute(Kg::from_grams(30_000)),
                    Target::Exactly(RepCount::new(4)?),
                ),
                None,
            ),
            rested(
                PrescribedSet::warmup(
                    Load::Absolute(Kg::from_grams(70_000)),
                    Target::Exactly(RepCount::new(1)?),
                ),
                Some(into_the_single),
            ),
            rested(
                PrescribedSet::fixed(
                    Load::Absolute(Kg::from_grams(77_500)),
                    Target::Exactly(RepCount::new(3)?),
                ),
                Some(range),
            ),
        ])?,
    };

    let mut session = session()?;
    session.workout = PrescribedWorkout::new(
        WorkoutShape::new(NonEmpty::new(vec![PrescribedItem::Exercise {
            slot: SlotId::KneeDominant,
            exercise: squat,
        }])?),
        session.workout.issued_for(),
        session.workout.session_role(),
        session.workout.week(),
        session.workout.derived_from(),
        session.workout.parameters().clone(),
        session.workout.parameters_authored_at(),
        session.workout.programme(),
        session.workout.issued_at(),
    );
    Ok(session)
}

/// **The longest rest the exercise instructs, not the first one found.**
///
/// The source holds one rest per exercise where we prescribe one per set, so
/// something has to be chosen. Taking the first put the *warm-up's* number on
/// the front squat — 2:00 where the working sets ask for up to 3:00 — and it
/// looked entirely plausible in the app. Caught by reading a preview rather than
/// by a test, which is what this closes.
#[test]
fn an_exercise_carries_the_longest_rest_it_instructs() {
    let rendered = support::corpus::block_on(async {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/routine_folders"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "routine_folders": [{ "id": 42, "title": "summer-2026-front-squat" }]
            })))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/v1/routines"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "routine": [{ "id": "rested" }]
            })))
            .mount(&server)
            .await;

        let hevy =
            HevyRoutines::new(server.uri(), "key").expect("the destination is constructible");
        hevy.deliver(&ramped_primary().expect("the ramp fixture builds"))
            .await
            .expect("the session is delivered");

        let requests = server
            .received_requests()
            .await
            .expect("the mock recorded its requests");
        let posted = requests
            .iter()
            .find(|request| request.url.path() == "/v1/routines")
            .expect("a routine was posted");
        serde_json::from_slice::<Value>(&posted.body).expect("the request body is json")
    })
    .expect("the runtime runs");

    let squat = &rendered["routine"]["exercises"][0];
    assert_eq!(
        squat["rest_seconds"], 180,
        "the working sets' longest, not the warm-up's 120"
    );
    assert!(
        squat["notes"]
            .as_str()
            .is_some_and(|notes| notes.contains("120s-180s")),
        "and the range the source cannot hold survives in the notes: {}",
        squat["notes"]
    );
}
