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
    gym::{Kg, exercise::Exercise},
    measure::RepCount,
    normalised::OperatorZone,
    plan::{Occupies, PlanName},
    prescription::{
        Anchor, AnchorProvenance, Anchoring, BlockPeriodisation, Calendar, Entry, Linear,
        Mesocycle, MesocycleId, PerRole, Progression, Sbs, SessionRole, Skip, SlotId, Test,
        TestTarget, Tested,
        block::EntryTest,
        linear::{Fill, Primary, PrimaryPattern, SlotFills, StaticFill},
    },
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider},
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
/// `role` is `None` where the slot does not alternate.
struct FillRow {
    slot: SlotId,
    role: Option<SessionRole>,
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
        Ok(Fill::Alternating(PerRole {
            light: *light,
            heavy: *heavy,
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
        Ok(Fill::Alternating(PerRole {
            light: *light,
            heavy: *heavy,
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
               m.anchor_grams AS "anchor_grams: i64",
               m.anchor_provenance AS "anchor_provenance: String",
               m.anchor_from AS "anchor_from: String",
               m.anchor_failed_grams AS "anchor_failed_grams: i64",
               m.opening_grams AS "opening_grams: i64",
               m.gating_role AS "gating_role: String",
               m.start_date AS "start_date!: String",
               m.duration_weeks AS "duration_weeks!: i64",
               m.test_reps AS "test_reps: i64",
               m.test_target_grams AS "test_target_grams: i64",
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
        let weekdays = read_weekdays(pool, row.id).await?;
        let interruptions = read_interruptions(pool, row.id).await?;

        let duration = u32::try_from(row.duration_weeks)
            .map_err(|_| corrupt(&"a duration the domain cannot hold"))?;
        let calendar = Calendar::new(
            row.start_date
                .parse::<Date>()
                .map_err(|_| corrupt(&"a start date that is not a date"))?,
            duration,
            &interruptions,
            weekdays,
            zone.as_time_zone(),
        )
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
            "test" => rehydrate_test(common, row.test_reps, row.test_target_grams)?,
            template @ ("linear" | "block" | "sbs") => rehydrate_periodisation(
                common,
                template,
                read_entry(
                    row.anchor_grams,
                    row.anchor_failed_grams,
                    row.anchor_provenance,
                    row.anchor_from,
                    row.opening_grams,
                )?,
                row.gating_role,
                read_entry_test(row.entry_test_reps, row.entry_test_light_grams)?,
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
    anchor_grams: Option<i64>,
    provenance: Option<&'static str>,
    anchor_from: Option<String>,
    anchor_failed: Option<i64>,
    declared_opening: Option<i64>,
    gating: Option<&'static str>,
    test_reps: Option<i64>,
    test_target: Option<i64>,
    entry_test_reps: Option<i64>,
    entry_test_light: Option<i64>,
}

fn columns_of(programme: &Mesocycle) -> Result<Columns, StoreError> {
    let anchor = programme.anchor();
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
    let (test_reps, test_target) = match programme {
        Mesocycle::Test(test) => (
            Some(i64::from(test.reps().as_u32())),
            match test.target() {
                TestTarget::Inherited => None,
                TestTarget::Declared(load) => Some(
                    i64::try_from(load.as_grams())
                        .map_err(|_| corrupt(&"a target larger than the store can hold"))?,
                ),
            },
        ),
        Mesocycle::Progression(_) => (None, None),
    };
    Ok(Columns {
        anchor_grams: anchor
            .map(|anchor| {
                i64::try_from(anchor.load().as_grams())
                    .map_err(|_| corrupt(&"an anchor larger than the store can hold"))
            })
            .transpose()?,
        provenance: anchor.map(|anchor| anchor.provenance().as_str()),
        anchor_from: anchor.map(|anchor| anchor.from().to_string()),
        anchor_failed: anchor
            .and_then(Anchor::failed)
            .map(|failed| {
                i64::try_from(failed.as_grams())
                    .map_err(|_| corrupt(&"a failed load larger than the store can hold"))
            })
            .transpose()?,
        // Only a linear programme may declare one: a block's loads are shares of
        // its anchor, and a test has no ladder to open.
        declared_opening: match programme {
            Mesocycle::Progression(Progression::Linear(linear)) => linear.declared_opening(),
            // Nor may SBS: every load in the chart is a share of the maximum,
            // so there is no opening for one to be declared against.
            Mesocycle::Progression(
                Progression::BlockPeriodisation(_) | Progression::Provided { .. },
            )
            | Mesocycle::Test(_) => None,
        }
        .map(|opening| {
            i64::try_from(opening.as_grams())
                .map_err(|_| corrupt(&"an opening larger than the store can hold"))
        })
        .transpose()?,
        gating: programme.gating_role().map(SessionRole::as_str),
        test_reps,
        test_target,
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
    test_target_grams: Option<i64>,
) -> Result<Mesocycle, StoreError> {
    let reps = test_reps.ok_or_else(|| corrupt(&"a test with no repetition count"))?;
    let reps = u32::try_from(reps)
        .ok()
        .and_then(|count| RepCount::new(count).ok())
        .ok_or_else(|| corrupt(&"a test at no repetitions"))?;
    // Null is the ordinary case and means inherited: the target moves as the
    // record does (decision 0011), so storing one is what a test with nothing to
    // inherit from does.
    let target = match test_target_grams {
        None => TestTarget::Inherited,
        Some(grams) => TestTarget::Declared(
            u64::try_from(grams)
                .map(Kg::from_grams)
                .map_err(|_| corrupt(&"a target stored as a negative mass"))?,
        ),
    };
    Ok(Mesocycle::Test(
        Test::rehydrate(
            Tested::new(common.pattern, common.exercise, reps),
            common.fills,
            common.calendar,
            target,
            common.provided,
        )
        .map_err(|error| corrupt(&error))?,
    ))
}

/// A programme that climbs, by whichever of the three models.
fn rehydrate_periodisation(
    common: Common,
    template: &str,
    entry: Option<Entry>,
    gating_role: Option<String>,
    entry_test: Option<EntryTest>,
) -> Result<Mesocycle, StoreError> {
    let gating =
        gating_role.ok_or_else(|| corrupt(&"a programme that climbs with nothing gating it"))?;
    let primary = Primary::new(
        common.pattern,
        common.exercise,
        SessionRole::try_from(gating).map_err(|error| corrupt(&error))?,
    );
    if template == "sbs" {
        // A `CHECK` refuses an `sbs` row without a provider, so a `None` here is
        // a row that got past the database rather than a state to default.
        let from = common
            .provided
            .ok_or_else(|| corrupt(&"a provided cycle that names no programme"))?;
        // `stored` rather than `new`: the checks ran when it was written, and
        // re-refusing a row now would make a rule change unreadable data.
        // **A null anchor is the inherited case, not a corrupt row.** Only a
        // provided cycle may leave it out: it opens from whatever the mesocycle
        // before it measured, which is not knowable when the plan is authored.
        let entry = entry.map_or(Anchoring::Inherited, Anchoring::Stated);
        return Ok(Mesocycle::Progression(Progression::Provided {
            from,
            cycle: Sbs::stored(
                common.pattern,
                common.exercise,
                common.fills,
                entry,
                common.calendar,
            ),
        }));
    }

    // A ladder and a block are every load a share of their anchor, so an absent
    // one here is a row the `CHECK` should have refused.
    let entry = entry.ok_or_else(|| corrupt(&"a programme that climbs from no anchor"))?;
    Ok(Mesocycle::Progression(if template == "linear" {
        Progression::Linear(
            Linear::rehydrate(primary, common.fills, entry, common.calendar)
                .map_err(|error| corrupt(&error))?,
        )
    } else {
        Progression::BlockPeriodisation(
            BlockPeriodisation::rehydrate(
                primary,
                common.fills,
                entry,
                entry_test,
                common.calendar,
            )
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
        EntryTest::new(reps, light).map_err(|error| corrupt(&error))?,
    ))
}

/// The anchor and its opening, from the columns a programme that climbs must
/// carry.
///
/// **`None` is an absent anchor, which only a provided cycle may have.** It
/// inherits: what it opens from is whatever the mesocycle before it measured, so
/// there is no number to store (issue #86). The `CHECK` still refuses a null
/// anchor for a ladder or a block, where an absent one is a row that got past
/// the database — corrupt, and reported as such by the caller rather than
/// defaulted.
fn read_entry(
    grams: Option<i64>,
    failed_grams: Option<i64>,
    provenance: Option<String>,
    from: Option<String>,
    opening_grams: Option<i64>,
) -> Result<Option<Entry>, StoreError> {
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

    let declared_opening = opening_grams
        .map(|grams| {
            u64::try_from(grams)
                .map(Kg::from_grams)
                .map_err(|_| corrupt(&"a declared opening stored as a negative mass"))
        })
        .transpose()?;
    Ok(Some(Entry::new(anchor, declared_opening)))
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
        anchor_grams,
        provenance,
        anchor_from,
        anchor_failed,
        declared_opening,
        gating,
        test_reps,
        test_target,
        entry_test_reps,
        entry_test_light,
    } = columns_of(mesocycle)?;

    let id = sqlx::query!(
        r#"
        INSERT INTO gym_mesocycle (
            plan, ordinal, provider, provided_programme, template,
            primary_pattern, primary_exercise,
            anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams,
            opening_grams, gating_role, start_date, duration_weeks,
            test_reps, test_target_grams, entry_test_reps, entry_test_light_grams
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        plan,
        ordinal,
        provider,
        provided_programme,
        template,
        pattern,
        primary,
        anchor_grams,
        provenance,
        anchor_from,
        anchor_failed,
        declared_opening,
        gating,
        start,
        duration,
        test_reps,
        test_target,
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
        let role_key = fill.role.map(SessionRole::as_str);
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
                mesocycle, slot, role, position, exercise, static_sets, static_reps
            )
            -- `position` ordered the members of a supersetted slot. Every slot
            -- now holds one exercise, so it is always zero; the column stays
            -- because dropping it is a migration this change does not need.
            VALUES (?, ?, ?, 0, ?, ?, ?)
            ",
            id,
            slot_key,
            role_key,
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
        SELECT slot AS "slot!: String", role AS "role: String",
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
            role: match fill.role {
                Some(role) => Some(SessionRole::try_from(role).map_err(|error| corrupt(&error))?),
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
            match fill.role {
                None => entry.same_static.push(fixed),
                Some(SessionRole::Light) => entry.light_static.push(fixed),
                Some(SessionRole::Heavy) => entry.heavy_static.push(fixed),
            }
            continue;
        }
        match fill.role {
            None => entry.same.push(fill.exercise),
            Some(SessionRole::Light) => entry.light.push(fill.exercise),
            Some(SessionRole::Heavy) => entry.heavy.push(fill.exercise),
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

/// When the block runs: its weekdays, and the weeks it skips.
///
/// Split out of `author` so that function stays inside the line budget. The two
/// go together because both answer "does this date carry a session", which is
/// the question `Calendar::place` is rebuilt from on read.
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

    for (day, role) in calendar.weekdays().iter() {
        let day_key = weekday_key(day);
        let role_key = role.as_str();
        sqlx::query!(
            r"
            INSERT INTO gym_weekday (mesocycle, weekday, role)
            VALUES (?, ?, ?)
            ",
            mesocycle,
            day_key,
            role_key
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

/// Which weekdays the mesocycle runs, and as what.
async fn read_weekdays(
    pool: &SqlitePool,
    mesocycle: i64,
) -> Result<domain::prescription::Weekdays, StoreError> {
    let weekday_rows = sqlx::query!(
        r#"
        SELECT weekday AS "weekday!: String", role AS "role!: String"
        FROM gym_weekday
        WHERE mesocycle = ?
        "#,
        mesocycle
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut days = Vec::with_capacity(weekday_rows.len());
    for day in weekday_rows {
        days.push((
            weekday_of(&day.weekday)?,
            SessionRole::try_from(day.role).map_err(|error| corrupt(&error))?,
        ));
    }
    domain::prescription::Weekdays::new(days).map_err(|error| corrupt(&error))
}

/// Our exercise vocabulary, from its stored key.
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
        let mut push = |role, fixed: &StaticFill| {
            rows.push(FlatFill {
                slot,
                role,
                exercise: fixed.exercise,
                statics: Some((fixed.sets, fixed.reps)),
            });
        };
        match fill {
            Fill::Same(fixed) => push(None, fixed),
            Fill::Alternating(per_role) => {
                push(Some(SessionRole::Light), &per_role.light);
                push(Some(SessionRole::Heavy), &per_role.heavy);
            }
        }
    };
    statics(SlotId::Plyometric, &fills.plyometric);
    statics(SlotId::Power, &fills.power);

    let mut single = |slot: SlotId, fill: &Fill<Exercise>| {
        let mut push = |role, exercise| {
            rows.push(FlatFill {
                slot,
                role,
                exercise,
                statics: None,
            });
        };
        match fill {
            Fill::Same(exercise) => push(None, *exercise),
            Fill::Alternating(per_role) => {
                push(Some(SessionRole::Light), per_role.light);
                push(Some(SessionRole::Heavy), per_role.heavy);
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
    role: Option<SessionRole>,
    exercise: Exercise,
    statics: Option<(RepCount, RepCount)>,
}

/// A repetition count as the store holds it.
fn count_of(value: i64) -> Result<RepCount, StoreError> {
    let count = u32::try_from(value).map_err(|_| corrupt(&"a count the domain cannot hold"))?;
    RepCount::new(count).map_err(|error| corrupt(&error))
}
