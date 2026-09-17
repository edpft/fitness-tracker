//! Withings groups into Body Scan weigh-ins, and the operator's three rulings
//! (2026-09-17, #153). Every payload here is invented; the shapes are the ones
//! the operator's account serves.

use application::{Translation, ports::Translator};
use domain::{
    body::{BodyScanWeighIn, Rhythm},
    landing::{
        Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
        LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{OperatorZone, RefusalReason},
};
use infrastructure::withings::{WithingsWeighInTranslator, group};
use serde_json::{Value, json};

type Failure = Box<dyn std::error::Error>;

const AT: i64 = 1_789_000_000;

fn record(id: i64, payload: &Value) -> Result<LandedRecord, Failure> {
    let provenance = EventProvenance::new(
        Endpoint::try_from("/measure")?,
        EventKind::try_from("updated")?,
        None,
    );
    let landed = LandingRecord::land(
        LandingStream::try_from("withings.measurements")?,
        FetchedAt::try_from("2026-09-17T08:00:00Z")?,
        SourceRecordId::try_from(format!("group-{id}").as_str())?,
        provenance.into(),
        RawPayload::try_from(serde_json::to_vec(payload)?)?,
    );
    Ok(LandedRecord::new(LandingRecordId::try_from(id)?, landed))
}

fn measure(kind: i64, value: i64, unit: i64, position: Option<i64>) -> Value {
    json!({
        "type": kind, "value": value, "unit": unit, "algo": 42, "fm": 3, "position": position,
    })
}

fn group_of(attrib: i64, model: Option<&str>, measures: &[Value]) -> Value {
    json!({
        "grpid": 1, "attrib": attrib, "date": AT, "created": AT, "modified": AT,
        "category": 1, "model": model, "timezone": "Europe/London", "measures": measures,
    })
}

fn composition() -> Value {
    let mut measures = vec![
        measure(1, 84_619, -3, None),
        measure(6, 20_241, -3, None),
        measure(5, 6_700, -2, Some(7)),
        measure(8, 1_760, -2, Some(7)),
        measure(76, 6_350, -2, Some(7)),
        measure(77, 4_800, -2, Some(7)),
        measure(88, 330, -2, Some(7)),
        measure(168, 1_900, -2, Some(7)),
        measure(169, 2_900, -2, Some(7)),
        measure(170, 37, -1, Some(7)),
        measure(226, 1_914, 0, Some(7)),
        measure(227, 42, 0, Some(7)),
    ];
    for position in [2, 3, 10, 11, 12] {
        measures.push(measure(173, 1_000 + position, -2, Some(position)));
        measures.push(measure(174, 200 + position, -2, Some(position)));
        measures.push(measure(175, 900 + position, -2, Some(position)));
    }
    group_of(0, Some("Body Scan"), &measures)
}

fn heart(rhythm: i64) -> Value {
    group_of(
        0,
        Some("Body Scan"),
        &[measure(11, 61, 0, None), measure(130, rhythm, 0, None)],
    )
}

fn nerves() -> Value {
    group_of(
        0,
        Some("Body Scan"),
        &[
            measure(158, 60_000, -3, None),
            measure(159, 61_000, -3, None),
            measure(167, 62_000, -3, None),
        ],
    )
}

fn vascular() -> Value {
    group_of(
        8,
        Some("Body Scan"),
        &[measure(91, 7_100, -3, None), measure(155, 431, -1, None)],
    )
}

fn translate(payloads: &[Value]) -> Result<Translation<BodyScanWeighIn>, Failure> {
    let records = payloads
        .iter()
        .zip(1..)
        .map(|(payload, id)| record(id, payload))
        .collect::<Result<Vec<_>, _>>()?;
    let accounts = group(records);
    let [account] = accounts.as_slice() else {
        return Err(format!("{} weigh-ins, not one", accounts.len()).into());
    };
    Ok(WithingsWeighInTranslator.translate(account, &OperatorZone::try_from("UTC")?)?)
}

fn refused_for(payloads: &[Value]) -> Result<RefusalReason, Failure> {
    match translate(payloads)? {
        Translation::Refused(refusals) => Ok(refusals.first().reason.clone()),
        other => Err(format!("not refused: {other:?}").into()),
    }
}

#[test]
fn every_group_at_one_second_is_one_weigh_in() {
    let translated =
        translate(&[composition(), heart(9), nerves(), vascular()]).expect("the groups translate");
    let Translation::Entity { entity, refusals } = translated else {
        panic!("no weigh-in: {translated:?}");
    };
    assert!(refusals.is_empty(), "{refusals:?}");

    let composition = entity.composition();
    assert_eq!(entity.mass().as_grams(), 84_619);
    assert_eq!(composition.fat_free_mass.as_grams(), 67_000);
    assert_eq!(composition.visceral_fat.as_tenths(), 37);
    assert_eq!(composition.metabolic_age.as_tenths_of_a_year(), 420);
    assert_eq!(
        composition.segments.right_arm.fat_free_mass.as_grams(),
        10_020
    );
    assert_eq!(composition.segments.torso.muscle_mass.as_grams(), 9_120);

    let heart = entity.heart().expect("a heart reading");
    assert_eq!(heart.heart_rate.as_u32(), 61);
    assert_eq!(heart.rhythm, Some(Rhythm::SinusRhythm));
    assert_eq!(
        entity
            .nerves()
            .expect("a nerve reading")
            .both_feet
            .as_nanosiemens(),
        62_000
    );
    assert_eq!(
        entity
            .vascular()
            .expect("a vascular reading")
            .pulse_wave_velocity
            .as_millimetres_per_second(),
        7_100
    );
    // The zone is the one Withings states, not the declared fallback.
    assert_eq!(entity.measured_at().zone().id(), "Europe/London");
    assert_eq!(entity.parts().len(), 4);
}

#[test]
fn a_weigh_in_without_its_optional_parts_still_stands() {
    let translated = translate(&[composition()]).expect("the group translates");
    let Translation::Entity { entity, .. } = translated else {
        panic!("no weigh-in: {translated:?}");
    };
    assert!(entity.heart().is_none());
    assert!(entity.nerves().is_none());
    assert!(entity.vascular().is_none());
}

#[test]
fn a_nerve_reading_alone_is_refused() {
    assert_eq!(
        refused_for(&[nerves()]).expect("a refusal"),
        RefusalReason::WithoutWeighIn { part: "nerve" }
    );
}

#[test]
fn an_unattributed_reading_is_refused() {
    let unattributed = group_of(1, Some("Body Scan"), &[measure(1, 86_600, -3, None)]);
    assert_eq!(
        refused_for(&[unattributed]).expect("a refusal"),
        RefusalReason::Unattributed
    );
}

#[test]
fn a_manual_entry_is_refused() {
    let manual = group_of(2, None, &[measure(1, 88_000, -3, None)]);
    assert!(matches!(
        refused_for(&[manual]).expect("a refusal"),
        RefusalReason::NotTheInstrument { .. }
    ));
}

#[test]
fn an_ecg_code_nobody_has_read_off_the_app_is_refused() {
    assert!(matches!(
        refused_for(&[composition(), heart(1)]).expect("a refusal"),
        RefusalReason::Unmodelled { .. }
    ));
}

#[test]
fn the_other_two_ecg_codes_are_the_ones_the_app_showed() {
    for (code, rhythm) in [(10, Rhythm::HighHeartRate), (5, Rhythm::NotClassified)] {
        let translated = translate(&[composition(), heart(code)]).expect("the groups translate");
        let Translation::Entity { entity, .. } = translated else {
            panic!("no weigh-in: {translated:?}");
        };
        assert_eq!(entity.heart().and_then(|heart| heart.rhythm), Some(rhythm));
    }
}
