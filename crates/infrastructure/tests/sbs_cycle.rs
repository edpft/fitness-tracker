//! The SBS cycle as an operator authors it, and what it refuses.
//!
//! **Through the document reader**, because that is the only way an SBS cycle
//! comes into existence and because it is where the template's own rules live:
//! a chart that states every set has nothing for an `opening`, an `entry_test`
//! or a `duration_weeks` to mean, and a document offering one has the wrong
//! template rather than a spare field.
//!
//! What the *chart* prescribes is proved in `domain/tests/sbs.rs` against the
//! workbook. This is about the programme built around it.

mod support;

use domain::prescription::{Periodisation, Programme, SlotId};
use infrastructure::{
    Document,
    programme::draft::{Draft, FillLine, Ladder, Shape, render},
};
use jiff::civil::Date;
use support::{corpus, programme};

/// A cycle as the operator would write one. **Four weeks and no duration**: the
/// chart is four weeks, so stating it would be stating the obvious and
/// permitting the wrong.
const SBS_DOCUMENT: &str = r#"
[programme]
name             = "autumn-2026-front-squat"
template         = "sbs"
primary          = "knee_dominant"
primary_exercise = "front-squat"
start            = "2026-09-14"

[programme.weekdays]
monday = "light"
friday = "heavy"

# From the standalone week 4 that runs first, so the cycle opens on a measured
# number rather than an expectation (decision 0024).
[programme.anchor]
load       = "100kg"
provenance = "tested"
from       = "2026-09-11"

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

/// Read a document, with no predecessor to inherit from.
fn read(text: &str) -> Result<Programme, Box<dyn std::error::Error>> {
    let document: Document = toml::from_str(text)?;
    Ok(document.programme(
        &programme::parameters()?,
        corpus::zone()?.as_time_zone(),
        None,
        &[],
    )?)
}

#[test]
fn a_document_authors_an_sbs_cycle() {
    let programme = read(SBS_DOCUMENT).expect("the document reads");

    assert_eq!(programme.template(), "sbs");
    assert_eq!(programme.name().as_str(), "autumn-2026-front-squat");
    assert!(
        matches!(programme, Programme::Periodisation(Periodisation::Sbs(_))),
        "and it is a periodisation, beside linear and block",
    );
    assert_eq!(
        programme.calendar().duration_weeks(),
        4,
        "four weeks, taken from the chart rather than from the document",
    );
}

#[test]
fn the_cycle_leaves_a_maximum_and_claims_one() {
    let programme = read(SBS_DOCUMENT).expect("the document reads");

    assert!(
        programme.produces_maximum().is_some(),
        "week 4 day 2 is a single, and it is what opens the next cycle",
    );
    assert!(
        programme.claims_an_earlier_maximum(),
        "a tested anchor here can only point backwards: the chart's test is its \
         last session, so there is no case where the cycle is about to measure \
         its own opening",
    );
}

#[test]
fn a_duration_is_refused_rather_than_obeyed() {
    let text = SBS_DOCUMENT.replace(
        "start            = \"2026-09-14\"",
        "start            = \"2026-09-14\"\nduration_weeks   = 5",
    );
    assert!(
        read(&text).is_err(),
        "a five-week SBS cycle is not this programme stretched, it is a \
         different programme",
    );
}

#[test]
fn an_opening_is_refused_because_every_load_is_a_share() {
    let text = SBS_DOCUMENT.replace(
        "start            = \"2026-09-14\"",
        "start            = \"2026-09-14\"\nopening          = \"80kg\"",
    );
    assert!(
        read(&text).is_err(),
        "the chart has no opening for a document to declare against",
    );
}

#[test]
fn an_entry_test_is_refused_because_the_test_is_at_the_end() {
    let text = format!("{SBS_DOCUMENT}\n[programme.entry_test]\nreps = 3\nlight = \"60kg\"\n");
    assert!(
        read(&text).is_err(),
        "an SBS cycle's test is its last session, not a week in front of it",
    );
}

#[test]
fn a_gating_role_is_refused_because_the_chart_says_which_session_advances() {
    let text = SBS_DOCUMENT.replace(
        "start            = \"2026-09-14\"",
        "start            = \"2026-09-14\"\ngating_role      = \"heavy\"",
    );
    assert!(
        read(&text).is_err(),
        "the second session of every week is the rep-max day by construction, \
         so there is nothing here for an operator to decide",
    );
}

#[test]
fn a_cycle_whose_anchor_is_not_before_it_is_refused() {
    // The anchor dated on the cycle's own start day. Refused for the reason the
    // linear template refuses it: a cycle containing the session that anchors it
    // would read that session twice.
    let text = SBS_DOCUMENT.replace("from       = \"2026-09-11\"", "from       = \"2026-09-14\"");
    assert!(
        read(&text).is_err(),
        "the test precedes the cycle it anchors"
    );
}

/// **What the wizard writes is what the reader accepts.**
///
/// The two halves are written separately and nothing made them agree: `render`
/// decides which keys a template emits, the reader decides which it accepts, and
/// a template whose halves disagree produces a document the wizard's own reader
/// refuses. `draft.rs`'s own tests assert on keys, which cannot catch that — a
/// key correctly written and correctly refused looks right to both.
///
/// SBS is where they are most likely to disagree, because it refuses four fields
/// the other climbing templates require or allow.
#[test]
fn the_wizard_s_sbs_output_authors_a_cycle() {
    let fills: Vec<(SlotId, FillLine)> = SlotId::ALL
        .iter()
        .map(|slot| match slot {
            SlotId::Plyometric => (
                *slot,
                FillLine::Static {
                    exercise: "pogo".to_owned(),
                    sets: 3,
                    reps: 20,
                },
            ),
            SlotId::Power => (
                *slot,
                FillLine::Static {
                    exercise: "box-jump".to_owned(),
                    sets: 3,
                    reps: 5,
                },
            ),
            other => (*other, FillLine::Same(fill_for(*other).to_owned())),
        })
        .collect();

    let draft = Draft {
        name: "autumn-2026-front-squat".to_owned(),
        start: Date::constant(2026, 9, 14),
        pattern: "knee_dominant",
        primary: "front-squat".to_owned(),
        weekdays: vec![("monday", "light"), ("friday", "heavy")],
        shape: Shape::Climb {
            weeks: 4,
            gating: "heavy",
            anchor: "100".to_owned(),
            anchor_from: Date::constant(2026, 9, 11),
            provenance: "tested",
            ladder: Ladder::Sbs,
        },
    };

    let document = render(&draft, &fills);

    // The four the chart states, which the reader refuses. Writing any of them
    // would make the wizard produce a document it cannot read back.
    for refused in ["duration_weeks", "gating_role", "opening", "entry_test"] {
        assert!(
            !document.contains(refused),
            "the wizard wrote {refused:?}, which the reader refuses:\n{document}",
        );
    }

    let programme = match read(&document) {
        Ok(programme) => programme,
        Err(error) => panic!("the wizard's own output was refused: {error}\n\n{document}"),
    };
    assert_eq!(programme.template(), "sbs");
    assert_eq!(programme.calendar().duration_weeks(), 4);
}

/// A fill the vocabulary knows, per slot.
const fn fill_for(slot: SlotId) -> &'static str {
    match slot {
        SlotId::KneeDominant => "front-squat",
        SlotId::UpperPush => "chest-dip",
        SlotId::UpperPull => "neutral-grip-pull-up",
        SlotId::HipDominant => "nordic-hamstrings-curls",
        SlotId::Biceps => "preacher-curl-barbell",
        SlotId::Triceps => "overhead-triceps-extension-cable",
        SlotId::WristFlexion => "wrist-flexion-dumbbell",
        SlotId::WristExtension => "wrist-extension-dumbbell",
        SlotId::Core => "bent-over-cable-chop",
        SlotId::HandstandHold => "handstand-hold",
        SlotId::DeadHang => "dead-hang",
        SlotId::HipFlexorStretch => "couch-stretch",
        SlotId::HipExternalRotatorStretch => "ninety-ninety",
        SlotId::HamstringStretch => "standing-straddle-fold",
        SlotId::GroinStretch => "squatting-groin-stretch",
        SlotId::Plyometric | SlotId::Power => "pogo",
    }
}
