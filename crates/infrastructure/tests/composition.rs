//! Which programmes may follow which, and what a block may open from.
//!
//! **The operator's ten rows, asserted rather than described** (decisions 0013
//! and 0016). Six are about what the programme before produced; four are about
//! how long ago it produced it. They are one rule between them:
//!
//! > A block needs an entry test of its own unless the programme immediately
//! > before it produces a maximum in the same lift, and does so recently enough
//! > to still speak for it.
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
/// the predecessor ends.
fn block_without_a_test(gap: i64) -> Result<Programme, Box<dyn std::error::Error>> {
    let start = ADJACENT.checked_add(jiff::Span::new().days(gap * 7))?;
    Ok(Programme::Periodisation(Periodisation::Block(
        Periodised::new(
            name("under-test")?,
            primary(B),
            fills(B)?,
            // Dated to the predecessor's test day, and labelled as measured —
            // which is what the operator writes when opening from one.
            Entry::derived(anchor(AnchorProvenance::Tested)?),
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
    let directory = tempfile::tempdir()?;
    let pool: SqlitePool = connect(&directory.path().join("test.db")).await?;
    let parameters = programme::parameters()?;

    let before_this = predecessor(before, &parameters)?;
    let under_test = block_without_a_test(gap)?;
    let zone = corpus::zone()?;

    let (_, _): (_, Authored) = Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), zone.clone()),
        SqliteGenerationParameterStore::new(pool.clone()),
    )
    .author(&before_this, &parameters)
    .await?;

    match Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), zone),
        SqliteGenerationParameterStore::new(pool),
    )
    .author(&under_test, &parameters)
    .await
    {
        Ok(_) => Ok(true),
        // Only the two refusals this file is about count as "needs its own test".
        // Anything else is a broken fixture and should not read as a passing row.
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

/// What the programme before produced decides whether the block must test.
///
/// ```text
/// test a   → block b   produces a ≠ b     needs its own
/// test b   → block b   produces b         uses it
/// linear a → block b   produces nothing   needs its own
/// linear b → block b   produces nothing   needs its own
/// block a  → block b   produces a ≠ b     needs its own
/// block b  → block b   produces b, its exit test   uses it
/// ```
///
/// Row four is the one that invites a wrong guess. A linear programme for the
/// same lift never tests it (decision 0013), so its last heavy single is not a
/// maximum however heavy it was.
#[test]
fn what_came_before_decides_whether_a_block_must_test() {
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
            "{} → block for exercise b: expected it to {}",
            before.label(),
            if opens_without_testing {
                "open from that maximum"
            } else {
                "need an entry test of its own"
            }
        );
    }
}

/// And how long ago decides it too.
///
/// ```text
/// test b  → block b   adjacent    uses it
/// test b  → 1 week  → block b     uses it
/// test b  → 2 weeks → block b     needs its own
/// block b → 1 week  → block b     uses it
/// block b → 2 weeks → block b     needs its own
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
