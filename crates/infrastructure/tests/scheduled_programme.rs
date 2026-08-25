//! A programme reads the days it loses from the schedule (roadmap step 2).
//!
//! **The operator states the window; the schedule states the losses.** A block
//! is told when it starts, how long it runs and which slots are the gym's, and
//! reads back which of its days are gone. It is resolved at authoring rather
//! than at derivation, so the stored programme is complete on its own — a
//! holiday coming off the calendar afterwards cannot retroactively move what a
//! prescription said.
//!
//! This mirrors the wiring in `cli::prescribing::add`, the way
//! `standalone_test` mirrors the wiring that resolves a test's inherited fills.

mod support;

use std::{collections::BTreeMap, num::NonZeroU8};

use application::{DiaryAuthor as _, DiaryStore as _};
use domain::{
    gym::OperatorZone,
    prescription::Skip,
    schedule::{Alteration, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
};
use infrastructure::{Document, SqliteDiaryStore, connect};
use jiff::civil::{Weekday, date};
use support::{corpus, programme as fixture};

/// The autumn block from Monday 14 September: nine phase weeks, plus the entry
/// test week in front of them, so ten calendar weeks ending 22 November.
const AUTUMN: &str = r#"
[programme]
name             = "autumn-block"
template         = "block"
primary          = "knee_dominant"
primary_exercise = "front-squat"
gating_role      = "heavy"
start            = "2026-09-14"
duration_weeks   = 9

[programme.weekdays]
monday = "light"
friday = "heavy"

# What the operator expects to lift. Week one finds out; a result that differs is
# answered by re-authoring, which decision 0012 makes a supersession.
[programme.anchor]
load       = "90kg"
provenance = "asserted"
from       = "2026-07-03"

[programme.entry_test]
reps  = 3
light = "60kg"

# A block states every slot itself. Only a test inherits, and only because a test
# is two sessions rather than a programme.
[fills]
knee_dominant                = "front-squat"
upper_push                   = "chest-dip"
upper_pull                   = "neutral-grip-pull-up"
hip_dominant                 = "nordic-hamstrings-curls"
biceps                       = "preacher-curl-barbell"
triceps                      = "overhead-triceps-extension-cable"
wrist_flexion                = "wrist-flexion-dumbbell"
wrist_extension              = "wrist-extension-dumbbell"
core                         = "bent-over-cable-chop"
handstand_hold               = "handstand-hold"
dead_hang                    = "dead-hang"
hip_flexor_stretch           = "couch-stretch"
hip_external_rotator_stretch = "ninety-ninety"
hamstring_stretch            = "standing-straddle-fold"
groin_stretch                = "squatting-groin-stretch"

[fills.plyometric]
exercise = "pogo"
sets     = 3
reps     = 20

[fills.power]
exercise = "box-jump"
sets     = 3
reps     = 5
"#;

macro_rules! zone {
    ($name:literal) => {
        match OperatorZone::try_from($name.to_owned()) {
            Ok(zone) => zone,
            Err(error) => panic!("{} is a zone: {error}", $name),
        }
    };
}

macro_rules! days {
    ($count:literal) => {
        match NonZeroU8::new($count) {
            Some(days) => days,
            None => panic!("{} is not zero", $count),
        }
    };
}

/// Monday and Friday evenings are the gym's; Wednesday and Sunday are not.
fn ordinary() -> BTreeMap<TrainingSlot, Discipline> {
    [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            Discipline::Gym,
        ),
        (
            TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            Discipline::Cycling,
        ),
        (
            TrainingSlot::new(Weekday::Friday, PartOfDay::Evening),
            Discipline::Gym,
        ),
        (
            TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            Discipline::Cycling,
        ),
    ]
    .into_iter()
    .collect()
}

/// What `cli::prescribing::add` does between reading the document and building
/// the programme.
async fn derived(
    document: &Document,
    store: &SqliteDiaryStore,
) -> Result<Vec<Skip>, Box<dyn std::error::Error>> {
    let Some((from, until)) = document.window()? else {
        return Ok(Vec::new());
    };
    Ok(store
        .diary()
        .await?
        .unavailable(from, until, Discipline::Gym)
        .into_iter()
        .map(Skip::day)
        .collect())
}

async fn seeded() -> Result<(SqliteDiaryStore, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let store = SqliteDiaryStore::new(connect(&directory.path().join("test.db")).await?);

    store
        .record_pattern(&TrainingPattern::new(
            date(2026, 8, 24),
            zone!("Europe/London"),
            ordinary(),
        ))
        .await?;
    // The one fact the goal turns on.
    store
        .record_alteration(&Alteration::new(
            date(2026, 9, 14),
            days!(1),
            None,
            Some(BTreeMap::new()),
            "away, and unable to train".to_owned(),
        ))
        .await?;

    Ok((store, directory))
}

/// **The acceptance line for step 2.** Adding the autumn block derives the loss
/// of Monday 14 September without the document stating it.
#[test]
fn the_autumn_block_derives_the_loss_of_the_fourteenth() {
    let outcome = corpus::block_on(async {
        let (store, _directory) = seeded().await?;
        let document: Document = toml::from_str(AUTUMN)?;
        let skips = derived(&document, &store).await?;
        Ok::<_, Box<dyn std::error::Error>>(skips)
    });

    let skips = match outcome {
        Ok(Ok(skips)) => skips,
        Ok(Err(error)) => panic!("the block is authored: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    assert_eq!(
        skips,
        vec![Skip::day(date(2026, 9, 14))],
        "the Monday is lost and nothing else in the ten weeks is"
    );
}

/// The window is the block's own, so nothing outside it is derived — which is
/// also what keeps every derived skip admissible, since `Calendar` refuses an
/// interruption that falls outside the block.
#[test]
fn an_absence_outside_the_window_is_not_the_blocks_business() {
    let outcome = corpus::block_on(async {
        let (store, _directory) = seeded().await?;
        // A fortnight away, well after the ten weeks end on 22 November.
        store
            .record_alteration(&Alteration::new(
                date(2026, 12, 7),
                days!(14),
                None,
                Some(BTreeMap::new()),
                "away in December".to_owned(),
            ))
            .await?;

        let document: Document = toml::from_str(AUTUMN)?;
        let skips = derived(&document, &store).await?;
        Ok::<_, Box<dyn std::error::Error>>(skips)
    });

    let skips = match outcome {
        Ok(Ok(skips)) => skips,
        Ok(Err(error)) => panic!("the block is authored: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    assert_eq!(
        skips,
        vec![Skip::day(date(2026, 9, 14))],
        "December is not this block's problem"
    );
}

/// **A document that states its own interruptions is not asked.** That is the
/// override: the case where the diary has not been told something.
#[test]
fn a_stated_interruption_overrides_the_schedule() {
    let stated = AUTUMN.replace(
        "duration_weeks   = 9",
        "duration_weeks   = 9\ninterruptions    = [\"2026-09-18\"]",
    );

    let outcome = corpus::block_on(async move {
        let (store, _directory) = seeded().await?;
        let document: Document = toml::from_str(&stated)?;
        let skips = derived(&document, &store).await?;
        let parameters = fixture::parameters()?;
        let programme =
            document.programme(&parameters, corpus::zone()?.as_time_zone(), None, &skips)?;
        Ok::<_, Box<dyn std::error::Error>>(programme)
    });

    let programme = match outcome {
        Ok(Ok(programme)) => programme,
        Ok(Err(error)) => panic!("the block is authored: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    let interruptions: Vec<Skip> = programme.calendar().interruptions().iter().collect();
    assert_eq!(
        interruptions,
        vec![Skip::day(date(2026, 9, 18))],
        "the stated Friday, and not the derived Monday"
    );
}

/// **The window covers the entry test week, which is not in `duration_weeks`.**
///
/// A block's `duration_weeks` counts phase weeks and an entry test adds one in
/// front of them (decision 0016), so a nine-week block occupies ten calendar
/// weeks. Asking the schedule over nine would leave the last week unconsulted —
/// and the last week of a block is its exit test.
#[test]
fn the_window_covers_the_entry_test_week() {
    let Ok(document) = toml::from_str::<Document>(AUTUMN) else {
        panic!("the autumn document parses")
    };
    let window = match document.window() {
        Ok(Some(window)) => window,
        Ok(None) => panic!("the autumn document states how long it runs"),
        Err(error) => panic!("the window is readable: {error}"),
    };

    assert_eq!(window.0, date(2026, 9, 14), "the day it starts");
    assert_eq!(
        window.1,
        date(2026, 11, 22),
        "ten calendar weeks, not the nine the document names"
    );
}
