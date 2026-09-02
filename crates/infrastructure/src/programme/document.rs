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

use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    gym::{
        Duration, Kg, RepCount,
        exercise::{DistanceExercise, DurationExercise, Exercise, Implement, RepsExercise},
    },
    prescription::{
        AccessoryScheme, Anchor, AnchorProvenance, BackOff, BlockRest, Calendar, Entry,
        GenerationParameters, InconsistentProgramme, InvalidCalendar, Linear, LoadSteps, PerRole,
        Percentage, Periodisation, Periodised, Programme, ProgrammeName, ResetProtocol, RestScheme,
        Sbs, Scales, SessionRole, Skip, Step, Target, Test, TestTarget, Tested, TopSetReps,
        WarmupStep, Weekdays,
        block::EntryTest,
        linear::{Fill, Primary, PrimaryPattern, SlotFills, StaticFill},
        sbs::WEEKS,
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

/// Every block's rest, from the document's tables.
///
/// A block missing from the document is named rather than defaulted: there is no
/// sensible rest to invent for work nobody has said how to pace, and a silent
/// zero would instruct straight-through training.
fn rest_scheme(authored: &BTreeMap<String, RestSection>) -> Result<RestScheme, DocumentError> {
    let block = |name: &str| -> Result<BlockRest, DocumentError> {
        let field = format!("parameters.rest.{name}");
        let section = authored.get(name).ok_or_else(|| {
            invalid(
                "parameters.rest",
                format!("no rest is described for the {name} block"),
            )
        })?;
        Ok(BlockRest {
            between_sets: rest_target(&format!("{field}.between_sets"), &section.between_sets)?,
            after_superset: section
                .after_superset
                .as_deref()
                .map(|stated| rest_target(&format!("{field}.after_superset"), stated))
                .transpose()?,
        })
    };

    Ok(RestScheme {
        plyometric: block("plyometric")?,
        power: block("power")?,
        strength: block("strength")?,
        hypertrophy: block("hypertrophy")?,
        mobility: block("mobility")?,
    })
}

/// A rest instruction: one duration, or two separated by a dash.
///
/// **The document names bounds and the domain holds a span**, so this is the
/// boundary where `"180s-120s"` is refused. The type behind it cannot express
/// that mistake, which is exactly why the check belongs here and nowhere
/// further in.
fn rest_target(field: &str, value: &str) -> Result<Target<Duration>, DocumentError> {
    let text = settled(field, value)?;
    let Some((low, high)) = text.split_once('-') else {
        return Ok(Target::Exactly(seconds(field, text)?));
    };

    Target::between(seconds(field, low)?, seconds(field, high)?)
        .ok_or_else(|| invalid(field, "a rest range runs low-high and must span"))
}

fn reps(field: &str, value: u32) -> Result<RepCount, DocumentError> {
    RepCount::new(value).map_err(|error| invalid(field, error))
}

/// Every implement's scale, from the document's table.
fn scales(authored: &BTreeMap<String, ScaleSection>) -> Result<Scales, DocumentError> {
    let mut scales = BTreeMap::new();
    for (name, section) in authored {
        let field = format!("parameters.scales.{name}");
        let implement = Implement::try_from(name.clone())
            .map_err(|error| invalid("parameters.scales", error))?;
        let steps = match section {
            ScaleSection::Uniform(size) => LoadSteps::uniform(mass(&field, size)?),
            ScaleSection::Banded(bands) => {
                let mut read = Vec::with_capacity(bands.len());
                for (at, band) in bands.iter().enumerate() {
                    read.push(Step {
                        from: mass(&format!("{field}[{at}].from"), &band.from)?,
                        size: mass(&format!("{field}[{at}].size"), &band.size)?,
                    });
                }
                LoadSteps::new(read)
            }
        }
        .map_err(|error| invalid(&field, error))?;
        scales.insert(implement, steps);
    }
    Ok(Scales::new(scales))
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
    /// **Partial for a test, total for anything else.**
    ///
    /// A test week is two sessions and the operator does not re-author
    /// seventeen slots for it (decision 0013): the document names the lift being
    /// tested and any accessory variant moving with it, and every slot it leaves
    /// out is taken from the programme this one follows. Defaulted so a test
    /// changing nothing but the primary needs no section at all.
    #[serde(default)]
    fills: BTreeMap<String, toml::Value>,
    /// **Absent means the set in force.**
    ///
    /// § 14 requires only the current value of a generation parameter, and what
    /// each prescription was generated against is recorded concretely on the
    /// prescription itself — so a document that has nothing to say about them
    /// need not restate them. Which matters most for a test: it is two sessions,
    /// and requiring it to repeat every warm-up step and load scale to say so
    /// would be the whole-programme authoring decision 0013 exists to avoid.
    ///
    /// Stating them is how they are *changed*, and a linear programme or a block
    /// authored without them is authored against whatever is already stored.
    #[serde(default)]
    parameters: Option<ParametersSection>,
}

#[derive(serde::Deserialize)]
struct ProgrammeSection {
    /// What identifies this programme across re-authorings (decision 0012).
    ///
    /// In the document rather than on the command line, because § 12 makes the
    /// authored record a primary input: it has to be reproducible from the
    /// document alone, and `--amend` is invocation state no document remembers.
    /// Re-authoring under the same name corrects that programme; a new name
    /// starts a new one, and `programme author` says which it is doing.
    name: String,
    template: String,
    #[serde(rename = "primary")]
    primary_pattern: String,
    primary_exercise: String,
    /// Which session's top set advances the plan.
    ///
    /// Absent for a test, which advances nothing: it has no ladder to gate and
    /// its own session is the heavy one by definition.
    #[serde(default)]
    gating_role: Option<String>,
    start: String,
    /// Absent for a test, which is one week by definition and has nowhere to put
    /// a duration that could disagree with that.
    #[serde(default)]
    duration_weeks: Option<u32>,
    /// The weeks the block does not run, each named by a date inside it.
    ///
    /// Defaulted rather than required, because a block with nothing in its way
    /// is a real state and not an unsettled one — unlike a `TODO`, which is a
    /// decision nobody has taken yet.
    #[serde(default)]
    /// **Absent derives; `[]` states none.**
    ///
    /// A block loses the days the operator cannot train, and the schedule
    /// already knows which those are — so a document that says nothing has them
    /// worked out for it. Writing them by hand is then an *override*, for the
    /// case the diary has not been told about, and an explicit empty list is how
    /// a block says it runs through everything.
    ///
    /// The distinction is the one an alteration's slots already make: absent and
    /// empty are different facts, and collapsing them would make every
    /// unadorned document silently claim it runs through the holidays.
    interruptions: Option<Vec<SkipSection>>,
    weekdays: BTreeMap<String, String>,
    /// The starting 1RM. Absent for a test, which produces one rather than
    /// reading one — that being the whole of what a test does.
    #[serde(default)]
    anchor: Option<AnchorSection>,
    /// Where the ladder opens, stated rather than derived.
    ///
    /// Absent derives it from the anchor's entry test. Present is the answer
    /// for a block the derivation cannot reach — one picked up mid-flight, or
    /// one starting far enough after its test that nothing off that test is
    /// evidence any more. Not a `TODO` candidate: absent is a real state.
    #[serde(default)]
    opening: Option<String>,
    /// What a test is performed at: a single before a linear programme, a triple
    /// before a block. Absent for anything that is not a test.
    #[serde(default)]
    reps: Option<u32>,
    /// What a test is an attempt at, where it cannot be inherited.
    ///
    /// **Absent is the ordinary case**, and means the target comes from the
    /// programme before it as the record stands (decision 0011). Stating one is
    /// for a test with nothing before it in the same lift — a front squat
    /// maximum is not evidence about an RDL — and not a default, because a
    /// number written here does not move when the record does.
    #[serde(default)]
    target: Option<String>,
    /// The week a block spends measuring the maximum it plans from.
    ///
    /// **Absent means the block opens from a test that already happened**, and
    /// its anchor must then say `provenance = "tested"`. Present means the block
    /// measures its own: the anchor becomes what the operator expects, this week
    /// finds out, and a result that differs is answered by re-authoring.
    ///
    /// It never changes what `duration_weeks` means. That number counts phase
    /// weeks whether this is here or not, and the calendar carries one more when
    /// it is — which is the fork decision 0013 removed from the linear template,
    /// kept out of this one.
    #[serde(default)]
    entry_test: Option<EntryTestSection>,
}

/// A block's entry-test week.
#[derive(serde::Deserialize)]
struct EntryTestSection {
    /// What the attempt is performed at. A triple, by convention: a cold maximal
    /// single measures technique as much as strength, and a peaked one is what
    /// the block's realisation weeks prepare for.
    reps: u32,
    /// What the week's other session runs its primary at.
    ///
    /// **Absent means it is not run.** There is no derivation available here —
    /// the lift's maximum is what this week is about to measure — so a number is
    /// stated or the session does not happen. Anything else would be a load
    /// fitted to a record that has not been made yet.
    #[serde(default)]
    light: Option<String>,
}

/// A skip, as a bare date or as a run of days.
///
/// Untagged: `"2026-09-04"` and `{ start = "...", days = 5 }` are already
/// distinguishable by shape, and a tag would be noise in a document a person
/// edits. Both normalise to one `Skip` on the way in, so the domain never sees
/// two spellings of one fact.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SkipSection {
    Day(String),
    Run { start: String, days: u8 },
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
    back_off: BTreeMap<String, BackOffSection>,
    /// One load scale per implement, keyed by the implement's stable name.
    ///
    /// An implement absent from here is not defaulted to the barbell's steps:
    /// anything loaded on it reports as underivable, and the rest of the
    /// session still issues.
    scales: BTreeMap<String, ScaleSection>,
    light_of_heavy: String,
    static_hold: String,
    ladder: LadderSection,
    strength: AccessorySection,
    hypertrophy: AccessorySection,
    roles: BTreeMap<String, RoleSection>,
    warmup: Vec<WarmupSection>,
    reset: ResetSection,
    /// How long to rest, block by block. Keyed by the block's stable name, so a
    /// block the document forgets is named in the error rather than defaulted.
    rest: BTreeMap<String, RestSection>,
}

/// One block's rest, as the document states it.
///
/// `between_sets` is required and `after_superset` is not: a block that rests
/// the same however its work is grouped says so by leaving it out. Both are
/// written the way a duration is written everywhere else here — `"90s"` for one
/// number, `"120s-180s"` for a range.
#[derive(serde::Deserialize)]
struct RestSection {
    between_sets: String,
    after_superset: Option<String>,
}

#[derive(serde::Deserialize)]
struct LadderSection {
    climb_per_week: String,
    /// Negative. What a derived opening drops off the load the entry test
    /// failed. Authored rather than borrowed from the first reset's drop, so
    /// the two agreeing is a decision and not a coincidence nothing pins.
    entry_drop: String,
}

#[derive(serde::Deserialize)]
struct BackOffSection {
    sets: u32,
    reps: u32,
    of_top_set: String,
}

/// One implement's scale: a bare step, or bands of one.
///
/// Untagged, because `"2.5kg"` and a list of bands are already distinguishable
/// by shape and a tag would be noise in a document a person edits.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum ScaleSection {
    /// One step, forever. What a barbell is.
    Uniform(String),
    /// Bands, lightest first. The first must start at nothing.
    Banded(Vec<BandSection>),
}

#[derive(serde::Deserialize)]
struct BandSection {
    from: String,
    size: String,
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
    pub fn parameters(&self) -> Result<Option<GenerationParameters>, DocumentError> {
        let Some(p) = &self.parameters else {
            return Ok(None);
        };

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

        let back_off = |name: &str| -> Result<BackOff, DocumentError> {
            let field = format!("parameters.back_off.{name}");
            let section = p.back_off.get(name).ok_or_else(|| {
                invalid(
                    "parameters.back_off",
                    format!("no {name} session's back-off is described"),
                )
            })?;
            Ok(BackOff {
                sets: reps(&format!("{field}.sets"), section.sets)?,
                reps: reps(&format!("{field}.reps"), section.reps)?,
                of_top_set: percentage(&format!("{field}.of_top_set"), &section.of_top_set)?,
            })
        };

        Ok(Some(GenerationParameters {
            rest: rest_scheme(&p.rest)?,
            warmup,
            back_off: PerRole {
                light: back_off("light")?,
                heavy: back_off("heavy")?,
            },
            light_of_heavy: percentage("parameters.light_of_heavy", &p.light_of_heavy)?,
            ladder_climb_per_week: mass(
                "parameters.ladder.climb_per_week",
                &p.ladder.climb_per_week,
            )?,
            entry_drop: percentage("parameters.ladder.entry_drop", &p.ladder.entry_drop)?,
            top_set_reps: PerRole {
                light: role("light")?,
                heavy: role("heavy")?,
            },
            strength: scheme("parameters.strength", &p.strength)?,
            hypertrophy: scheme("parameters.hypertrophy", &p.hypertrophy)?,
            static_hold: seconds("parameters.static_hold", &p.static_hold)?,
            scales: scales(&p.scales)?,
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
        }))
    }

    /// Whether this document takes its unstated fills from the programme before
    /// it.
    ///
    /// Only a test does. A linear programme and a block state every slot
    /// themselves, and an omission in one of those is a document with a hole in
    /// it rather than an inheritance.
    #[must_use]
    pub fn inherits(&self) -> bool {
        self.programme.template == "test"
    }

    /// The day this programme starts, before anything else about it is read.
    ///
    /// Wanted by the caller ahead of [`Self::programme`], because finding the
    /// programme to inherit from is a question about a date and the store is the
    /// only thing that can answer it.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the start is unsettled or is not a date.
    pub fn start(&self) -> Result<Date, DocumentError> {
        settled("programme.start", &self.programme.start)?
            .parse::<Date>()
            .map_err(|error| invalid("programme.start", error))
    }

    /// The days this programme runs across, first and last.
    ///
    /// Wanted by the caller ahead of [`Self::programme`], for the same reason
    /// [`Self::start`] is: what the gym loses is a question about a span of
    /// dates, and the schedule is the only thing that can answer it.
    ///
    /// **The nominal span, before interruptions**, which is what stops this
    /// being circular: the losses are read from the window, so the window
    /// cannot be read from the losses. `Calendar` refuses an interruption
    /// outside the block, so asking over exactly this span is also what keeps
    /// every derived skip admissible.
    ///
    /// **A limitation, declared rather than solved.** `Calendar::calendar_weeks`
    /// walks the skips, so a week in which *every* session is lost pushes the
    /// block's real end past this span — and a day lost in that extension is not
    /// consulted here. It takes a whole training week going at once to happen,
    /// and the answer when it does is to state the interruptions in the
    /// document, which overrides this entirely.
    ///
    /// `None` where the document does not say how long it runs, which is a
    /// document `programme` will refuse for its own reasons.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] if the start is unsettled or is not a date.
    pub fn window(&self) -> Result<Option<(Date, Date)>, DocumentError> {
        let start = self.start()?;

        // A test is one week; anything else says how many it runs for.
        //
        // **`duration_weeks` counts phase weeks, and an entry test adds one in
        // front of them** (decision 0016). A nine-week block that measures its
        // own entry occupies ten calendar weeks, and asking the schedule over
        // nine would leave the last week unconsulted.
        let weeks = if self.programme.template == "test" {
            1
        } else {
            let Some(weeks) = self.programme.duration_weeks else {
                return Ok(None);
            };
            weeks + u32::from(self.programme.entry_test.is_some())
        };

        let days = i64::from(weeks) * 7 - 1;
        let Ok(last) = start.checked_add(jiff::Span::new().days(days)) else {
            return Ok(None);
        };
        Ok(Some((start, last)))
    }

    /// The programme, in domain terms, validated.
    ///
    /// **`inherited` is the fills of the programme this one follows**, where the
    /// store holds one. A test resolves its own fills over them here, when the
    /// document is read — not when a session is derived. That keeps the stored
    /// test complete on its own, so re-authoring the predecessor later cannot
    /// retroactively move what this test prescribes, and it keeps the domain
    /// ignorant of inheritance entirely (§ 12, § 14). Nothing else reads it: a
    /// linear programme and a block state every slot themselves.
    ///
    /// # Errors
    ///
    /// [`DocumentError`] for an unsettled value, an unknown exercise, a field a
    /// template has no use for or one it cannot do without, or a programme the
    /// consistency checks refuse.
    pub fn programme(
        &self,
        parameters: &GenerationParameters,
        zone: TimeZone,
        inherited: Option<&SlotFills>,
        derived: &[Skip],
    ) -> Result<Programme, DocumentError> {
        let section = &self.programme;

        let mut days = Vec::with_capacity(section.weekdays.len());
        for (day, role) in &section.weekdays {
            days.push((
                weekday(day)?,
                SessionRole::try_from(role.clone())
                    .map_err(|error| invalid("programme.weekdays", error))?,
            ));
        }
        let weekdays = Weekdays::new(days).map_err(|error| invalid("programme.weekdays", error))?;

        // Stated wins; absent takes what the schedule worked out. Resolved here
        // rather than at derivation for the reason a test's inherited fills are:
        // the stored programme is then complete on its own, and a holiday coming
        // off somebody's calendar afterwards cannot move what it prescribed.
        let interruptions: Vec<Skip> = match section.interruptions.as_ref() {
            None => derived.to_vec(),
            Some(stated) => {
                let mut parsed = Vec::with_capacity(stated.len());
                for (at, skip) in stated.iter().enumerate() {
                    let field = format!("programme.interruptions[{at}]");
                    let day = |value: &str| -> Result<Date, DocumentError> {
                        settled(&field, value)?
                            .parse::<Date>()
                            .map_err(|error| invalid(&field, error))
                    };
                    parsed.push(match skip {
                        SkipSection::Day(value) => Skip::day(day(value)?),
                        SkipSection::Run { start, days } => Skip::new(
                            day(start)?,
                            NonZeroU8::new(*days).ok_or_else(|| {
                                invalid(&field, "a skip of no days does not skip anything")
                            })?,
                        ),
                    });
                }
                parsed
            }
        };

        let start = settled("programme.start", &section.start)?
            .parse::<Date>()
            .map_err(|error| invalid("programme.start", error))?;
        let name = ProgrammeName::try_from(settled("programme.name", &section.name)?.to_owned())
            .map_err(|error| invalid("programme.name", error))?;
        let pattern = PrimaryPattern::try_from(section.primary_pattern.clone())
            .map_err(|error| invalid("programme.primary", error))?;
        let primary_exercise = exercise("programme.primary_exercise", &section.primary_exercise)?;

        match section.template.as_str() {
            "test" => self.test(
                pattern,
                primary_exercise,
                name,
                start,
                &interruptions,
                weekdays,
                zone,
                inherited,
            ),
            template @ ("linear" | "block" | "sbs") => self.climbing(
                template,
                pattern,
                primary_exercise,
                name,
                start,
                &interruptions,
                weekdays,
                zone,
                parameters,
            ),
            other => Err(invalid(
                "programme.template",
                format!(
                    "{other:?} is not a template this build can read; it reads \
                     \"linear\" and \"block\", which were \"v1\" and \"v2\" until \
                     2026-08-18, \"sbs\", and \"test\""
                ),
            )),
        }
    }

    /// A standalone test, over the fills of the programme it follows.
    #[expect(
        clippy::too_many_arguments,
        reason = "the caller has already parsed each of these out of the \
                  document, and grouping them into a struct would introduce a \
                  type whose only purpose is to be taken apart again one line \
                  later"
    )]
    fn test(
        &self,
        pattern: PrimaryPattern,
        primary_exercise: Exercise,
        name: ProgrammeName,
        start: Date,
        interruptions: &[Skip],
        weekdays: Weekdays,
        zone: TimeZone,
        inherited: Option<&SlotFills>,
    ) -> Result<Programme, DocumentError> {
        let section = &self.programme;
        let count = section
            .reps
            .ok_or_else(|| invalid("programme.reps", "a test says what it is performed at"))?;
        let target = match section.target.as_deref() {
            None => TestTarget::Inherited,
            Some(load) => TestTarget::Declared(mass("programme.target", load)?),
        };
        Self::refuse_unused(
            "test",
            &[
                ("programme.anchor", section.anchor.is_some()),
                ("programme.opening", section.opening.is_some()),
                ("programme.gating_role", section.gating_role.is_some()),
                ("programme.duration_weeks", section.duration_weeks.is_some()),
            ],
        )?;
        let calendar = Test::week(start, interruptions, weekdays, zone)?;
        Ok(Programme::Test(Test::new(
            name,
            Tested::new(pattern, primary_exercise, reps("programme.reps", count)?),
            self.fills_over(inherited)?,
            calendar,
            target,
        )?))
    }

    /// A programme that climbs: linear or block.
    #[expect(
        clippy::too_many_arguments,
        reason = "as `Self::test`, and for the same reason"
    )]
    fn climbing(
        &self,
        template: &str,
        pattern: PrimaryPattern,
        primary_exercise: Exercise,
        name: ProgrammeName,
        start: Date,
        interruptions: &[Skip],
        weekdays: Weekdays,
        zone: TimeZone,
        parameters: &GenerationParameters,
    ) -> Result<Programme, DocumentError> {
        let section = &self.programme;
        let anchor = self.anchor()?;
        let declared_opening = section
            .opening
            .as_deref()
            .map(|load| mass("programme.opening", load))
            .transpose()?;
        // **Before the gating requirement, because the chart answers it.** Every
        // other climbing template genuinely needs to be told which session
        // advances it; an SBS cycle's second session is the repetition-maximum
        // day by construction, so asking would be asking for a settled number.
        if template == "sbs" {
            let calendar = Calendar::new(start, WEEKS, interruptions, weekdays, zone)?;
            return self.sbs(name, pattern, primary_exercise, anchor, calendar);
        }

        let gating = section.gating_role.as_ref().ok_or_else(|| {
            invalid(
                "programme.gating_role",
                "a programme that climbs says which session advances it",
            )
        })?;
        let primary = Primary::new(
            pattern,
            primary_exercise,
            SessionRole::try_from(gating.clone())
                .map_err(|error| invalid("programme.gating_role", error))?,
        );

        let duration = section.duration_weeks.ok_or_else(|| {
            invalid(
                "programme.duration_weeks",
                "a programme that climbs says for how long",
            )
        })?;
        Self::refuse_unused(
            template,
            &[
                ("programme.reps", section.reps.is_some()),
                ("programme.target", section.target.is_some()),
            ],
        )?;
        let entry_test = section
            .entry_test
            .as_ref()
            .map(|test| {
                let light = test
                    .light
                    .as_deref()
                    .map(|load| mass("programme.entry_test.light", load))
                    .transpose()?;
                EntryTest::new(reps("programme.entry_test.reps", test.reps)?, light)
                    .map_err(|error| invalid("programme.entry_test", error))
            })
            .transpose()?;
        let fills = self.fills()?;

        if template == "linear" {
            // A linear programme has neither an entry test nor an exit one
            // (decision 0013), so a document naming one has the wrong template
            // rather than a spare field.
            Self::refuse_unused("linear", &[("programme.entry_test", entry_test.is_some())])?;
            let calendar = Calendar::new(start, duration, interruptions, weekdays, zone)?;
            Ok(Programme::Periodisation(Periodisation::Linear(
                Linear::new(
                    name,
                    primary,
                    fills,
                    Entry::new(anchor, declared_opening),
                    calendar,
                    parameters,
                )?,
            )))
        } else {
            // A block's loads are shares of its anchor, so there is no
            // opening for a document to declare and none to derive.
            Self::refuse_unused("block", &[("programme.opening", section.opening.is_some())])?;
            // `duration_weeks` counts phase weeks; an entry test adds a week in
            // front of them, which is why the calendar is built rather than
            // taken from that number directly.
            let calendar = Periodised::weeks(
                start,
                duration,
                entry_test.is_some(),
                interruptions,
                weekdays,
                zone,
            )?;
            Ok(Programme::Periodisation(Periodisation::Block(
                Periodised::new(
                    name,
                    primary,
                    fills,
                    Entry::derived(anchor),
                    entry_test,
                    calendar,
                )?,
            )))
        }
    }

    /// The maximum a climbing programme opens from.
    ///
    /// Its own method because all three climbing templates need it identically,
    /// and because `climbing` outgrew what fits in one read.
    fn anchor(&self) -> Result<Anchor, DocumentError> {
        let section = self.programme.anchor.as_ref().ok_or_else(|| {
            invalid(
                "programme.anchor",
                "a programme that climbs opens from a maximum",
            )
        })?;
        let failed = section
            .failed
            .as_deref()
            .map(|value| mass("programme.anchor.failed", value))
            .transpose()?;
        Anchor::new(
            mass("programme.anchor.load", &section.load)?,
            failed,
            AnchorProvenance::try_from(section.provenance.clone())
                .map_err(|error| invalid("programme.anchor.provenance", error))?,
            settled("programme.anchor.from", &section.from)?
                .parse::<Date>()
                .map_err(|error| invalid("programme.anchor.from", error))?,
        )
        .map_err(|error| invalid("programme.anchor", error))
    }

    /// An SBS cycle.
    ///
    /// **It says nothing about duration and must not be asked to.** The chart is
    /// four weeks; a document stating that would be stating the obvious, and one
    /// stating anything else would describe a programme this build cannot
    /// prescribe. Every other climbing template genuinely needs the number, so
    /// the requirement stays for them and this branch runs before it.
    ///
    /// The same goes for an opening — every load is a share of the maximum, so
    /// there is nothing to open against — and for an entry test, since the
    /// chart's test is its last session rather than a week in front.
    fn sbs(
        &self,
        name: ProgrammeName,
        pattern: PrimaryPattern,
        exercise: Exercise,
        anchor: Anchor,
        calendar: Calendar,
    ) -> Result<Programme, DocumentError> {
        let section = &self.programme;
        Self::refuse_unused(
            "sbs",
            &[
                ("programme.duration_weeks", section.duration_weeks.is_some()),
                ("programme.opening", section.opening.is_some()),
                ("programme.entry_test", section.entry_test.is_some()),
                // **The chart says which session advances the cycle**, so this
                // is a field with nothing to decide. See `sbs::programme::GATING`.
                ("programme.gating_role", section.gating_role.is_some()),
            ],
        )?;
        Ok(Programme::Periodisation(Periodisation::Sbs(Sbs::new(
            name,
            pattern,
            exercise,
            self.fills()?,
            Entry::derived(anchor),
            calendar,
        )?)))
    }

    /// Refuse a field this template has no use for.
    ///
    /// **Refused rather than ignored.** A `gating_role` on a test is not a
    /// harmless extra line: it is the operator believing something about this
    /// programme that is not true of it, and reading past it silently is how a
    /// document and what it authors drift apart.
    fn refuse_unused(template: &str, fields: &[(&str, bool)]) -> Result<(), DocumentError> {
        for (field, present) in fields {
            if *present {
                return Err(invalid(
                    field,
                    format!("a {template} programme has no use for this"),
                ));
            }
        }
        Ok(())
    }

    /// Every slot's fill, stated in full.
    fn fills(&self) -> Result<SlotFills, DocumentError> {
        self.fills_over(None)
    }

    /// Every slot's fill, over whatever the previous programme filled it with.
    ///
    /// **The result is total either way.** `SlotFills` has a field per slot and
    /// no way to be partial, which is the point: what a test inherits is
    /// resolved here and nothing downstream ever sees a gap. A slot the document
    /// states wins; a slot it omits falls back; a slot neither answers is the
    /// same error it has always been.
    fn fills_over(&self, inherited: Option<&SlotFills>) -> Result<SlotFills, DocumentError> {
        Ok(SlotFills {
            plyometric: self.statics_over("plyometric", inherited.map(|f| &f.plyometric))?,
            power: self.statics_over("power", inherited.map(|f| &f.power))?,
            knee_dominant: self
                .single_over("knee_dominant", inherited.map(|f| &f.knee_dominant))?,
            upper_push: self.single_over("upper_push", inherited.map(|f| &f.upper_push))?,
            upper_pull: self.single_over("upper_pull", inherited.map(|f| &f.upper_pull))?,
            hip_dominant: self.single_over("hip_dominant", inherited.map(|f| &f.hip_dominant))?,
            biceps: self.single_over("biceps", inherited.map(|f| &f.biceps))?,
            triceps: self.single_over("triceps", inherited.map(|f| &f.triceps))?,
            wrist_flexion: self
                .single_over("wrist_flexion", inherited.map(|f| &f.wrist_flexion))?,
            wrist_extension: self
                .single_over("wrist_extension", inherited.map(|f| &f.wrist_extension))?,
            core: self.single_over("core", inherited.map(|f| &f.core))?,
            handstand_hold: self
                .single_over("handstand_hold", inherited.map(|f| &f.handstand_hold))?,
            dead_hang: self.single_over("dead_hang", inherited.map(|f| &f.dead_hang))?,
            hip_flexor_stretch: self.single_over(
                "hip_flexor_stretch",
                inherited.map(|f| &f.hip_flexor_stretch),
            )?,
            hip_external_rotator_stretch: self.single_over(
                "hip_external_rotator_stretch",
                inherited.map(|f| &f.hip_external_rotator_stretch),
            )?,
            hamstring_stretch: self
                .single_over("hamstring_stretch", inherited.map(|f| &f.hamstring_stretch))?,
            groin_stretch: self
                .single_over("groin_stretch", inherited.map(|f| &f.groin_stretch))?,
        })
    }

    fn statics_over(
        &self,
        slot: &str,
        inherited: Option<&Fill<StaticFill>>,
    ) -> Result<Fill<StaticFill>, DocumentError> {
        match inherited {
            Some(fill) if !self.fills.contains_key(slot) => Ok(fill.clone()),
            _ => self.statics(slot),
        }
    }

    fn single_over(
        &self,
        slot: &str,
        inherited: Option<&Fill<Exercise>>,
    ) -> Result<Fill<Exercise>, DocumentError> {
        match inherited {
            Some(fill) if !self.fills.contains_key(slot) => Ok(fill.clone()),
            _ => self.single(slot),
        }
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
    // **The document names two bounds and the domain holds a span.** A stated
    // range is exactly the place an inversion gets written down, so this is the
    // boundary where `4-6` is checked and `6-4` is refused — the type behind it
    // cannot express either mistake.
    let reps_target = Target::between(count(low)?, count(high)?)
        .ok_or_else(|| invalid(field, "a range runs low-high and must span"))?;
    Ok(AccessoryScheme {
        reps: reps_target,
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
