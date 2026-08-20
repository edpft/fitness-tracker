//! The authored programme, as a document.
//!
//! **The only place a TOML type exists.** § 21 exempts an interface language
//! kept at its adapter, and the exemption is only honest while nothing here
//! escapes: every shape below converts into a `domain` type before it is
//! returned, and the `architecture` check verifies by ring that `domain` has no
//! `toml` dependency.
//!
//! A document rather than thirty command-line flags, because the programme's
//! only reason to be readable is that a person edits it and checks it over.

use std::collections::BTreeMap;

use domain::{
    gym::{
        Duration, Kg, RepCount,
        exercise::{DistanceExercise, DurationExercise, Exercise, RepsExercise},
    },
    prescription::{
        AccessoryScheme, Anchor, AnchorProvenance, Calendar, GenerationParameters,
        InconsistentProgramme, InvalidCalendar, PerRole, Percentage, PlateIncrement, Programme,
        ResetProtocol, SessionRole, TopSetReps, WarmupStep, Weekdays,
        linear::{Fill, PrimaryPattern, SlotFills, StaticFill},
    },
};
use jiff::{civil::Date, tz::TimeZone};

/// Why a document could not be authored.
#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error("the programme document could not be read: {0}")]
    Unreadable(#[from] std::io::Error),
    #[error("the programme document is not valid TOML: {0}")]
    Malformed(#[from] toml::de::Error),
    /// A value the operator has not settled yet.
    ///
    /// Refused rather than defaulted: a placeholder that authors successfully is
    /// worse than one that fails, because the prescription it produces looks
    /// exactly like one derived from a decision.
    #[error("{field} is still TODO — author it before prescribing from this programme")]
    Unsettled { field: String },
    #[error("{field}: {detail}")]
    Invalid { field: String, detail: String },
    #[error(transparent)]
    Inconsistent(#[from] InconsistentProgramme),
    #[error("programme.interruptions: {0}")]
    Uncalendarable(#[from] InvalidCalendar),
}

fn invalid(field: &str, detail: impl std::fmt::Display) -> DocumentError {
    DocumentError::Invalid {
        field: field.to_owned(),
        detail: detail.to_string(),
    }
}

/// A value that may still be `TODO`.
fn settled<'a>(field: &str, value: &'a str) -> Result<&'a str, DocumentError> {
    if value.trim() == "TODO" {
        return Err(DocumentError::Unsettled {
            field: field.to_owned(),
        });
    }
    Ok(value)
}

fn percentage(field: &str, value: &str) -> Result<Percentage, DocumentError> {
    Percentage::try_from(settled(field, value)?.to_owned()).map_err(|error| invalid(field, error))
}

fn mass(field: &str, value: &str) -> Result<Kg, DocumentError> {
    let text = settled(field, value)?;
    Kg::try_from(text.trim_end_matches("kg").to_owned()).map_err(|error| invalid(field, error))
}

fn seconds(field: &str, value: &str) -> Result<Duration, DocumentError> {
    let text = settled(field, value)?.trim_end_matches('s');
    text.parse::<u64>()
        .map(Duration::from_seconds)
        .map_err(|error| invalid(field, error))
}

fn reps(field: &str, value: u32) -> Result<RepCount, DocumentError> {
    RepCount::new(value).map_err(|error| invalid(field, error))
}

/// Our vocabulary, from a key in the document.
fn exercise(field: &str, key: &str) -> Result<Exercise, DocumentError> {
    if let Ok(reps) = RepsExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Reps(reps));
    }
    if let Ok(duration) = DurationExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Duration(duration));
    }
    if let Ok(distance) = DistanceExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Distance(distance));
    }
    Err(invalid(
        field,
        format!("{key:?} does not name an exercise in the vocabulary"),
    ))
}

// --- The document's shapes -------------------------------------------------

#[derive(serde::Deserialize)]
pub struct Document {
    programme: ProgrammeSection,
    fills: BTreeMap<String, toml::Value>,
    parameters: ParametersSection,
}

#[derive(serde::Deserialize)]
struct ProgrammeSection {
    template: String,
    #[serde(rename = "primary")]
    primary_pattern: String,
    primary_exercise: String,
    gating_role: String,
    start: String,
    duration_weeks: u32,
    /// The weeks the block does not run, each named by a date inside it.
    ///
    /// Defaulted rather than required, because a block with nothing in its way
    /// is a real state and not an unsettled one — unlike a `TODO`, which is a
    /// decision nobody has taken yet.
    #[serde(default)]
    interruptions: Vec<String>,
    weekdays: BTreeMap<String, String>,
    anchor: AnchorSection,
}

#[derive(serde::Deserialize)]
struct AnchorSection {
    load: String,
    /// What the test failed above `load`, if it found the ceiling. Absent is a
    /// test that failed nothing, which is a different state and not a default.
    #[serde(default)]
    failed: Option<String>,
    provenance: String,
    from: String,
}

#[derive(serde::Deserialize)]
struct ParametersSection {
    back_off_of_top_set: String,
    plate_increment: String,
    light_of_heavy: String,
    static_hold: String,
    ladder: LadderSection,
    strength: AccessorySection,
    hypertrophy: AccessorySection,
    roles: BTreeMap<String, RoleSection>,
    warmup: Vec<WarmupSection>,
    reset: ResetSection,
}

#[derive(serde::Deserialize)]
struct LadderSection {
    climb_per_week: String,
}

#[derive(serde::Deserialize)]
struct AccessorySection {
    reps: String,
    sets: u32,
}

#[derive(serde::Deserialize)]
struct RoleSection {
    top_set_reps: u32,
}

#[derive(serde::Deserialize)]
struct WarmupSection {
    of_top_set: String,
    reps: u32,
}

#[derive(serde::Deserialize)]
struct ResetSection {
    first: ResetProtocolSection,
    second: ResetProtocolSection,
}

#[derive(serde::Deserialize)]
struct ResetProtocolSection {
    drop: String,
    reclimb_per_week: String,
}

impl Document {
    /// Read a document from a path.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the file is missing, is not TOML, or holds a value
    /// the vocabulary does not recognise.
    pub fn read(path: &std::path::Path) -> Result<Self, DocumentError> {
        Ok(toml::from_str(&std::fs::read_to_string(path)?)?)
    }

    /// The parameters, in domain terms.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] for an unsettled or invalid value.
    pub fn parameters(&self) -> Result<GenerationParameters, DocumentError> {
        let p = &self.parameters;

        let mut warmup = Vec::with_capacity(p.warmup.len());
        for (at, step) in p.warmup.iter().enumerate() {
            warmup.push(WarmupStep {
                of_top_set: percentage(
                    &format!("parameters.warmup[{at}].of_top_set"),
                    &step.of_top_set,
                )?,
                reps: reps(&format!("parameters.warmup[{at}].reps"), step.reps)?,
            });
        }
        let warmup = domain::gym::NonEmpty::new(warmup)
            .map_err(|_| invalid("parameters.warmup", "a ramp needs at least one step"))?;

        let role = |name: &str| -> Result<TopSetReps, DocumentError> {
            let section = p.roles.get(name).ok_or_else(|| {
                invalid(
                    "parameters.roles",
                    format!("no {name} session is described"),
                )
            })?;
            Ok(TopSetReps::new(reps(
                &format!("parameters.roles.{name}.top_set_reps"),
                section.top_set_reps,
            )?))
        };

        Ok(GenerationParameters {
            warmup,
            back_off_of_top_set: percentage(
                "parameters.back_off_of_top_set",
                &p.back_off_of_top_set,
            )?,
            light_of_heavy: percentage("parameters.light_of_heavy", &p.light_of_heavy)?,
            ladder_climb_per_week: mass(
                "parameters.ladder.climb_per_week",
                &p.ladder.climb_per_week,
            )?,
            top_set_reps: PerRole {
                light: role("light")?,
                heavy: role("heavy")?,
            },
            strength: scheme("parameters.strength", &p.strength)?,
            hypertrophy: scheme("parameters.hypertrophy", &p.hypertrophy)?,
            static_hold: seconds("parameters.static_hold", &p.static_hold)?,
            plate_increment: PlateIncrement::new(mass(
                "parameters.plate_increment",
                &p.plate_increment,
            )?)
            .map_err(|error| invalid("parameters.plate_increment", error))?,
            first_reset: ResetProtocol {
                drop: percentage("parameters.reset.first.drop", &p.reset.first.drop)?,
                reclimb_per_week: mass(
                    "parameters.reset.first.reclimb_per_week",
                    &p.reset.first.reclimb_per_week,
                )?,
            },
            second_reset: ResetProtocol {
                drop: percentage("parameters.reset.second.drop", &p.reset.second.drop)?,
                reclimb_per_week: mass(
                    "parameters.reset.second.reclimb_per_week",
                    &p.reset.second.reclimb_per_week,
                )?,
            },
        })
    }

    /// The programme, in domain terms, validated.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] for an unsettled value, an unknown exercise, or a
    /// programme the three consistency checks refuse.
    pub fn programme(
        &self,
        parameters: &GenerationParameters,
        zone: TimeZone,
    ) -> Result<Programme, DocumentError> {
        let section = &self.programme;
        if section.template != "linear" {
            return Err(invalid(
                "programme.template",
                format!(
                    "{:?} is not a template this build can read; it reads \
                     \"linear\", which was called \"v1\" until 2026-08-18",
                    section.template
                ),
            ));
        }

        let mut days = Vec::with_capacity(section.weekdays.len());
        for (day, role) in &section.weekdays {
            days.push((
                weekday(day)?,
                SessionRole::try_from(role.clone())
                    .map_err(|error| invalid("programme.weekdays", error))?,
            ));
        }
        let weekdays = Weekdays::new(days).map_err(|error| invalid("programme.weekdays", error))?;

        let failed = section
            .anchor
            .failed
            .as_deref()
            .map(|value| mass("programme.anchor.failed", value))
            .transpose()?;
        let anchor = Anchor::new(
            mass("programme.anchor.load", &section.anchor.load)?,
            failed,
            AnchorProvenance::try_from(section.anchor.provenance.clone())
                .map_err(|error| invalid("programme.anchor.provenance", error))?,
            settled("programme.anchor.from", &section.anchor.from)?
                .parse::<Date>()
                .map_err(|error| invalid("programme.anchor.from", error))?,
        )
        .map_err(|error| invalid("programme.anchor", error))?;

        let mut interruptions = Vec::with_capacity(section.interruptions.len());
        for (at, week) in section.interruptions.iter().enumerate() {
            let field = format!("programme.interruptions[{at}]");
            interruptions.push(
                settled(&field, week)?
                    .parse::<Date>()
                    .map_err(|error| invalid(&field, error))?,
            );
        }

        let calendar = Calendar::new(
            settled("programme.start", &section.start)?
                .parse::<Date>()
                .map_err(|error| invalid("programme.start", error))?,
            section.duration_weeks,
            &interruptions,
            weekdays,
            zone,
        )?;

        Ok(Programme::new(
            PrimaryPattern::try_from(section.primary_pattern.clone())
                .map_err(|error| invalid("programme.primary", error))?,
            exercise("programme.primary_exercise", &section.primary_exercise)?,
            self.fills()?,
            anchor,
            SessionRole::try_from(section.gating_role.clone())
                .map_err(|error| invalid("programme.gating_role", error))?,
            calendar,
            parameters,
        )?)
    }

    /// Every slot's fill.
    fn fills(&self) -> Result<SlotFills, DocumentError> {
        Ok(SlotFills {
            plyometric: self.statics("plyometric")?,
            power: self.statics("power")?,
            knee_dominant: self.single("knee_dominant")?,
            upper_push: self.single("upper_push")?,
            upper_pull: self.single("upper_pull")?,
            hip_dominant: self.single("hip_dominant")?,
            biceps: self.single("biceps")?,
            triceps: self.single("triceps")?,
            wrist_flexion: self.single("wrist_flexion")?,
            wrist_extension: self.single("wrist_extension")?,
            core: self.single("core")?,
            handstand_hold: self.single("handstand_hold")?,
            dead_hang: self.single("dead_hang")?,
            hip_flexor_stretch: self.single("hip_flexor_stretch")?,
            hip_external_rotator_stretch: self.single("hip_external_rotator_stretch")?,
            hamstring_stretch: self.single("hamstring_stretch")?,
            groin_stretch: self.single("groin_stretch")?,
        })
    }

    /// A statically prescribed slot: an exercise plus its sets and repetitions.
    ///
    /// The whole prescription is authored, because a static slot does not
    /// progress and so has nothing to derive from.
    fn statics(&self, slot: &str) -> Result<Fill<StaticFill>, DocumentError> {
        let field = format!("fills.{slot}");
        let value = self
            .fills
            .get(slot)
            .ok_or_else(|| invalid(&field, "no fill for this slot"))?;
        let table = value.as_table().ok_or_else(|| {
            invalid(
                &field,
                "a static slot is a table of exercise, sets and reps",
            )
        })?;

        let read = |section: &toml::Value| -> Result<StaticFill, DocumentError> {
            let key = section
                .get("exercise")
                .and_then(toml::Value::as_str)
                .ok_or_else(|| invalid(&field, "no exercise"))?;
            let count = |name: &str| -> Result<RepCount, DocumentError> {
                let value = section
                    .get(name)
                    .and_then(toml::Value::as_integer)
                    .ok_or_else(|| invalid(&field, format!("no {name}")))?;
                let value = u32::try_from(value).map_err(|error| invalid(&field, error))?;
                reps(&field, value)
            };
            Ok(StaticFill {
                exercise: exercise(&field, key)?,
                sets: count("sets")?,
                reps: count("reps")?,
            })
        };

        if table.contains_key("exercise") {
            return Ok(Fill::Same(read(value)?));
        }
        let by_role = |role: &str| -> Result<StaticFill, DocumentError> {
            let section = table
                .get(role)
                .ok_or_else(|| invalid(&field, format!("no {role} prescription")))?;
            read(section)
        };
        Ok(Fill::Alternating(PerRole {
            light: by_role("light")?,
            heavy: by_role("heavy")?,
        }))
    }

    /// A slot filled with one exercise, the same both ways or one per role.
    fn single(&self, slot: &str) -> Result<Fill<Exercise>, DocumentError> {
        let field = format!("fills.{slot}");
        let value = self
            .fills
            .get(slot)
            .ok_or_else(|| invalid(&field, "no fill for this slot"))?;

        if let Some(key) = value.as_str() {
            return Ok(Fill::Same(exercise(&field, key)?));
        }
        let table = value
            .as_table()
            .ok_or_else(|| invalid(&field, "a fill is an exercise or a table keyed by role"))?;
        let by_role = |role: &str| -> Result<Exercise, DocumentError> {
            let key = table
                .get(role)
                .and_then(toml::Value::as_str)
                .ok_or_else(|| invalid(&field, format!("no {role} fill")))?;
            exercise(&field, key)
        };
        Ok(Fill::Alternating(PerRole {
            light: by_role("light")?,
            heavy: by_role("heavy")?,
        }))
    }
}

/// One block's double-progression scheme.
fn scheme(field: &str, section: &AccessorySection) -> Result<AccessoryScheme, DocumentError> {
    let (low, high) = section
        .reps
        .split_once('-')
        .ok_or_else(|| invalid(field, "a range reads as low-high"))?;
    let count = |value: &str| -> Result<RepCount, DocumentError> {
        value
            .trim()
            .parse::<u32>()
            .map_err(|error| invalid(field, error))
            .and_then(|parsed| reps(field, parsed))
    };
    Ok(AccessoryScheme {
        low: count(low)?,
        high: count(high)?,
        sets: reps(field, section.sets)?,
    })
}

fn weekday(key: &str) -> Result<jiff::civil::Weekday, DocumentError> {
    use jiff::civil::Weekday;
    match key {
        "monday" => Ok(Weekday::Monday),
        "tuesday" => Ok(Weekday::Tuesday),
        "wednesday" => Ok(Weekday::Wednesday),
        "thursday" => Ok(Weekday::Thursday),
        "friday" => Ok(Weekday::Friday),
        "saturday" => Ok(Weekday::Saturday),
        "sunday" => Ok(Weekday::Sunday),
        other => Err(invalid(
            "programme.weekdays",
            format!("{other:?} is not a weekday"),
        )),
    }
}
