//! Which programmes may follow which, and what a block may open from.
//!
//! **The operator's ten rows, asserted rather than described** (decisions 0013
//! and 0016). Six are about what the programme before produced; four are about
//! how long ago it produced it. They are one rule between them:
//!
//! > A block that says it opens from an earlier test must be right about that:
//! > the test must have happened, in the same lift, recently enough to still
//! > speak for it.
//!
//! **It refuses a claim, not a choice.** A block's anchor is a previous test, an
//! entry test of its own, or a declared number, and the operator picks. Only the
//! first says anything about the past, so only the first can be wrong.
//!
//! **At the adapter's ring, because the rule reads the store.** Whether a
//! maximum exists is a fact about what came before, not a claim a document can
//! settle about itself — so this is the one check in the system that refuses one
//! programme by reading another, and it cannot be driven without a real one.
//!
//! Every predecessor below ends on Sunday 20 September and takes its test, where
//! it takes one, on Friday 18 September. What varies is the lift, the template,
//! and how many blank weeks sit between it and the block that follows.

mod support;

use application::{Authored, PrescriptionError, ProgrammeAuthor as _, prescribe::Authoring};
use domain::{
    gym::{
        Kg, RepCount,
        exercise::{Exercise, RepsExercise},
    },
    prescription::{
        Anchor, AnchorProvenance, Calendar, Entry, EntryTest, Fill, GenerationParameters, Linear,
        Periodisation, Periodised, Primary, PrimaryPattern, Programme, ProgrammeName, SessionRole,
        Skip, SlotFills, Test, TestTarget, Tested, Weekdays,
    },
};
use infrastructure::{SqliteGenerationParameterStore, SqliteProgrammeStore, connect};
use jiff::civil::Date;
use sqlx::SqlitePool;
use support::{corpus, programme};

/// The lift a block is about to train. "Exercise b" in the operator's table.
const B: RepsExercise = RepsExercise::FrontSquat;
/// Any other lift. "Exercise a".
const A: RepsExercise = RepsExercise::SquatBarbell;

/// The day every predecessor takes its test, where it takes one.
const TESTED_ON: Date = Date::constant(2026, 9, 18);
/// The Monday after every predecessor ends. A block starting here is adjacent.
const ADJACENT: Date = Date::constant(2026, 9, 21);

/// What the programme before the block was.
#[derive(Debug, Clone, Copy)]
enum Before {
    Test(RepsExercise),
    Linear(RepsExercise),
    Block(RepsExercise),
}

impl Before {
    /// A label for the assertion message, so a failure names the row.
    fn label(self) -> String {
        let (kind, lift) = match self {
            Self::Test(lift) => ("test", lift),
            Self::Linear(lift) => ("linear", lift),
            Self::Block(lift) => ("block", lift),
        };
        let lift = if lift == B { "b" } else { "a" };
        format!("{kind} for exercise {lift}")
    }
}

fn name(value: &str) -> Result<ProgrammeName, Box<dyn std::error::Error>> {
    Ok(ProgrammeName::try_from(value.to_owned())?)
}

fn weekdays() -> Result<Weekdays, Box<dyn std::error::Error>> {
    Ok(Weekdays::new(vec![
        (jiff::civil::Weekday::Monday, SessionRole::Light),
        (jiff::civil::Weekday::Friday, SessionRole::Heavy),
    ])?)
}

/// Every slot filled, with the knee-dominant one taking the lift under test.
///
/// Both lifts are knee-dominant, so a change of lift here is a change of one
/// fill and not of the template's arrangement — which keeps these rows about
/// what they are about.
fn fills(lift: RepsExercise) -> Result<SlotFills, Box<dyn std::error::Error>> {
    Ok(SlotFills {
        knee_dominant: Fill::Same(Exercise::Reps(lift)),
        ..programme::fills()?
    })
}

fn anchor(provenance: AnchorProvenance) -> Result<Anchor, Box<dyn std::error::Error>> {
    Ok(Anchor::new(
        Kg::try_from("90".to_owned())?,
        None,
        provenance,
        TESTED_ON,
    )?)
}

const fn primary(lift: RepsExercise) -> Primary {
    Primary::new(
        PrimaryPattern::KneeDominant,
        Exercise::Reps(lift),
        SessionRole::Heavy,
    )
}

/// The programme before, ending Sunday 20 September.
fn predecessor(
    before: Before,
    parameters: &GenerationParameters,
) -> Result<Programme, Box<dyn std::error::Error>> {
    Ok(match before {
        // One week, 14 to 20 September, testing on the Friday.
        Before::Test(lift) => Programme::Test(Test::new(
            name("before")?,
            Tested::new(
                PrimaryPattern::KneeDominant,
                Exercise::Reps(lift),
                RepCount::new(1)?,
            ),
            fills(lift)?,
            Test::week(
                Date::constant(2026, 9, 14),
                &[] as &[Skip],
                weekdays()?,
                corpus::zone()?.as_time_zone(),
            )?,
            TestTarget::Declared(Kg::try_from("90".to_owned())?),
        )?),
        // Three weeks, 31 August to 20 September. It tests nothing, ever.
        Before::Linear(lift) => Programme::Periodisation(Periodisation::Linear(Linear::new(
            name("before")?,
            primary(lift),
            fills(lift)?,
            // Its own opening, from before its own start. What matters for
            // these rows is that a linear programme leaves no maximum behind
            // it, whatever it opened from.
            Entry::declaring(
                Anchor::new(
                    Kg::try_from("80".to_owned())?,
                    None,
                    AnchorProvenance::Asserted,
                    Date::constant(2026, 8, 21),
                )?,
                Kg::try_from("80".to_owned())?,
            ),
            Calendar::new(
                Date::constant(2026, 8, 31),
                3,
                &[] as &[Skip],
                weekdays()?,
                corpus::zone()?.as_time_zone(),
            )?,
            parameters,
        )?)),
        // Eight phase weeks and an entry test in front, 20 July to 20 September.
        // Its exit test is the last of them, on Friday 18 September.
        Before::Block(lift) => {
            Programme::Periodisation(Periodisation::Block(Periodised::new(
                name("before")?,
                primary(lift),
                fills(lift)?,
                // Dated before its own start, since it tests its own entry and
                // this number is only what it expected.
                Entry::derived(Anchor::new(
                    Kg::try_from("85".to_owned())?,
                    None,
                    AnchorProvenance::Asserted,
                    Date::constant(2026, 7, 17),
                )?),
                Some(EntryTest::new(RepCount::new(3)?, None)?),
                Periodised::weeks(
                    Date::constant(2026, 7, 20),
                    8,
                    true,
                    &[] as &[Skip],
                    weekdays()?,
                    corpus::zone()?.as_time_zone(),
                )?,
            )?))
        }
    })
}

/// A block for exercise b with no entry test, starting `gap` blank weeks after
/// the predecessor ends, and saying how it arrived at its anchor.
fn block_opening_from(
    gap: i64,
    provenance: AnchorProvenance,
) -> Result<Programme, Box<dyn std::error::Error>> {
    let start = ADJACENT.checked_add(jiff::Span::new().days(gap * 7))?;
    Ok(Programme::Periodisation(Periodisation::Block(
        Periodised::new(
            name("under-test")?,
            primary(B),
            fills(B)?,
            // Dated to the predecessor's test day, and labelled as measured —
            // which is what the operator writes when opening from one.
            Entry::derived(anchor(provenance)?),
            None,
            Periodised::weeks(
                start,
                8,
                false,
                &[] as &[Skip],
                weekdays()?,
                corpus::zone()?.as_time_zone(),
            )?,
        )?,
    )))
}

/// Author the predecessor and then the block, and report whether the block was
/// accepted.
async fn composes(before: Before, gap: i64) -> Result<bool, Box<dyn std::error::Error>> {
    composes_with(before, gap, AnchorProvenance::Tested).await
}

async fn composes_with(
    before: Before,
    gap: i64,
    provenance: AnchorProvenance,
) -> Result<bool, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;
    let parameters = programme::parameters()?;

    let before_this = predecessor(before, &parameters)?;
    let under_test = block_opening_from(gap, provenance)?;
    let zone = corpus::zone()?;

    let (_, _): (_, Authored) = Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), zone.clone()),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&before_this, &parameters)
    .await?;

    accepted(
        Authoring::new(
            SqliteProgrammeStore::new(pool.clone(), zone),
            SqliteGenerationParameterStore::new(pool),
        )
        .author(&under_test, &parameters)
        .await,
    )
}

/// Whether the block was accepted, refusing to read any other failure as one.
///
/// Only the refusals this file is about count as "needs a test of its own".
/// Anything else is a broken fixture and must not read as a passing row.
fn accepted<T>(authored: Result<T, PrescriptionError>) -> Result<bool, Box<dyn std::error::Error>> {
    match authored {
        Ok(_) => Ok(true),
        Err(
            PrescriptionError::NoMaximumToOpenFrom { .. }
            | PrescriptionError::MaximumIsStale { .. },
        ) => Ok(false),
        Err(other) => Err(Box::new(other)),
    }
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the fixture authors: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// What the programme before produced decides whether the claim can stand.
///
/// ```text
/// test a   → block b   produces a ≠ b            no such test
/// test b   → block b   produces b                opens from it
/// linear a → block b   produces nothing          no such test
/// linear b → block b   produces nothing          no such test
/// block a  → block b   produces a ≠ b            no such test
/// block b  → block b   produces b, its exit test opens from it
/// ```
///
/// Every block here says `provenance = "tested"`, which is the operator claiming
/// the first of the three ways to arrive at an anchor. Row four is the one that
/// invites a wrong guess: a linear programme for the same lift never tests it
/// (decision 0013), so its last heavy single is not a maximum however heavy it
/// was.
#[test]
fn what_came_before_decides_whether_the_claim_stands() {
    let rows = [
        (Before::Test(A), false),
        (Before::Test(B), true),
        (Before::Linear(A), false),
        (Before::Linear(B), false),
        (Before::Block(A), false),
        (Before::Block(B), true),
    ];
    for (before, opens_without_testing) in rows {
        let got = run!(composes(before, 0));
        assert_eq!(
            got,
            opens_without_testing,
            "{} → block for exercise b claiming a tested anchor: expected it to {}",
            before.label(),
            if opens_without_testing {
                "open from that maximum"
            } else {
                "find no such test"
            }
        );
    }
}

/// And how long ago decides it too.
///
/// ```text
/// test b  → block b   adjacent    opens from it
/// test b  → 1 week  → block b     opens from it
/// test b  → 2 weeks → block b     too old
/// block b → 1 week  → block b     opens from it
/// block b → 2 weeks → block b     too old
/// ```
///
/// The operator's rule is the week before the programme or the week before that,
/// so a blank week between them still leaves the maximum speaking and two does
/// not. Counted between Mondays: the day-difference-over-seven the calendar uses
/// reads a Friday test against a Monday start as the same week, which is wrong in
/// the middle of every week.
#[test]
fn how_long_ago_decides_it_too() {
    for before in [Before::Test(B), Before::Block(B)] {
        for (gap, opens_without_testing) in [(0, true), (1, true), (2, false)] {
            let got = run!(composes(before, gap));
            assert_eq!(
                got,
                opens_without_testing,
                "{} → {gap} blank week(s) → block for exercise b",
                before.label()
            );
        }
    }
}

/// Declaring is one of the three ways to arrive at an anchor, always.
///
/// **A block's anchor is a previous test, an entry test of its own, or a
/// declared number** (decision 0016), and the operator picks. What the store
/// checks is only the first, because it is the only one that says something
/// about the past. So a block that could have inherited a maximum and instead
/// says "this is a number I am asserting" is authored — the provenance records
/// honestly which of the three it was.
#[test]
fn a_declared_anchor_is_never_refused() {
    // With a test of the same lift in the week before, which the block could
    // have opened from.
    let accepted = run!(composes_with(
        Before::Test(B),
        0,
        AnchorProvenance::Asserted
    ));
    assert!(
        accepted,
        "declaring is a statement about a number, not a claim about a test"
    );

    // And with nothing before it at all.
    let alone = run!(authors_alone(AnchorProvenance::Asserted));
    assert!(alone, "and needs nothing before it to be legitimate");
}

/// But claiming a test that did not happen is refused.
///
/// The claim and the choice are different things: a block with nothing to
/// inherit may run its own entry test or declare a number, and may not say a
/// measurement happened when none did.
#[test]
fn claiming_a_test_that_did_not_happen_is_refused() {
    let alone = run!(authors_alone(AnchorProvenance::Tested));
    assert!(
        !alone,
        "with nothing before it, there is no test for a tested anchor to be"
    );
}

/// Author the block alone, with no predecessor at all.
async fn authors_alone(provenance: AnchorProvenance) -> Result<bool, Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;
    let parameters = programme::parameters()?;
    let under_test = block_opening_from(0, provenance)?;
    let authored = Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), corpus::zone()?),
        SqliteGenerationParameterStore::new(pool),
    )
    .author(&under_test, &parameters)
    .await;
    accepted(authored)
}
