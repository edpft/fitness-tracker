//! A programme reads the days it loses from the schedule (roadmap step 2).
//!
//! **The operator states the window; the schedule states the losses.** A block
//! is told when it starts, how long it runs and which slots are the gym's, and
//! reads back which of its days are gone. It is resolved at authoring rather
//! than at derivation, so the stored programme is complete on its own — a
//! holiday coming off the calendar afterwards cannot retroactively move what a
//! prescription said.
//!
//! This mirrors the wiring in `cli::wizard::add`, which is the only authoring
//! path there is.

mod support;

use std::{collections::BTreeMap, num::NonZeroU8};

use application::{DiaryAuthor as _, DiaryStore as _, ExerciseHistory as _};
use domain::{
    gym::OperatorZone,
    prescription::{
        Anchor, AnchorProvenance, Authored, EntryTest, SessionRole, Skip, authored::Shape,
    },
    schedule::{Alteration, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
};
use infrastructure::{SqliteDiaryStore, SqliteExerciseHistory, connect};
use jiff::civil::{Weekday, date};
use support::{corpus, programme as fixture};

/// The autumn block from Monday 14 September: nine phase weeks, plus the entry
/// test week in front of them, so ten calendar weeks ending 22 November.
fn autumn() -> Result<Authored, Box<dyn std::error::Error>> {
    Ok(fixture::authored(
        date(2026, 9, 14),
        Shape::Block {
            gating: SessionRole::Heavy,
            weeks: 9,
            // What the operator expects to lift. Week one finds out; a result
            // that differs is answered by re-authoring, which decision 0012
            // makes a supersession.
            anchor: Anchor::new(
                "90".to_owned().try_into()?,
                None,
                AnchorProvenance::Asserted,
                date(2026, 7, 3),
            )?,
            entry_test: Some(EntryTest::new(
                domain::gym::RepCount::new(3)?,
                Some("60".to_owned().try_into()?),
            )?),
        },
    )?)
}

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

/// What `cli::wizard::add` does between the last question and building the
/// programme.
async fn derived(
    answers: &Authored,
    store: &SqliteDiaryStore,
) -> Result<Vec<Skip>, Box<dyn std::error::Error>> {
    let Some((from, until)) = answers.window() else {
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
        let skips = derived(&autumn()?, &store).await?;
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

        let skips = derived(&autumn()?, &store).await?;
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

/// **The window covers the entry test week, which is not in the block's weeks.**
///
/// A block's stated duration counts phase weeks and an entry test adds one in
/// front of them (decision 0016), so a nine-week block occupies ten calendar
/// weeks. Asking the schedule over nine would leave the last week unconsulted —
/// and the last week of a block is its exit test.
#[test]
fn the_window_covers_the_entry_test_week() {
    let Ok(answers) = autumn() else {
        panic!("the autumn answers build")
    };
    let Some(window) = answers.window() else {
        panic!("the autumn block has a window")
    };

    assert_eq!(window.0, date(2026, 9, 14), "the day it starts");
    assert_eq!(
        window.1,
        date(2026, 11, 22),
        "ten calendar weeks, not the nine the block names"
    );
}

// --- The four scenarios the operator asked to see covered -------------------
//
// Written because they describe things that have happened or will: the
// schedule changes, and it changes at every point in a block's life. What each
// one asserts is the *behaviour*, not a wish about it — where the answer is
// unhelpful the test says so plainly rather than pretending otherwise.

/// **Scenario 1: the happy path.** The schedule removes a day, the programme
/// derives the loss, and asking for that day is refused by name.
///
/// The two halves are tested apart — derivation above, refusal in
/// `calendar` — and this is the join: the day the diary took out is the day the
/// calendar will not place.
#[test]
fn the_day_the_schedule_removed_is_refused_by_the_programme() {
    let outcome = corpus::block_on(async {
        let (store, _directory) = seeded().await?;
        let skips = derived(&autumn()?, &store).await?;
        let programme = fixture::authoring(autumn()?, &skips)??;
        Ok::<_, Box<dyn std::error::Error>>(programme)
    });

    let programme = match outcome {
        Ok(Ok(programme)) => programme,
        Ok(Err(error)) => panic!("the block is authored: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    let refusal = programme.calendar().place(date(2026, 9, 14));
    assert!(
        matches!(
            refusal,
            Err(domain::prescription::NotScheduled::Interrupted { .. })
        ),
        "the 14th is refused as interrupted, got {refusal:?}"
    );

    // And the Friday of that week is untouched: one day went, not the week.
    assert!(
        programme.calendar().place(date(2026, 9, 18)).is_ok(),
        "the Friday still runs"
    );
}

/// **Scenario 2: the schedule changes before the programme starts.**
///
/// Re-authoring reads the diary as it stands, so the change is picked up in
/// full. Nothing has been issued yet, so there is nothing stale behind it.
///
/// **What this does not do is notice on its own.** Nothing watches the diary;
/// the operator re-runs `programme add`, and re-running it under the same name
/// is a supersession rather than a second programme (decision 0012).
#[test]
fn a_schedule_changed_before_the_start_is_picked_up_by_re_authoring() {
    let outcome = corpus::block_on(async {
        let directory = tempfile::tempdir()?;
        let store = SqliteDiaryStore::new(connect(&directory.path().join("test.db")).await?);
        store
            .record_pattern(&TrainingPattern::new(
                date(2026, 8, 24),
                zone!("Europe/London"),
                ordinary(),
            ))
            .await?;

        let before = derived(&autumn()?, &store).await?;

        // The operator books something for the Friday of week two.
        store
            .record_alteration(&Alteration::new(
                date(2026, 9, 25),
                days!(1),
                None,
                Some(BTreeMap::new()),
                "a wedding".to_owned(),
            ))
            .await?;
        let after = derived(&autumn()?, &store).await?;

        Ok::<_, Box<dyn std::error::Error>>((before, after, directory))
    });

    let (before, after, _directory) = match outcome {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => panic!("the diary answers: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    assert_eq!(
        before,
        Vec::new(),
        "nothing is lost before anything is booked"
    );
    assert_eq!(
        after,
        vec![Skip::day(date(2026, 9, 25))],
        "re-authoring reads the diary as it now stands"
    );
}

/// **Scenario 3: the schedule changes after the programme has started.**
///
/// The authored programme does not move. Its interruptions were resolved when
/// it was authored and are stored on it, so a diary change afterwards leaves
/// every session it has already placed exactly where it was — which is what
/// makes a prescription issued last week reproducible this week.
///
/// **Adopting the change is therefore a deliberate act**: re-author, and reissue
/// the dates already prescribed (`asking_twice_issues_once` pins that asking
/// again does *not* re-derive on its own). Anything already delivered stays in
/// the source, because a routine is created and never updated (decision 0017).
#[test]
fn a_schedule_changed_after_authoring_does_not_move_what_was_authored() {
    let outcome = corpus::block_on(async {
        let (store, _directory) = seeded().await?;
        // Authored while the diary says only the 14th is gone.
        let skips = derived(&autumn()?, &store).await?;
        let authored = fixture::authoring(autumn()?, &skips)??;

        // Afterwards, another Monday goes.
        store
            .record_alteration(&Alteration::new(
                date(2026, 9, 21),
                days!(1),
                None,
                Some(BTreeMap::new()),
                "called away".to_owned(),
            ))
            .await?;

        let now = derived(&autumn()?, &store).await?;
        Ok::<_, Box<dyn std::error::Error>>((authored, now))
    });

    let (authored, now) = match outcome {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => panic!("the block is authored: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    let stored: Vec<Skip> = authored.calendar().interruptions().iter().collect();
    assert_eq!(
        stored,
        vec![Skip::day(date(2026, 9, 14))],
        "the authored block still holds what the diary said when it was authored"
    );
    assert!(
        authored.calendar().place(date(2026, 9, 21)).is_ok(),
        "and still places the Monday that has since gone"
    );

    // The diary knows. Nothing has asked it since.
    assert_eq!(
        now,
        vec![Skip::day(date(2026, 9, 14)), Skip::day(date(2026, 9, 21))],
        "re-authoring is what adopts it"
    );
}

/// **Scenario 4: the session happened on a day the schedule says was gone.**
///
/// The operator trains, and only afterwards records the alteration — a Sunday
/// ride done on the Saturday, a session squeezed in on a day he had written off.
/// The order is the wrong way round and it does not matter.
///
/// **The schedule and the programme decide what is *prescribed*. They never
/// decide what *counts*.** Progression reads the performed record joined on the
/// exercise and nothing else — not the date, not the programme, not the diary —
/// so a session performed on a day the diary calls unavailable advances the
/// ladder exactly as one on a programmed day would.
///
/// That is the § II line holding: the record is an observation, and an
/// observation is not made false by a plan that disagrees with it.
///
/// **What nothing does is reconcile the two.** `domain::prescription::project`
/// can compare a performed workout against a prescribed one and no live path
/// calls it, so there is no notion of a prescription being missed, met, or met
/// on another day. The prescription issued for the day he skipped simply stays
/// issued and unperformed. That is a gap, recorded here rather than implied.
#[test]
fn a_session_performed_on_an_unavailable_day_still_counts() {
    // A real front squat in the corpus: Monday 10 August 2026.
    let performed_on = date(2026, 8, 10);

    let outcome = corpus::block_on(async move {
        let (directory, pool) = support::store::derived_and_authored().await?;

        // The alteration is recorded *after* the session was performed, which
        // is the scenario: the operator writes down what happened once it has.
        let diary = SqliteDiaryStore::new(pool.clone());
        diary
            .record_pattern(&TrainingPattern::new(
                date(2026, 1, 1),
                zone!("Europe/London"),
                ordinary(),
            ))
            .await?;
        diary
            .record_alteration(&Alteration::new(
                performed_on,
                days!(1),
                None,
                Some(BTreeMap::new()),
                "written off, and then trained anyway".to_owned(),
            ))
            .await?;

        let history = SqliteExerciseHistory::new(pool.clone());
        let performances = history
            .performances(domain::gym::exercise::RepsExercise::FrontSquat)
            .await?;
        let lost = diary
            .diary()
            .await?
            .unavailable(performed_on, performed_on, Discipline::Gym);

        Ok::<_, Box<dyn std::error::Error>>((performances, lost, directory))
    });

    let (performances, lost, _directory) = match outcome {
        Ok(Ok(values)) => values,
        Ok(Err(error)) => panic!("the corpus lands and derives: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };

    assert_eq!(
        lost,
        vec![performed_on],
        "the diary says the gym lost that Monday"
    );
    assert!(
        performances
            .iter()
            .any(|performance| performance.on == performed_on),
        "and the front squat performed on it is in the history all the same"
    );
}
