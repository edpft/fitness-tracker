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
    pub weeks: u32,
    pub pattern: &'static str,
    pub primary: String,
    pub gating: &'static str,
    pub weekdays: Vec<(&'static str, &'static str)>,
    pub anchor: String,
    pub anchor_from: Date,
    pub entry_reps: u32,
    pub entry_light: Option<String>,
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
pub fn render(block: &Draft, fills: &[(SlotId, FillLine)]) -> String {
    let mut out = String::new();
    line!(
        out,
        "# Written by `fitness programme add`. Yours to edit and re-author:"
    );
    line!(
        out,
        "# authoring again under the same name corrects this block rather than"
    );
    line!(out, "# starting a second one.");
    line!(out, "");

    line!(out, "[programme]");
    line!(out, "name             = {:?}", block.name);
    line!(out, "template         = \"block\"");
    line!(out, "primary          = {:?}", block.pattern);
    line!(out, "primary_exercise = {:?}", block.primary);
    line!(out, "gating_role      = {:?}", block.gating);
    line!(out, "start            = {:?}", block.start.to_string());
    line!(out, "duration_weeks   = {}", block.weeks);

    line!(out, "");
    line!(
        out,
        "# No `interruptions`: they are derived from the schedule when this is"
    );
    line!(out, "# authored. State a list here to override that.");

    line!(out, "");
    line!(out, "[programme.weekdays]");
    for (day, role) in &block.weekdays {
        line!(out, "{day} = {role:?}");
    }

    line!(out, "");
    line!(out, "# What you expect to lift. Week one finds out.");
    line!(out, "[programme.anchor]");
    line!(out, "load       = \"{}kg\"", block.anchor);
    line!(out, "provenance = \"asserted\"");
    line!(out, "from       = {:?}", block.anchor_from.to_string());

    line!(out, "");
    line!(out, "[programme.entry_test]");
    line!(out, "reps  = {}", block.entry_reps);
    if let Some(light) = &block.entry_light {
        line!(out, "light = \"{light}kg\"");
    }

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

    out
}

#[cfg(test)]
mod tests {
    use super::{Draft, FillLine, render};
    use domain::prescription::SlotId;
    use jiff::civil::date;

    fn autumn() -> Draft {
        Draft {
            name: "autumn".to_owned(),
            start: date(2026, 9, 14),
            weeks: 9,
            pattern: "knee_dominant",
            primary: "front-squat".to_owned(),
            gating: "heavy",
            weekdays: vec![("monday", "light"), ("friday", "heavy")],
            anchor: "90".to_owned(),
            anchor_from: date(2026, 7, 3),
            entry_reps: 3,
            entry_light: Some("60".to_owned()),
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
        block.entry_light = None;
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
}
