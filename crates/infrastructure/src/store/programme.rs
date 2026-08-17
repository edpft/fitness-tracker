//! The authored programme (§ 12).
//!
//! Written once and kept, superseded by `authored_at` like the parameters
//! beside it. A programme is a record of intent, so nothing regenerates it and
//! nothing replaces it wholesale.
//!
//! **Reading uses `Programme::rehydrate`, not `Programme::new`.** The three
//! consistency checks that depend on nothing but the programme are re-run,
//! because a row edited by hand should be caught. The ladder check is not, because
//! it was proved against the parameters in force when the programme was authored
//! and those may since have been superseded — re-checking against today's would
//! ask a different question and could refuse something that was valid when
//! written.

use application::{ProgrammeStore, StoreError};
use domain::{
    gym::{
        Kg,
        exercise::Exercise,
        sequence::{AtLeastTwo, TooShort},
    },
    prescription::{
        Anchor, AnchorProvenance, PerRole, Programme, ProgrammeId, SessionRole, SlotId,
        v1::{Fill, PrimaryPattern, SlotFills},
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
/// `role` is `None` where the slot does not alternate, and `position` orders the
/// members of a supersetted slot.
struct FillRow {
    slot: SlotId,
    role: Option<SessionRole>,
    exercise: Exercise,
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

    /// A supersetted fill.
    fn superset(&self, slot: SlotId) -> Result<Fill<AtLeastTwo<Exercise>>, StoreError> {
        let short = |error: TooShort| corrupt(&format!("slot {slot} is a superset and {error}"));
        if !self.same.is_empty() {
            return Ok(Fill::Same(
                AtLeastTwo::new(self.same.clone()).map_err(short)?,
            ));
        }
        Ok(Fill::Alternating(PerRole {
            light: AtLeastTwo::new(self.light.clone()).map_err(short)?,
            heavy: AtLeastTwo::new(self.heavy.clone()).map_err(short)?,
        }))
    }
}

#[derive(Debug, Clone)]
pub struct SqliteProgrammeStore {
    pool: SqlitePool,
}

impl SqliteProgrammeStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
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

        let anchor_grams = u64::try_from(row.anchor_grams)
            .map_err(|_| corrupt(&"an anchor stored as a negative mass"))?;
        let anchor = Anchor::new(
            Kg::from_grams(anchor_grams),
            AnchorProvenance::try_from(row.anchor_provenance).map_err(|error| corrupt(&error))?,
            row.anchor_from
                .parse::<Date>()
                .map_err(|_| corrupt(&"an anchor date that is not a date"))?,
        )
        .map_err(|error| corrupt(&error))?;

        let duration = u32::try_from(row.duration_weeks)
            .map_err(|_| corrupt(&"a duration the domain cannot hold"))?;

        let programme = Programme::rehydrate(
            PrimaryPattern::try_from(row.primary_pattern).map_err(|error| corrupt(&error))?,
            exercise_of(&row.primary_exercise)?,
            fills,
            anchor,
            SessionRole::try_from(row.gating_role).map_err(|error| corrupt(&error))?,
            row.start_date
                .parse::<Date>()
                .map_err(|_| corrupt(&"a start date that is not a date"))?,
            duration,
            weekdays,
            // The zone is operator configuration rather than programme data, and
            // the calendar needs one. Taken from the anchor's own recording zone
            // would be wrong — that is a fact about the test, not about where the
            // operator trains — so `Calendar` is rebuilt with UTC here and the
            // caller re-places it. See the note in `prescribe`.
            jiff::tz::TimeZone::UTC,
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
        let gating = programme.gating_role().as_str();
        let start = programme.calendar().start().to_string();
        let duration = i64::from(programme.calendar().duration_weeks());

        let id = sqlx::query!(
            r"
            INSERT INTO programme (
                authored_at, template, primary_pattern, primary_exercise,
                anchor_grams, anchor_provenance, anchor_from,
                gating_role, start_date, duration_weeks
            )
            VALUES (?, 'v1', ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id
            ",
            authored_at,
            pattern,
            primary,
            anchor_grams,
            provenance,
            anchor_from,
            gating,
            start,
            duration
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        for (slot, role, position, exercise) in flatten(programme.fills()) {
            let slot_key = slot.as_str();
            let role_key = role.map(SessionRole::as_str);
            let exercise_key = exercise.as_str();
            sqlx::query!(
                r"
                INSERT INTO programme_slot_fill (programme, slot, role, position, exercise)
                VALUES (?, ?, ?, ?, ?)
                ",
                id,
                slot_key,
                role_key,
                position,
                exercise_key
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        for (day, role) in programme.calendar().weekdays().iter() {
            let day_key = weekday_key(day);
            let role_key = role.as_str();
            sqlx::query!(
                r"
                INSERT INTO programme_weekday (programme, weekday, role)
                VALUES (?, ?, ?)
                ",
                id,
                day_key,
                role_key
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(ProgrammeId::new(id))
    }
}

/// The eleven slot fills for one programme.
///
/// Split out of `current` so that function stays inside the line budget, and
/// because "assemble the fills" is a whole job on its own: the rows are flat, the
/// template fixes which slots are single and which are supersets, and either
/// shape may alternate by role.
async fn read_fills(pool: &SqlitePool, programme: i64) -> Result<SlotFills, StoreError> {
    let fill_rows = sqlx::query!(
        r#"
        SELECT slot AS "slot!: String", role AS "role: String",
               exercise AS "exercise!: String"
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
        });
    }

    let mut grouped: std::collections::BTreeMap<SlotId, SlotRows> =
        std::collections::BTreeMap::new();
    for fill in parsed {
        let entry = grouped.entry(fill.slot).or_default();
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
        plyometric: rows_for(SlotId::Plyometric)?.single(SlotId::Plyometric)?,
        power: rows_for(SlotId::Power)?.single(SlotId::Power)?,
        knee_dominant: rows_for(SlotId::KneeDominant)?.single(SlotId::KneeDominant)?,
        upper_push: rows_for(SlotId::UpperPush)?.single(SlotId::UpperPush)?,
        upper_pull: rows_for(SlotId::UpperPull)?.single(SlotId::UpperPull)?,
        hip_dominant: rows_for(SlotId::HipDominant)?.single(SlotId::HipDominant)?,
        arms: rows_for(SlotId::Arms)?.superset(SlotId::Arms)?,
        forearms: rows_for(SlotId::Forearms)?.superset(SlotId::Forearms)?,
        core: rows_for(SlotId::Core)?.single(SlotId::Core)?,
        mobility_hold: rows_for(SlotId::MobilityHold)?.single(SlotId::MobilityHold)?,
        mobility_stretch: rows_for(SlotId::MobilityStretch)?.superset(SlotId::MobilityStretch)?,
    })
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
fn flatten(fills: &SlotFills) -> Vec<(SlotId, Option<SessionRole>, i64, Exercise)> {
    let mut rows = Vec::new();

    let mut single = |slot: SlotId, fill: &Fill<Exercise>| match fill {
        Fill::Same(exercise) => rows.push((slot, None, 0, *exercise)),
        Fill::Alternating(per_role) => {
            rows.push((slot, Some(SessionRole::Light), 0, per_role.light));
            rows.push((slot, Some(SessionRole::Heavy), 0, per_role.heavy));
        }
    };
    single(SlotId::Plyometric, &fills.plyometric);
    single(SlotId::Power, &fills.power);
    single(SlotId::KneeDominant, &fills.knee_dominant);
    single(SlotId::UpperPush, &fills.upper_push);
    single(SlotId::UpperPull, &fills.upper_pull);
    single(SlotId::HipDominant, &fills.hip_dominant);
    single(SlotId::Core, &fills.core);
    single(SlotId::MobilityHold, &fills.mobility_hold);

    let mut superset = |slot: SlotId, fill: &Fill<AtLeastTwo<Exercise>>| match fill {
        Fill::Same(members) => {
            for (position, exercise) in members.iter().enumerate() {
                rows.push((slot, None, position_of(position), *exercise));
            }
        }
        Fill::Alternating(per_role) => {
            for (position, exercise) in per_role.light.iter().enumerate() {
                rows.push((
                    slot,
                    Some(SessionRole::Light),
                    position_of(position),
                    *exercise,
                ));
            }
            for (position, exercise) in per_role.heavy.iter().enumerate() {
                rows.push((
                    slot,
                    Some(SessionRole::Heavy),
                    position_of(position),
                    *exercise,
                ));
            }
        }
    };
    superset(SlotId::Arms, &fills.arms);
    superset(SlotId::Forearms, &fills.forearms);
    superset(SlotId::MobilityStretch, &fills.mobility_stretch);

    rows
}

/// A member position on its way into the store.
///
/// Saturating rather than checked: a superset with more members than an `i64` can
/// index is not a case worth an error path.
fn position_of(position: usize) -> i64 {
    i64::try_from(position).unwrap_or(i64::MAX)
}
