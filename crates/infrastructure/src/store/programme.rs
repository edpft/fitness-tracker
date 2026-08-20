//! The authored programme (§ 12).
//!
//! Written once and kept, superseded by `authored_at` like the parameters
//! beside it. A programme is a record of intent, so nothing regenerates it and
//! nothing replaces it wholesale.
//!
//! **Reading uses `Programme::rehydrate`, not `Programme::new`.** The three
//! consistency checks that depend on nothing but the programme are re-run,
//! because a row edited by hand should be caught. The ladder check is not: its
//! span comes from the parameters, so on read it would be asserting that this
//! programme's duration works with whatever span is in force *now* — not a
//! property of the stored programme. Leaving it out is also what lets this store
//! answer without reading another one, so a programme can still be shown when the
//! parameters are what is broken.

use application::{ProgrammeStore, StoreError};
use domain::{
    gym::{Kg, OperatorZone, RepCount, exercise::Exercise},
    prescription::{
        Anchor, AnchorProvenance, Calendar, PerRole, Programme, ProgrammeId, SessionRole, SlotId,
        linear::{Fill, PrimaryPattern, SlotFills, StaticFill},
    },
};
use jiff::civil::{Date, Weekday};
use sqlx::SqlitePool;

use super::{corrupt, store_error};

/// A weekday's stable key.
///
/// `jiff::civil::Weekday` has no text form we own, so the mapping is written out
/// rather than derived from `Debug` — a `Debug` representation is not a stable
/// key, and this one is persisted.
const fn weekday_key(day: Weekday) -> &'static str {
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

fn weekday_of(key: &str) -> Result<Weekday, StoreError> {
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
pub struct SqliteProgrammeStore {
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

impl SqliteProgrammeStore {
    pub const fn new(pool: SqlitePool, zone: OperatorZone) -> Self {
        Self { pool, zone }
    }
}

impl ProgrammeStore for SqliteProgrammeStore {
    async fn current(&self) -> Result<Option<(ProgrammeId, Programme)>, StoreError> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT id AS "id!: i64", authored_at AS "authored_at!: String",
                   primary_pattern AS "primary_pattern!: String",
                   primary_exercise AS "primary_exercise!: String",
                   anchor_grams AS "anchor_grams!: i64",
                   anchor_provenance AS "anchor_provenance!: String",
                   anchor_from AS "anchor_from!: String",
                   anchor_failed_grams AS "anchor_failed_grams: i64",
                   gating_role AS "gating_role!: String",
                   start_date AS "start_date!: String",
                   duration_weeks AS "duration_weeks!: i64"
            FROM programme
            ORDER BY authored_at DESC, id DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        else {
            return Ok(None);
        };

        let fills = read_fills(&self.pool, row.id).await?;
        let weekdays = read_weekdays(&self.pool, row.id).await?;
        let interruptions = read_interruptions(&self.pool, row.id).await?;

        let anchor_grams = u64::try_from(row.anchor_grams)
            .map_err(|_| corrupt(&"an anchor stored as a negative mass"))?;
        let anchor_failed = row
            .anchor_failed_grams
            .map(|grams| {
                u64::try_from(grams)
                    .map(Kg::from_grams)
                    .map_err(|_| corrupt(&"a failed load stored as a negative mass"))
            })
            .transpose()?;
        let anchor = Anchor::new(
            Kg::from_grams(anchor_grams),
            anchor_failed,
            AnchorProvenance::try_from(row.anchor_provenance).map_err(|error| corrupt(&error))?,
            row.anchor_from
                .parse::<Date>()
                .map_err(|_| corrupt(&"an anchor date that is not a date"))?,
        )
        .map_err(|error| corrupt(&error))?;

        let duration = u32::try_from(row.duration_weeks)
            .map_err(|_| corrupt(&"a duration the domain cannot hold"))?;

        let calendar = Calendar::new(
            row.start_date
                .parse::<Date>()
                .map_err(|_| corrupt(&"a start date that is not a date"))?,
            duration,
            &interruptions,
            weekdays,
            self.zone.as_time_zone(),
        )
        .map_err(|error| corrupt(&error))?;

        let programme = Programme::rehydrate(
            PrimaryPattern::try_from(row.primary_pattern).map_err(|error| corrupt(&error))?,
            exercise_of(&row.primary_exercise)?,
            fills,
            anchor,
            SessionRole::try_from(row.gating_role).map_err(|error| corrupt(&error))?,
            calendar,
            row.authored_at
                .parse()
                .map_err(|_| corrupt(&"an authoring date that is not an instant"))?,
        )
        .map_err(|error| corrupt(&error))?;

        Ok(Some((ProgrammeId::new(row.id), programme)))
    }

    async fn author(&self, programme: &Programme) -> Result<ProgrammeId, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let authored_at = programme.authored_at().to_string();
        let pattern = programme.primary().as_str();
        let primary = programme.primary_exercise().as_str();
        let anchor_grams = i64::try_from(programme.anchor().load().as_grams())
            .map_err(|_| corrupt(&"an anchor larger than the store can hold"))?;
        let provenance = programme.anchor().provenance().as_str();
        let anchor_from = programme.anchor().from().to_string();
        let anchor_failed = programme
            .anchor()
            .failed()
            .map(|failed| {
                i64::try_from(failed.as_grams())
                    .map_err(|_| corrupt(&"a failed load larger than the store can hold"))
            })
            .transpose()?;
        let gating = programme.gating_role().as_str();
        let start = programme.calendar().start().to_string();
        let duration = i64::from(programme.calendar().duration_weeks());

        let id = sqlx::query!(
            r"
            INSERT INTO programme (
                authored_at, template, primary_pattern, primary_exercise,
                anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams,
                gating_role, start_date, duration_weeks
            )
            VALUES (?, 'linear', ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
            authored_at,
            pattern,
            primary,
            anchor_grams,
            provenance,
            anchor_from,
            anchor_failed,
            gating,
            start,
            duration
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        for fill in flatten(programme.fills()) {
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
                INSERT INTO programme_slot_fill (
                    programme, slot, role, position, exercise, static_sets, static_reps
                )
                -- `position` ordered the members of a supersetted slot. Every
                -- slot now holds one exercise, so it is always zero; the column
                -- stays because dropping it is a migration this change does not
                -- need.
                VALUES (?, ?, ?, 0, ?, ?, ?)
                ",
                id,
                slot_key,
                role_key,
                exercise_key,
                static_sets,
                static_reps
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        write_calendar(&mut tx, id, programme.calendar()).await?;

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(ProgrammeId::new(id))
    }
}

/// Every slot fill for one programme.
///
/// Split out of `current` so that function stays inside the line budget, and
/// because "assemble the fills" is a whole job on its own: the rows are flat, and
/// any of them may alternate by role.
async fn read_fills(pool: &SqlitePool, programme: i64) -> Result<SlotFills, StoreError> {
    let fill_rows = sqlx::query!(
        r#"
        SELECT slot AS "slot!: String", role AS "role: String",
               exercise AS "exercise!: String",
               static_sets AS "static_sets: i64", static_reps AS "static_reps: i64"
        FROM programme_slot_fill
        WHERE programme = ?
        ORDER BY slot ASC, position ASC
        "#,
        programme
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
            .ok_or_else(|| corrupt(&format!("the stored programme has no fill for {slot}")))
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
    programme: i64,
    calendar: &domain::prescription::Calendar,
) -> Result<(), StoreError> {
    for week in calendar.interruptions().iter() {
        let week = week.to_string();
        sqlx::query!(
            r"
            INSERT INTO programme_interruption (programme, week)
            VALUES (?, ?)
            ",
            programme,
            week
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
            INSERT INTO programme_weekday (programme, weekday, role)
            VALUES (?, ?, ?)
            ",
            programme,
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
async fn read_interruptions(pool: &SqlitePool, programme: i64) -> Result<Vec<Date>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT week AS "week!: String"
        FROM programme_interruption
        WHERE programme = ?
        ORDER BY week ASC
        "#,
        programme
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut weeks = Vec::with_capacity(rows.len());
    for row in rows {
        weeks.push(
            row.week
                .parse::<Date>()
                .map_err(|_| corrupt(&"an interrupted week that is not a date"))?,
        );
    }
    Ok(weeks)
}

/// Which weekdays the programme runs, and as what.
async fn read_weekdays(
    pool: &SqlitePool,
    programme: i64,
) -> Result<domain::prescription::Weekdays, StoreError> {
    let weekday_rows = sqlx::query!(
        r#"
        SELECT weekday AS "weekday!: String", role AS "role!: String"
        FROM programme_weekday
        WHERE programme = ?
        "#,
        programme
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
