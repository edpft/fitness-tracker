//! Writing a programme document, the other direction from [`super::document`].
//!
//! **The format lives here in both directions (§ 21).** `cli` asks the operator
//! the questions — that is an operator interaction and belongs at the driving
//! edge — and hands the answers over as a [`Draft`]. What TOML looks like is
//! this crate's business and nobody else's, which the `document-format` check
//! enforces by refusing anything but `infrastructure` a dependency on `toml`.
//!
//! **Written rather than serialised**, so the comments survive. A document
//! nobody can read is not the reviewable artefact the wizard exists to leave
//! behind, and `serde` would give a correct file with nothing in it to explain
//! itself.

use domain::prescription::SlotId;
use jiff::civil::Date;

/// What the block is, before what it contains.
///
/// Few and short, which is the point: the seventeen slots are the long part,
/// and everything here is a line the operator already knows the answer to.
pub struct Draft {
    pub name: String,
    pub start: Date,
    pub pattern: &'static str,
    pub primary: String,
    pub weekdays: Vec<(&'static str, &'static str)>,
    /// What this template needs beyond the five facts above.
    pub shape: Shape,
}

/// The part of a document that differs by template.
///
/// **Split here because the document reader splits here.** `document.rs`
/// refuses an anchor, an opening, a gating role or a duration on a test, and
/// refuses an entry test on a linear — so a flat struct with every field
/// optional would let the wizard write a document the reader rejects, and would
/// put the rules in a second place to keep in step.
pub enum Shape {
    /// One week, measuring. It carries no anchor because measuring is what it
    /// is for, and no duration because a test is a week (decision 0013).
    Test {
        reps: u32,
        /// The load to attempt. `None` inherits it from the programme this
        /// follows, which is the ordinary case (decision 0013).
        target: Option<String>,
    },
    /// A programme that climbs a top-set ladder: linear or block.
    Climb {
        weeks: u32,
        gating: &'static str,
        anchor: String,
        anchor_from: Date,
        /// How the anchor was arrived at: read off the record, or declared.
        provenance: &'static str,
        ladder: Ladder,
    },
}

/// Which of the two climbing templates, and the one field only it has.
pub enum Ladder {
    /// Every week it holds is a climbing week; it opens where the operator
    /// says, or from the anchor when he says nothing.
    Linear { opening: Option<String> },
    /// It owns the week that measures its anchor, prepended rather than
    /// counted (decision 0013).
    Block {
        entry_reps: u32,
        entry_light: Option<String>,
    },
    /// A published chart. **Nothing to author**: it states every set, every
    /// repetition, every percentage, its own four weeks and which session
    /// advances it (decision 0024).
    Sbs,
}

impl Shape {
    /// The word the document names this template by, and the reader matches on.
    const fn template(&self) -> &'static str {
        match self {
            Self::Test { .. } => "test",
            Self::Climb {
                ladder: Ladder::Linear { .. },
                ..
            } => "linear",
            Self::Climb {
                ladder: Ladder::Sbs,
                ..
            } => "sbs",
            Self::Climb {
                ladder: Ladder::Block { .. },
                ..
            } => "block",
        }
    }
}

/// One slot's entry in the document.
///
/// Three shapes, because a slot has three: prescribed outright with its own
/// sets and reps, the same on both sessions, or one exercise per session role.
pub enum FillLine {
    Static {
        exercise: String,
        sets: u32,
        reps: u32,
    },
    Same(String),
    Alternating {
        light: String,
        heavy: String,
    },
}

/// One line into the document.
///
/// `push_str(&format!(…))` allocates twice and clippy says so nineteen times;
/// `writeln!` into a `String` cannot fail, so the `Result` is discarded here
/// once rather than at every line.
macro_rules! line {
    ($out:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = writeln!($out, $($arg)*);
    }};
}

/// The document, as the operator will read it back.
///
/// Written rather than serialised, so the comments survive: a document nobody
/// can read is not the reviewable artefact the wizard exists to leave behind.
#[must_use]
pub fn render(programme: &Draft, fills: &[(SlotId, FillLine)]) -> String {
    let mut out = String::new();
    line!(
        out,
        "# Written by `fitness programme add`. Yours to edit and re-author:"
    );
    line!(
        out,
        "# authoring again under the same name corrects this programme rather"
    );
    line!(out, "# than starting a second one.");
    line!(out, "");

    line!(out, "[programme]");
    line!(out, "name             = {:?}", programme.name);
    line!(out, "template         = {:?}", programme.shape.template());
    line!(out, "primary          = {:?}", programme.pattern);
    line!(out, "primary_exercise = {:?}", programme.primary);
    match &programme.shape {
        Shape::Test { reps, .. } => {
            line!(out, "start            = {:?}", programme.start.to_string());
            line!(out, "reps             = {reps}");
        }
        // **The chart states its own duration and its own gating session**, so
        // a document naming either is refused by the reader rather than
        // ignored. Writing them here would produce a document this build will
        // not read back.
        Shape::Climb {
            ladder: Ladder::Sbs,
            ..
        } => line!(out, "start            = {:?}", programme.start.to_string()),
        Shape::Climb { weeks, gating, .. } => {
            line!(out, "gating_role      = {gating:?}");
            line!(out, "start            = {:?}", programme.start.to_string());
            line!(out, "duration_weeks   = {weeks}");
        }
    }
    if let Shape::Climb {
        ladder: Ladder::Linear {
            opening: Some(opening),
        },
        ..
    } = &programme.shape
    {
        line!(out, "opening          = \"{opening}kg\"");
    }
    if let Shape::Test {
        target: Some(target),
        ..
    } = &programme.shape
    {
        line!(out, "target           = \"{target}kg\"");
    }

    line!(out, "");
    line!(
        out,
        "# No `interruptions`: they are derived from the schedule when this is"
    );
    line!(out, "# authored. State a list here to override that.");

    line!(out, "");
    line!(out, "[programme.weekdays]");
    for (day, role) in &programme.weekdays {
        line!(out, "{day} = {role:?}");
    }

    if let Shape::Climb {
        anchor,
        anchor_from,
        provenance,
        ladder,
        ..
    } = &programme.shape
    {
        line!(out, "");
        line!(out, "# What you expect to lift. Week one finds out.");
        line!(out, "[programme.anchor]");
        line!(out, "load       = \"{anchor}kg\"");
        line!(out, "provenance = {provenance:?}");
        line!(out, "from       = {:?}", anchor_from.to_string());

        if let Ladder::Block {
            entry_reps,
            entry_light,
        } = ladder
        {
            line!(out, "");
            line!(out, "[programme.entry_test]");
            line!(out, "reps  = {entry_reps}");
            if let Some(light) = entry_light {
                line!(out, "light = \"{light}kg\"");
            }
        }
    }

    slots_into(&mut out, fills);

    out
}

/// The fills, in the two passes TOML forces.
fn slots_into(out: &mut String, fills: &[(SlotId, FillLine)]) {
    // **Every bare key before the first table header.** TOML reads a bare key
    // as belonging to the table above it, so a simple fill written after
    // `[fills.plyometric]` would join that table and the document would stop
    // meaning what it looks like it means. Hence two passes.
    line!(out, "");
    line!(out, "[fills]");
    for (slot, fill) in fills {
        if let FillLine::Same(exercise) = fill {
            line!(out, "{:<28} = {exercise:?}", slot.as_str());
        }
    }
    for (slot, fill) in fills {
        match fill {
            FillLine::Same(_) => {}
            FillLine::Static {
                exercise,
                sets,
                reps,
            } => {
                line!(out, "");
                line!(out, "[fills.{}]", slot.as_str());
                line!(out, "exercise = {exercise:?}");
                line!(out, "sets     = {sets}");
                line!(out, "reps     = {reps}");
            }
            FillLine::Alternating { light, heavy } => {
                line!(out, "");
                line!(out, "[fills.{}]", slot.as_str());
                line!(out, "light = {light:?}");
                line!(out, "heavy = {heavy:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Draft, FillLine, Ladder, Shape, render};
    use domain::prescription::SlotId;
    use jiff::civil::date;

    fn autumn() -> Draft {
        Draft {
            name: "autumn".to_owned(),
            start: date(2026, 9, 14),
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            weekdays: vec![("monday", "light"), ("friday", "heavy")],
            shape: Shape::Climb {
                weeks: 9,
                gating: "heavy",
                anchor: "90".to_owned(),
                anchor_from: date(2026, 7, 3),
                provenance: "estimated",
                ladder: Ladder::Block {
                    entry_reps: 3,
                    entry_light: Some("60".to_owned()),
                },
            },
        }
    }

    /// **Every bare key comes before the first table header.**
    ///
    /// TOML reads a bare key as belonging to the table above it, so a simple
    /// fill written after `[fills.plyometric]` becomes part of the plyometric
    /// table and the document stops meaning what it looks like it means. The
    /// two passes in `render` exist for this and nothing else.
    #[test]
    fn the_simple_fills_are_written_before_any_table() {
        let document = render(
            &autumn(),
            &[
                (
                    SlotId::Plyometric,
                    FillLine::Static {
                        exercise: "pogo".to_owned(),
                        sets: 3,
                        reps: 20,
                    },
                ),
                (SlotId::UpperPush, FillLine::Same("chest-dip".to_owned())),
                (
                    SlotId::HipDominant,
                    FillLine::Alternating {
                        light: "back-extension-machine".to_owned(),
                        heavy: "nordic-hamstrings-curls".to_owned(),
                    },
                ),
            ],
        );

        let Some(simple) = document.find("upper_push") else {
            panic!("the simple fill is written")
        };
        let Some(first_table) = document.find("[fills.") else {
            panic!("a table is written")
        };
        assert!(
            simple < first_table,
            "a bare key after a table header belongs to that table:\n{document}"
        );
    }

    /// What the wizard writes, a document reads back.
    ///
    /// The renderer builds TOML by hand so its comments survive, which means
    /// nothing but this checks that what it builds parses.
    #[test]
    fn what_is_written_is_a_document() {
        let document = render(
            &autumn(),
            &[
                (
                    SlotId::Plyometric,
                    FillLine::Static {
                        exercise: "pogo".to_owned(),
                        sets: 3,
                        reps: 20,
                    },
                ),
                (
                    SlotId::KneeDominant,
                    FillLine::Same("front-squat".to_owned()),
                ),
                (
                    SlotId::HipDominant,
                    FillLine::Alternating {
                        light: "back-extension-machine".to_owned(),
                        heavy: "nordic-hamstrings-curls".to_owned(),
                    },
                ),
            ],
        );

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };

        let programme = &parsed["programme"];
        assert_eq!(programme["name"].as_str(), Some("autumn"));
        assert_eq!(programme["template"].as_str(), Some("block"));
        assert_eq!(programme["duration_weeks"].as_integer(), Some(9));
        assert_eq!(parsed["programme"]["anchor"]["load"].as_str(), Some("90kg"));

        let fills = &parsed["fills"];
        assert_eq!(fills["knee_dominant"].as_str(), Some("front-squat"));
        assert_eq!(fills["plyometric"]["sets"].as_integer(), Some(3));
        assert_eq!(
            fills["hip_dominant"]["heavy"].as_str(),
            Some("nordic-hamstrings-curls")
        );

        // Absent, so authoring derives them from the schedule. Writing `[]`
        // here would claim the block runs through every holiday.
        assert!(
            programme.get("interruptions").is_none(),
            "interruptions are derived, not written"
        );
    }

    /// A block that states no light load for its entry test writes no key,
    /// rather than an empty one the document would refuse.
    #[test]
    fn an_entry_test_with_no_light_load_omits_the_key() {
        let mut block = autumn();
        block.shape = Shape::Climb {
            weeks: 9,
            gating: "heavy",
            anchor: "90".to_owned(),
            anchor_from: date(2026, 7, 3),
            provenance: "estimated",
            ladder: Ladder::Block {
                entry_reps: 3,
                entry_light: None,
            },
        };
        let document = render(&block, &[]);

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };
        let entry = &parsed["programme"]["entry_test"];

        assert_eq!(entry["reps"].as_integer(), Some(3));
        assert!(
            entry.get("light").is_none(),
            "no light key at all — and `monday = \"light\"` is why this is asked \
             of the parsed document rather than of the text:\n{document}"
        );
    }

    /// **A test writes no anchor, no duration and no gating role.**
    ///
    /// `document.rs` refuses all three on a test — measuring is what it is for,
    /// a test is a week, and there is no ladder to gate. A renderer that wrote
    /// them anyway would produce a document only the wizard could love.
    #[test]
    fn a_test_writes_only_what_a_test_has() {
        let draft = Draft {
            name: "september-test".to_owned(),
            start: date(2026, 9, 7),
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            weekdays: vec![("monday", "light"), ("friday", "heavy")],
            shape: Shape::Test {
                reps: 3,
                target: None,
            },
        };
        let document = render(&draft, &[]);

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };
        let programme = &parsed["programme"];

        assert_eq!(programme["template"].as_str(), Some("test"));
        assert_eq!(programme["reps"].as_integer(), Some(3));
        for refused in ["anchor", "opening", "gating_role", "duration_weeks"] {
            assert!(
                programme.get(refused).is_none(),
                "a test document carries no {refused}:\n{document}"
            );
        }
        // Absent rather than empty: no target is what inherits one from the
        // programme this test follows (decision 0013).
        assert!(
            programme.get("target").is_none(),
            "an inherited target writes no key:\n{document}"
        );
    }

    /// A declared target is written, because it is the case inheritance cannot
    /// answer.
    #[test]
    fn a_declared_target_is_written() {
        let draft = Draft {
            name: "september-test".to_owned(),
            start: date(2026, 9, 7),
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            weekdays: vec![("friday", "heavy")],
            shape: Shape::Test {
                reps: 1,
                target: Some("95".to_owned()),
            },
        };
        let document = render(&draft, &[]);

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };
        assert_eq!(
            parsed["programme"]["target"].as_str(),
            Some("95kg"),
            "and with its unit, as every load in a document is written"
        );
    }

    /// **A linear writes no entry test.** Every week it holds is a climbing
    /// week, so `document.rs` refuses one — and the opening is the field it has
    /// in its place.
    #[test]
    fn a_linear_writes_an_opening_and_no_entry_test() {
        let draft = Draft {
            name: "summer".to_owned(),
            start: date(2026, 7, 6),
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            weekdays: vec![("monday", "light"), ("friday", "heavy")],
            shape: Shape::Climb {
                weeks: 8,
                gating: "heavy",
                anchor: "90".to_owned(),
                anchor_from: date(2026, 7, 3),
                provenance: "tested",
                ladder: Ladder::Linear {
                    opening: Some("80".to_owned()),
                },
            },
        };
        let document = render(&draft, &[]);

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };
        let programme = &parsed["programme"];

        assert_eq!(programme["template"].as_str(), Some("linear"));
        assert_eq!(programme["duration_weeks"].as_integer(), Some(8));
        assert_eq!(programme["opening"].as_str(), Some("80kg"));
        assert_eq!(programme["anchor"]["load"].as_str(), Some("90kg"));
        assert!(
            programme.get("entry_test").is_none(),
            "a linear holds no test week:\n{document}"
        );
    }

    /// A linear that states no opening writes no key, and derives it from the
    /// anchor instead.
    #[test]
    fn a_linear_with_no_opening_omits_the_key() {
        let draft = Draft {
            name: "summer".to_owned(),
            start: date(2026, 7, 6),
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            weekdays: vec![("monday", "light"), ("friday", "heavy")],
            shape: Shape::Climb {
                weeks: 8,
                gating: "heavy",
                anchor: "90".to_owned(),
                anchor_from: date(2026, 7, 3),
                provenance: "tested",
                ladder: Ladder::Linear { opening: None },
            },
        };
        let document = render(&draft, &[]);

        let parsed: toml::Value = match toml::from_str(&document) {
            Ok(value) => value,
            Err(error) => panic!("what the wizard writes parses: {error}\n{document}"),
        };
        assert!(
            parsed["programme"].get("opening").is_none(),
            "no opening key at all:\n{document}"
        );
    }
}
