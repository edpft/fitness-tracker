//! The gym mesocycles of an authored plan (§ 12).
//!
//! Written once and kept, superseded by the plan's `authored_at` like the
//! parameters beside it. A plan is a record of intent, so nothing regenerates it
//! and nothing replaces it wholesale.
//!
//! **Identity is the plan's** (issue #86). A mesocycle has no name and no
//! authoring time of its own: it is the *n*th gym mesocycle of a plan, and
//! re-authoring that plan supersedes every one of them at once. What this module
//! reads is therefore always joined to the plan in force under its name.
//!
//! **Reading uses `rehydrate`, not `new`.** The consistency checks that depend
//! on nothing but the programme are re-run, because a row edited by hand should
//! be caught. Linear's ladder check is not: its span comes from the parameters,
//! so on read it would be asserting that this programme's duration works with
//! whatever span is in force *now* — not a property of the stored programme.
//! Leaving it out is also what lets this store answer without reading another
//! one, so a programme can still be shown when the parameters are what is
//! broken.
//!
//! **One table, three templates, and the row shape is conditional.** A test has
//! no anchor and no gating role and does have a repetition count; migration 0016
//! makes each column's presence a `CHECK` on the template rather than leaving
//! them all nullable, so a half-formed row cannot be written and this module
//! reads what the template promises. Where it does not find it, the row is
//! corrupt rather than merely unexpected, which is the same verdict a failed
//! consistency check gets.

use std::num::NonZeroU8;

use application::{MesocycleStore, StoreError};
use domain::{
    gym::exercise::Exercise,
    measure::{Kg, RepCount},
    normalised::OperatorZone,
    plan::{Occupies, PlanName},
    prescription::{
        Anchor, AnchorProvenance, BlockPeriodisation, ByIntensity, Calendar, Linear, Mesocycle,
        MesocycleId, Progression, Sbs, Skip, SlotId, Test, Tested,
        block::EntryTest,
        linear::{Fill, Primary, PrimaryPattern, SlotFills, StaticFill},
    },
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
    schedule::{Discipline, Relative, SessionRole},
};
use jiff::civil::{Date, Weekday};
use sqlx::SqlitePool;

use super::{corrupt, store_error};

/// A weekday's stable key.
///
/// `jiff::civil::Weekday` has no text form we own, so the mapping is written out
/// rather than derived from `Debug` — a `Debug` representation is not a stable
/// key, and this one is persisted.
pub(super) const fn weekday_key(day: Weekday) -> &'static str {
    match day {
        Weekday::Monday => "monday",
        Weekday::Tuesday => "tuesday",
        Weekday::Wednesday => "wednesday",
        Weekday::Thursday => "thursday",
        Weekday::Friday => "friday",
        Weekday::Saturday => "saturday",
        Weekday::Sunday => "sunday",
    }
}

pub(super) fn weekday_of(key: &str) -> Result<Weekday, StoreError> {
    match key {
        "monday" => Ok(Weekday::Monday),
        "tuesday" => Ok(Weekday::Tuesday),
        "wednesday" => Ok(Weekday::Wednesday),
        "thursday" => Ok(Weekday::Thursday),
        "friday" => Ok(Weekday::Friday),
        "saturday" => Ok(Weekday::Saturday),
        "sunday" => Ok(Weekday::Sunday),
        other => Err(corrupt(&format!("{other:?} is not a weekday"))),
    }
}

/// One fill row, flattened.
///
/// `intensity` is `None` where the slot does not alternate. It is an intensity
/// rather than a whole role because that is what `ByIntensity` is keyed on:
/// what fills a slot differs between the gym's two sessions because one is
/// heavier, not because one is longer.
struct FillRow {
    slot: SlotId,
    intensity: Option<Relative>,
    exercise: Exercise,
    /// Present only for a static slot, which carries its whole prescription.
    statics: Option<(RepCount, RepCount)>,
}

/// The fills for one slot, grouped out of the flat rows.
///
/// A slot is either the same on both sessions or one per role, and either single
/// or a superset. Four combinations, and the template fixes which two apply to
/// each slot — so this assembles what it finds and the caller checks it against
/// the shape the slot must have.
#[derive(Default)]
struct SlotRows {
    same: Vec<Exercise>,
    light: Vec<Exercise>,
    heavy: Vec<Exercise>,
    same_static: Vec<StaticFill>,
    light_static: Vec<StaticFill>,
    heavy_static: Vec<StaticFill>,
}

impl SlotRows {
    /// A single-exercise fill.
    fn single(&self, slot: SlotId) -> Result<Fill<Exercise>, StoreError> {
        if !self.same.is_empty() {
            let [only] = self.same.as_slice() else {
                return Err(corrupt(&format!(
                    "slot {slot} is single but holds {} exercises",
                    self.same.len()
                )));
            };
            return Ok(Fill::Same(*only));
        }
        let ([light], [heavy]) = (self.light.as_slice(), self.heavy.as_slice()) else {
            return Err(corrupt(&format!(
                "slot {slot} alternates but does not hold one exercise per role"
            )));
        };
        Ok(Fill::Alternating(ByIntensity {
            lower: *light,
            higher: *heavy,
        }))
    }

    /// A statically prescribed fill.
    fn statics(&self, slot: SlotId) -> Result<Fill<StaticFill>, StoreError> {
        if let [only] = self.same_static.as_slice() {
            return Ok(Fill::Same(*only));
        }
        let ([light], [heavy]) = (self.light_static.as_slice(), self.heavy_static.as_slice())
        else {
            return Err(corrupt(&format!(
                "slot {slot} is static and does not hold one prescription per role"
            )));
        };
        Ok(Fill::Alternating(ByIntensity {
            lower: *light,
            higher: *heavy,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct SqliteGymMesocycleStore {
    pool: SqlitePool,
    /// The zone the operator declares they train in.
    ///
    /// Configuration rather than programme data, so it is supplied here and not
    /// read from a row (§ II.3). The calendar needs one to answer "today", and
    /// answering it in UTC is how a session lands on the wrong day for anyone
    /// who trains in the evening — or, in a zone ahead of UTC, first thing in
    /// the morning.
    zone: OperatorZone,
}

impl SqliteGymMesocycleStore {
    pub const fn new(pool: SqlitePool, zone: OperatorZone) -> Self {
        Self { pool, zone }
    }
}

/// Every gym mesocycle of every plan in force, earliest start first.
///
/// **The plan in force under a name is its latest authoring**, and a mesocycle
/// belonging to a superseded one is not read at all — which is what makes
/// re-authoring the autumn legal without deleting anything.
///
/// **One query for two readers.** The plan store groups these by plan and the
/// mesocycle store reads them flat, and a second query filtered by plan id would
/// be the same joins written twice with two chances to disagree about which
/// authoring is current.
pub(super) async fn in_force(
    pool: &SqlitePool,
    zone: &OperatorZone,
) -> Result<Vec<(i64, PlanName, MesocycleId, Mesocycle)>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT m.id AS "id!: i64", m.plan AS "plan!: i64",
               pl.name AS "plan_name!: String",
               m.template AS "template!: String",
               m.provider AS "provider: String",
               m.provided_programme AS "provided_programme: String",
               m.primary_pattern AS "primary_pattern!: String",
               m.primary_exercise AS "primary_exercise!: String",
               m.asserted_grams AS "asserted_grams: i64",
               m.asserted_provenance AS "asserted_provenance: String",
               m.asserted_from AS "asserted_from: String",
               m.asserted_failed_grams AS "asserted_failed_grams: i64",
               m.gating_intensity AS "gating_intensity: String",
               m.gating_volume AS "gating_volume: String",
               m.start_date AS "start_date!: String",
               m.duration_weeks AS "duration_weeks!: i64",
               m.test_reps AS "test_reps: i64",
               m.entry_test_reps AS "entry_test_reps: i64",
               m.entry_test_light_grams AS "entry_test_light_grams: i64"
        FROM gym_mesocycle AS m
        JOIN plan AS pl ON pl.id = m.plan
        WHERE pl.authored_at = (
            SELECT MAX(q.authored_at) FROM plan AS q WHERE q.name = pl.name
        )
        ORDER BY m.start_date ASC, m.id ASC
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut mesocycles = Vec::with_capacity(rows.len());
    for row in rows {
        let fills = read_fills(pool, row.id).await?;
        let interruptions = read_interruptions(pool, row.id).await?;

        let start = row
            .start_date
            .parse::<Date>()
            .map_err(|_| corrupt(&"a start date that is not a date"))?;
        // **Read from the schedule, not from the mesocycle.** `gym_weekday`
        // held a copy of this per row until 2026-09-20 (issue #63); the week
        // belongs to the operator, and a block is rebuilt against the one in
        // force when it started.
        let week = super::schedule::training_week(pool, start, Discipline::Gym)
            .await?
            .ok_or_else(|| {
                corrupt(&format!(
                    "the schedule gives the gym no day of the week as of {start}, \
                     so a block starting then has nothing to run on"
                ))
            })?;

        let duration = u32::try_from(row.duration_weeks)
            .map_err(|_| corrupt(&"a duration the domain cannot hold"))?;
        let calendar = Calendar::new(start, duration, &interruptions, week, zone.as_time_zone())
            .map_err(|error| corrupt(&error))?;

        let plan = PlanName::try_from(row.plan_name).map_err(|error| corrupt(&error))?;
        let pattern =
            PrimaryPattern::try_from(row.primary_pattern).map_err(|error| corrupt(&error))?;
        let exercise = exercise_of(&row.primary_exercise)?;
        let provided = read_provided(pool, row.id, row.provider, row.provided_programme).await?;

        let common = Common {
            pattern,
            exercise,
            fills,
            calendar,
            provided,
        };
        let mesocycle = match row.template.as_str() {
            "test" => rehydrate_test(
                common,
                row.test_reps,
                read_anchor(
                    row.asserted_grams,
                    row.asserted_failed_grams,
                    row.asserted_provenance,
                    row.asserted_from,
                )?,
            )?,
            template @ ("linear" | "block" | "sbs") => rehydrate_periodisation(
                common,
                template,
                gating_of(row.gating_intensity, row.gating_volume)?,
                read_entry_test(
                    row.entry_test_reps,
                    row.entry_test_light_grams,
                    read_anchor(
                        row.asserted_grams,
                        row.asserted_failed_grams,
                        row.asserted_provenance,
                        row.asserted_from,
                    )?,
                )?,
            )?,
            other => {
                return Err(corrupt(&format!(
                    "{other:?} is not a template this build can read"
                )));
            }
        };

        mesocycles.push((row.plan, plan, MesocycleId::new(row.id), mesocycle));
    }
    Ok(mesocycles)
}

/// Which microcycles of which published programme a mesocycle is.
///
/// **Both columns or neither** — a `CHECK` says so from the other side — and the
/// microcycle numbers are rows of their own, because a mesocycle taking µ1-2-4-5
/// is four facts rather than a string to be parsed back.
async fn read_provided(
    pool: &SqlitePool,
    mesocycle: i64,
    provider: Option<String>,
    programme: Option<String>,
) -> Result<Option<ProvidedFrom>, StoreError> {
    let (Some(provider), Some(programme)) = (provider, programme) else {
        return Ok(None);
    };
    let published = ExternalProgramme::new(
        Provider::try_from(provider).map_err(|error| corrupt(&error))?,
        ProgrammeName::try_from(programme).map_err(|error| corrupt(&error))?,
    );

    let rows = sqlx::query!(
        r#"
        SELECT microcycle AS "microcycle!: i64"
        FROM gym_microcycle
        WHERE mesocycle = ?
        ORDER BY position ASC
        "#,
        mesocycle
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut microcycles = Vec::with_capacity(rows.len());
    for row in rows {
        microcycles.push(
            u32::try_from(row.microcycle)
                .map_err(|_| corrupt(&"a published microcycle number the domain cannot hold"))?,
        );
    }
    ProvidedFrom::new(published, microcycles)
        .map(Some)
        .map_err(|error| corrupt(&error))
}

/// The columns whose presence depends on the template.
///
/// **Every `None` here is an absence, not a default.** Migration 0016 says the
/// same thing from the other side: what a row must carry is decided by what kind
/// of programme it is, so a test with an anchor and a linear programme without
/// one are both refused by the database as well as unrepresentable here.
struct Columns {
    asserted_grams: Option<i64>,
    provenance: Option<&'static str>,
    asserted_from: Option<String>,
    asserted_failed: Option<i64>,
    gating_intensity: Option<&'static str>,
    gating_volume: Option<&'static str>,
    test_reps: Option<i64>,
    entry_test_reps: Option<i64>,
    entry_test_light: Option<i64>,
}

fn columns_of(programme: &Mesocycle) -> Result<Columns, StoreError> {
    let entry_test = match programme {
        Mesocycle::Progression(Progression::BlockPeriodisation(block)) => block.entry_test(),
        // An SBS cycle has no entry test: its test is the last session of the
        // last week, not a week in front (decision 0024).
        Mesocycle::Progression(Progression::Linear(_) | Progression::Provided { .. })
        | Mesocycle::Test(_) => None,
    };
    let entry_test_light = entry_test
        .and_then(EntryTest::light)
        .map(|load| {
            i64::try_from(load.as_grams())
                .map_err(|_| corrupt(&"a light load larger than the store can hold"))
        })
        .transpose()?;

    // **Only an entry test carries one, and only the one at the front of a
    // sequence.** Every other week's anchor is a function of a test that
    // happened and is read off the record when a session is asked for; this is
    // the seed, which nothing can derive because nothing came before it. A
    // standalone test and a block that measures its own entry are the same case
    // in two shapes.
    let asserted = match programme {
        Mesocycle::Test(test) => test.asserted(),
        Mesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            block.entry_test().and_then(EntryTest::asserted)
        }
        Mesocycle::Progression(Progression::Linear(_) | Progression::Provided { .. }) => None,
    };
    let test_reps = match programme {
        Mesocycle::Test(test) => Some(i64::from(test.reps().as_u32())),
        Mesocycle::Progression(_) => None,
    };

    Ok(Columns {
        asserted_grams: asserted
            .map(|anchor| {
                i64::try_from(anchor.load().as_grams())
                    .map_err(|_| corrupt(&"an anchor larger than the store can hold"))
            })
            .transpose()?,
        provenance: asserted.map(|anchor| anchor.provenance().as_str()),
        asserted_from: asserted.map(|anchor| anchor.from().to_string()),
        asserted_failed: asserted
            .and_then(Anchor::failed)
            .map(|failed| {
                i64::try_from(failed.as_grams())
                    .map_err(|_| corrupt(&"a failed load larger than the store can hold"))
            })
            .transpose()?,
        gating_intensity: programme
            .gating_role()
            .map(|role| role.intensity().as_str()),
        gating_volume: programme.gating_role().map(|role| role.volume().as_str()),
        test_reps,
        entry_test_reps: entry_test.map(|test| i64::from(test.reps().as_u32())),
        entry_test_light,
    })
}

/// What every template's row carries, once parsed.
///
/// A struct rather than seven arguments: the two rehydrations below take all of
/// it and differ only in what they take *besides* it.
struct Common {
    pattern: PrimaryPattern,
    exercise: Exercise,
    fills: SlotFills,
    calendar: Calendar,
    provided: Option<ProvidedFrom>,
}

/// A test, from the two columns only a test carries.
fn rehydrate_test(
    common: Common,
    test_reps: Option<i64>,
    asserted: Option<Anchor>,
) -> Result<Mesocycle, StoreError> {
    let reps = test_reps.ok_or_else(|| corrupt(&"a test with no repetition count"))?;
    let reps = u32::try_from(reps)
        .ok()
        .and_then(|count| RepCount::new(count).ok())
        .ok_or_else(|| corrupt(&"a test at no repetitions"))?;
    // Null is the ordinary case: a test with something behind it defers to
    // whatever that measured, which is read off the record when a session is
    // asked for. A number here is the seed at the front of a sequence.
    Ok(Mesocycle::Test(
        Test::rehydrate(
            Tested::new(common.pattern, common.exercise, reps),
            common.fills,
            common.calendar,
            common.provided,
            asserted,
        )
        .map_err(|error| corrupt(&error))?,
    ))
}

/// A programme that climbs, by whichever of the three models.
fn rehydrate_periodisation(
    common: Common,
    template: &str,
    gating: Option<SessionRole>,
    entry_test: Option<EntryTest>,
) -> Result<Mesocycle, StoreError> {
    let gating =
        gating.ok_or_else(|| corrupt(&"a programme that climbs with nothing gating it"))?;
    let primary = Primary::new(common.pattern, common.exercise, gating);
    if template == "sbs" {
        // A `CHECK` refuses an `sbs` row without a provider, so a `None` here is
        // a row that got past the database rather than a state to default.
        let from = common
            .provided
            .ok_or_else(|| corrupt(&"a provided cycle that names no programme"))?;
        // `stored` rather than `new`: the checks ran when it was written, and
        // re-refusing a row now would make a rule change unreadable data.
        return Ok(Mesocycle::Progression(Progression::Provided {
            from,
            cycle: Sbs::stored(
                common.pattern,
                common.exercise,
                common.fills,
                common.calendar,
            ),
        }));
    }

    Ok(Mesocycle::Progression(if template == "linear" {
        Progression::Linear(
            Linear::rehydrate(primary, common.fills, common.calendar)
                .map_err(|error| corrupt(&error))?,
        )
    } else {
        Progression::BlockPeriodisation(
            BlockPeriodisation::rehydrate(primary, common.fills, entry_test, common.calendar)
                .map_err(|error| corrupt(&error))?,
        )
    }))
}

/// A block's entry-test week, where it has one.
///
/// The light load is null for a week that runs only its test, which is a real
/// state rather than a missing value: there is nothing to derive a light load
/// from when the lift's maximum is what the week is about to measure.
fn read_entry_test(
    reps: Option<i64>,
    light_grams: Option<i64>,
    asserted: Option<Anchor>,
) -> Result<Option<EntryTest>, StoreError> {
    let Some(reps) = reps else {
        return Ok(None);
    };
    let reps = u32::try_from(reps)
        .ok()
        .and_then(|count| RepCount::new(count).ok())
        .ok_or_else(|| corrupt(&"an entry test at no repetitions"))?;
    let light = light_grams
        .map(|grams| {
            u64::try_from(grams)
                .map(Kg::from_grams)
                .map_err(|_| corrupt(&"a light load stored as a negative mass"))
        })
        .transpose()?;
    Ok(Some(
        EntryTest::new(reps, light, asserted).map_err(|error| corrupt(&error))?,
    ))
}

/// The anchor a test asserts, from the four columns only a test may carry.
///
/// **`None` is the ordinary case**, and it is a statement rather than a gap: a
/// test with something behind it defers to whatever that measured, and what it
/// resolves to is read off the record when a session is asked for. A number is
/// the seed at the front of a sequence, where nothing came before to measure
/// one.
fn read_anchor(
    grams: Option<i64>,
    failed_grams: Option<i64>,
    provenance: Option<String>,
    from: Option<String>,
) -> Result<Option<Anchor>, StoreError> {
    let Some(grams) = grams else {
        return Ok(None);
    };
    let load = u64::try_from(grams)
        .map(Kg::from_grams)
        .map_err(|_| corrupt(&"an anchor stored as a negative mass"))?;
    let failed = failed_grams
        .map(|grams| {
            u64::try_from(grams)
                .map(Kg::from_grams)
                .map_err(|_| corrupt(&"a failed load stored as a negative mass"))
        })
        .transpose()?;
    let provenance = provenance.ok_or_else(|| corrupt(&"an anchor from nowhere"))?;
    let from = from
        .ok_or_else(|| corrupt(&"an anchor with no date"))?
        .parse::<Date>()
        .map_err(|_| corrupt(&"an anchor date that is not a date"))?;
    let anchor = Anchor::new(
        load,
        failed,
        AnchorProvenance::try_from(provenance).map_err(|error| corrupt(&error))?,
        from,
    )
    .map_err(|error| corrupt(&error))?;

    Ok(Some(anchor))
}

impl MesocycleStore for SqliteGymMesocycleStore {
    async fn on(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, Mesocycle)>, StoreError> {
        Ok(in_force(&self.pool, &self.zone)
            .await?
            .into_iter()
            .find(|(_, _, _, mesocycle)| mesocycle.span().covers(date))
            .map(|(_, plan, id, mesocycle)| (id, plan, mesocycle)))
    }

    async fn preceding(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, Mesocycle)>, StoreError> {
        // The latest mesocycle that has finished by this date. `in_force` is
        // ordered by start, so the last one whose span ends at or before the
        // date is the one immediately before it.
        Ok(in_force(&self.pool, &self.zone)
            .await?
            .into_iter()
            .rfind(|(_, _, _, mesocycle)| mesocycle.span().end() <= date)
            .map(|(_, plan, id, mesocycle)| (id, plan, mesocycle)))
    }

    async fn following(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, Mesocycle)>, StoreError> {
        // Ordered by start, so the first one beginning after the date is the
        // next in the sequence.
        Ok(in_force(&self.pool, &self.zone)
            .await?
            .into_iter()
            .find(|(_, _, _, mesocycle)| mesocycle.span().start() > date)
            .map(|(_, plan, id, mesocycle)| (id, plan, mesocycle)))
    }
}

/// Write one gym mesocycle of a plan, inside the plan's own transaction.
///
/// **Not a port method.** A mesocycle is not authored on its own since #86: the
/// plan is what is written, and a half-written one is not a state the store
/// should be able to hold — so this is a step of [`PlanStore::author`] rather
/// than an entry point of its own.
///
/// [`PlanStore::author`]: application::PlanStore::author
pub(super) async fn write(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    plan: i64,
    ordinal: i64,
    mesocycle: &Mesocycle,
) -> Result<MesocycleId, StoreError> {
    let template = mesocycle.template();
    let pattern = mesocycle.primary().as_str();
    let primary = mesocycle.primary_exercise().as_str();
    let start = mesocycle.calendar().start().to_string();
    let duration = i64::from(mesocycle.calendar().duration_weeks());
    let provided_from = provided_of(mesocycle);
    let provider = provided_from.map(|from| from.programme().provider().to_string());
    let provided_programme = provided_from.map(|from| from.programme().name().to_string());
    let Columns {
        asserted_grams,
        provenance,
        asserted_from,
        asserted_failed,
        gating_intensity,
        gating_volume,
        test_reps,
        entry_test_reps,
        entry_test_light,
    } = columns_of(mesocycle)?;

    let id = sqlx::query!(
        r#"
        INSERT INTO gym_mesocycle (
            plan, ordinal, provider, provided_programme, template,
            primary_pattern, primary_exercise,
            asserted_grams, asserted_provenance, asserted_from, asserted_failed_grams,
            gating_intensity, gating_volume, start_date, duration_weeks,
            test_reps, entry_test_reps, entry_test_light_grams
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        plan,
        ordinal,
        provider,
        provided_programme,
        template,
        pattern,
        primary,
        asserted_grams,
        provenance,
        asserted_from,
        asserted_failed,
        gating_intensity,
        gating_volume,
        start,
        duration,
        test_reps,
        entry_test_reps,
        entry_test_light
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?
    .id;

    if let Some(from) = provided_from {
        write_microcycles(tx, id, from).await?;
    }

    write_fills(tx, id, mesocycle).await?;
    write_calendar(tx, id, mesocycle.calendar()).await?;
    Ok(MesocycleId::new(id))
}

/// Which of a published programme's microcycles this mesocycle took, in order.
///
/// The position is the order they are ridden in, not the number they carry:
/// *Squat 2x Int* µ5 taken as the entry test is position 0 and microcycle 5.
async fn write_microcycles(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mesocycle: i64,
    from: &ProvidedFrom,
) -> Result<(), StoreError> {
    for (at, microcycle) in from.microcycles().enumerate() {
        let position = i64::try_from(at)
            .map_err(|_| corrupt(&"more microcycles than the store can number"))?;
        let number = i64::from(microcycle);
        sqlx::query!(
            r"
            INSERT INTO gym_microcycle (mesocycle, position, microcycle)
            VALUES (?, ?, ?)
            ",
            mesocycle,
            position,
            number
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// What fills each of the template's slots, flattened to one row per fill.
async fn write_fills(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    mesocycle: &Mesocycle,
) -> Result<(), StoreError> {
    for fill in flatten(mesocycle.fills()) {
        let slot_key = fill.slot.as_str();
        let intensity_key = fill.intensity.map(Relative::as_str);
        let exercise_key = fill.exercise.as_str();
        let (static_sets, static_reps) = fill.statics.map_or((None, None), |(sets, reps)| {
            (
                Some(i64::from(sets.as_u32())),
                Some(i64::from(reps.as_u32())),
            )
        });
        sqlx::query!(
            r"
            INSERT INTO gym_slot_fill (
                mesocycle, slot, intensity, position, exercise, static_sets, static_reps
            )
            -- `position` ordered the members of a supersetted slot. Every slot
            -- now holds one exercise, so it is always zero; the column stays
            -- because dropping it is a migration this change does not need.
            VALUES (?, ?, ?, 0, ?, ?, ?)
            ",
            id,
            slot_key,
            intensity_key,
            exercise_key,
            static_sets,
            static_reps
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// What provided a mesocycle, where anything did.
///
/// A provided progression must say; a derived one — linear, or block
/// periodisation — was provided by nobody and may not claim to be; a test may be
/// either.
const fn provided_of(mesocycle: &Mesocycle) -> Option<&ProvidedFrom> {
    match mesocycle {
        Mesocycle::Progression(Progression::Provided { from, .. }) => Some(from),
        Mesocycle::Test(test) => test.provided(),
        Mesocycle::Progression(Progression::Linear(_) | Progression::BlockPeriodisation(_)) => None,
    }
}

/// Every slot fill for one mesocycle.
///
/// Split out of `current` so that function stays inside the line budget, and
/// because "assemble the fills" is a whole job on its own: the rows are flat, and
/// any of them may alternate by role.
async fn read_fills(pool: &SqlitePool, mesocycle: i64) -> Result<SlotFills, StoreError> {
    let fill_rows = sqlx::query!(
        r#"
        SELECT slot AS "slot!: String", intensity AS "intensity: String",
               exercise AS "exercise!: String",
               static_sets AS "static_sets: i64", static_reps AS "static_reps: i64"
        FROM gym_slot_fill
        WHERE mesocycle = ?
        ORDER BY slot ASC, position ASC
        "#,
        mesocycle
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut parsed = Vec::with_capacity(fill_rows.len());
    for fill in fill_rows {
        parsed.push(FillRow {
            slot: SlotId::try_from(fill.slot).map_err(|error| corrupt(&error))?,
            intensity: match fill.intensity {
                Some(side) => Some(Relative::try_from(side).map_err(|error| corrupt(&error))?),
                None => None,
            },
            exercise: exercise_of(&fill.exercise)?,
            statics: match (fill.static_sets, fill.static_reps) {
                (Some(sets), Some(reps)) => Some((count_of(sets)?, count_of(reps)?)),
                _ => None,
            },
        });
    }

    let mut grouped: std::collections::BTreeMap<SlotId, SlotRows> =
        std::collections::BTreeMap::new();
    for fill in parsed {
        let entry = grouped.entry(fill.slot).or_default();
        if let Some((sets, reps)) = fill.statics {
            let fixed = StaticFill {
                exercise: fill.exercise,
                sets,
                reps,
            };
            match fill.intensity {
                None => entry.same_static.push(fixed),
                Some(Relative::Lower) => entry.light_static.push(fixed),
                Some(Relative::Higher) => entry.heavy_static.push(fixed),
            }
            continue;
        }
        match fill.intensity {
            None => entry.same.push(fill.exercise),
            Some(Relative::Lower) => entry.light.push(fill.exercise),
            Some(Relative::Higher) => entry.heavy.push(fill.exercise),
        }
    }
    let rows_for = |slot: SlotId| -> Result<&SlotRows, StoreError> {
        grouped
            .get(&slot)
            .ok_or_else(|| corrupt(&format!("the stored mesocycle has no fill for {slot}")))
    };

    Ok(SlotFills {
        plyometric: rows_for(SlotId::Plyometric)?.statics(SlotId::Plyometric)?,
        power: rows_for(SlotId::Power)?.statics(SlotId::Power)?,
        knee_dominant: rows_for(SlotId::KneeDominant)?.single(SlotId::KneeDominant)?,
        upper_push: rows_for(SlotId::UpperPush)?.single(SlotId::UpperPush)?,
        upper_pull: rows_for(SlotId::UpperPull)?.single(SlotId::UpperPull)?,
        hip_dominant: rows_for(SlotId::HipDominant)?.single(SlotId::HipDominant)?,
        biceps: rows_for(SlotId::Biceps)?.single(SlotId::Biceps)?,
        triceps: rows_for(SlotId::Triceps)?.single(SlotId::Triceps)?,
        wrist_flexion: rows_for(SlotId::WristFlexion)?.single(SlotId::WristFlexion)?,
        wrist_extension: rows_for(SlotId::WristExtension)?.single(SlotId::WristExtension)?,
        core: rows_for(SlotId::Core)?.single(SlotId::Core)?,
        handstand_hold: rows_for(SlotId::HandstandHold)?.single(SlotId::HandstandHold)?,
        dead_hang: rows_for(SlotId::DeadHang)?.single(SlotId::DeadHang)?,
        hip_flexor_stretch: rows_for(SlotId::HipFlexorStretch)?.single(SlotId::HipFlexorStretch)?,
        hip_external_rotator_stretch: rows_for(SlotId::HipExternalRotatorStretch)?
            .single(SlotId::HipExternalRotatorStretch)?,
        hamstring_stretch: rows_for(SlotId::HamstringStretch)?.single(SlotId::HamstringStretch)?,
        groin_stretch: rows_for(SlotId::GroinStretch)?.single(SlotId::GroinStretch)?,
    })
}

/// When the block does not run.
///
/// **Only the interruptions, since 2026-09-20.** The weekdays went with
/// `gym_weekday` (issue #63): which days the gym trains is the schedule's, read
/// back from `training_slot` rather than copied here once per mesocycle. What
/// the block *skipped* stays, because it is what was planned — a holiday coming
/// off the calendar afterwards must not retroactively move what was prescribed
/// (§ 12).
async fn write_calendar(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    mesocycle: i64,
    calendar: &domain::prescription::Calendar,
) -> Result<(), StoreError> {
    for skip in calendar.interruptions().iter() {
        let start = skip.start().to_string();
        let days = i64::from(skip.days().get());
        sqlx::query!(
            r"
            INSERT INTO gym_interruption (mesocycle, start_date, days)
            VALUES (?, ?, ?)
            ",
            mesocycle,
            start,
            days
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    Ok(())
}

/// The weeks the block does not run.
///
/// Ordered by the stored date so a rebuilt programme reads back the same
/// calendar it was authored with, whatever order the rows were written in.
async fn read_interruptions(pool: &SqlitePool, mesocycle: i64) -> Result<Vec<Skip>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT start_date AS "start_date!: String", days AS "days!: i64"
        FROM gym_interruption
        WHERE mesocycle = ?
        ORDER BY start_date ASC
        "#,
        mesocycle
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut skips = Vec::with_capacity(rows.len());
    for row in rows {
        let start = row
            .start_date
            .parse::<Date>()
            .map_err(|_| corrupt(&"an interruption that does not start on a date"))?;
        let days = u8::try_from(row.days)
            .ok()
            .and_then(NonZeroU8::new)
            .ok_or_else(|| corrupt(&"an interruption of no days, which skips nothing"))?;
        skips.push(Skip::new(start, days));
    }
    Ok(skips)
}

/// Our exercise vocabulary, from its stored key.
/// The gating role, from the pair of columns that carry it.
///
/// `None` for a test, which advances nothing and gates on nothing. A row with
/// one half of the pair got past the `CHECK` that ties them together.
fn gating_of(
    intensity: Option<String>,
    volume: Option<String>,
) -> Result<Option<SessionRole>, StoreError> {
    let side = |text: String| Relative::try_from(text).map_err(|error| corrupt(&error));
    match (intensity, volume) {
        (None, None) => Ok(None),
        (Some(intensity), Some(volume)) => {
            Ok(Some(SessionRole::new(side(intensity)?, side(volume)?)))
        }
        _ => Err(corrupt(
            &"a gating role that is half an intensity and half a volume",
        )),
    }
}

fn exercise_of(key: &str) -> Result<Exercise, StoreError> {
    use domain::gym::exercise::{DistanceExercise, DurationExercise, RepsExercise};
    if let Ok(reps) = RepsExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Reps(reps));
    }
    if let Ok(duration) = DurationExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Duration(duration));
    }
    if let Ok(distance) = DistanceExercise::try_from(key.to_owned()) {
        return Ok(Exercise::Distance(distance));
    }
    Err(corrupt(&format!(
        "{key:?} does not name an exercise in the vocabulary"
    )))
}

/// Every fill as a flat row, ready to write.
///
/// Exhaustive over the eleven slots by construction: each is named once, so
/// adding a slot to the template leaves this function failing to compile until it
/// is handled.
fn flatten(fills: &SlotFills) -> Vec<FlatFill> {
    let mut rows = Vec::new();

    let mut statics = |slot: SlotId, fill: &Fill<StaticFill>| {
        let mut push = |intensity, fixed: &StaticFill| {
            rows.push(FlatFill {
                slot,
                intensity,
                exercise: fixed.exercise,
                statics: Some((fixed.sets, fixed.reps)),
            });
        };
        match fill {
            Fill::Same(fixed) => push(None, fixed),
            Fill::Alternating(by_intensity) => {
                push(Some(Relative::Lower), &by_intensity.lower);
                push(Some(Relative::Higher), &by_intensity.higher);
            }
        }
    };
    statics(SlotId::Plyometric, &fills.plyometric);
    statics(SlotId::Power, &fills.power);

    let mut single = |slot: SlotId, fill: &Fill<Exercise>| {
        let mut push = |intensity, exercise| {
            rows.push(FlatFill {
                slot,
                intensity,
                exercise,
                statics: None,
            });
        };
        match fill {
            Fill::Same(exercise) => push(None, *exercise),
            Fill::Alternating(by_intensity) => {
                push(Some(Relative::Lower), by_intensity.lower);
                push(Some(Relative::Higher), by_intensity.higher);
            }
        }
    };
    single(SlotId::KneeDominant, &fills.knee_dominant);
    single(SlotId::UpperPush, &fills.upper_push);
    single(SlotId::UpperPull, &fills.upper_pull);
    single(SlotId::HipDominant, &fills.hip_dominant);
    single(SlotId::Biceps, &fills.biceps);
    single(SlotId::Triceps, &fills.triceps);
    single(SlotId::WristFlexion, &fills.wrist_flexion);
    single(SlotId::WristExtension, &fills.wrist_extension);
    single(SlotId::Core, &fills.core);
    single(SlotId::HandstandHold, &fills.handstand_hold);
    single(SlotId::DeadHang, &fills.dead_hang);
    single(SlotId::HipFlexorStretch, &fills.hip_flexor_stretch);
    single(
        SlotId::HipExternalRotatorStretch,
        &fills.hip_external_rotator_stretch,
    );
    single(SlotId::HamstringStretch, &fills.hamstring_stretch);
    single(SlotId::GroinStretch, &fills.groin_stretch);

    rows
}

/// One fill row, ready to write.
struct FlatFill {
    slot: SlotId,
    intensity: Option<Relative>,
    exercise: Exercise,
    statics: Option<(RepCount, RepCount)>,
}

/// A repetition count as the store holds it.
fn count_of(value: i64) -> Result<RepCount, StoreError> {
    let count = u32::try_from(value).map_err(|_| corrupt(&"a count the domain cannot hold"))?;
    RepCount::new(count).map_err(|error| corrupt(&error))
}
