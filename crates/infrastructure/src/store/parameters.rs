//! The § 14 generation parameters, each workout and each primary progression
//! (#192).
//!
//! **Superseded by date, never overwritten** (§ 12). Each workout and each
//! progression are dated versions, and a plan takes the latest of each authored
//! on or before it was made — a lookup rather than a reference stored on the
//! plan, and rather than a mutable flag. The load scales and bands are
//! equipment facts (#450) and are read from there.

use std::collections::BTreeMap;

use application::{GenerationParameterStore, ProgressionStore, StoreError, WorkoutStore};
use domain::{
    measure::{Duration, Kg, RepCount},
    prescription::{
        AccessoryScheme, BackOff, ByIntensity, Levels, LinearProgression, LinearRest, Percentage,
        ResetProtocol, Rest, RestScheme, SbsProgression, SbsRest, Scales, Section, SlotId,
        Superset, Target, TopSetReps, WarmupStep, Workout,
    },
    schedule::Relative,
    sequence::NonEmpty,
};
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::{corrupt, equipment::read_facts, store_error};

/// A percentage on its way into the store.
///
/// Basis points are `i32` in the domain and SQLite stores `i64`; the widening is
/// free and lives here rather than at each call site.
const fn bp_for_storage(percentage: Percentage) -> i64 {
    percentage.as_basis_points() as i64
}

fn bp_from_storage(points: i64) -> Result<Percentage, StoreError> {
    let narrowed = i32::try_from(points)
        .map_err(|_| corrupt(&"a percentage larger than the domain can hold"))?;
    Percentage::from_basis_points(narrowed).map_err(|error| corrupt(&error))
}

fn grams_for_storage(mass: Kg) -> Result<i64, StoreError> {
    i64::try_from(mass.as_grams()).map_err(|_| corrupt(&"a mass larger than the store can hold"))
}

fn grams_from_storage(grams: i64) -> Result<Kg, StoreError> {
    let unsigned = u64::try_from(grams)
        .map_err(|_| corrupt(&"a mass stored as a negative number of grams"))?;
    Ok(Kg::from_grams(unsigned))
}

/// One block's rest, as the four columns the schema keeps.
///
/// The inverse of [`section_rest`]. An exact rest writes no `high`, and a block
/// that states no superset rest writes no superset pair at all — null there
/// means "the same however it is grouped", which is a different fact from zero.
fn rest_columns(rest: Rest) -> Result<RestColumns, StoreError> {
    let (low, high) = rest_bounds(rest.between_sets)?;
    let (superset_low, superset_high) = match rest.after_superset {
        Some(grouped) => {
            let (low, high) = rest_bounds(grouped)?;
            (Some(low), high)
        }
        None => (None, None),
    };
    Ok(RestColumns {
        low,
        high,
        superset_low,
        superset_high,
    })
}

/// One block's rest, as the schema keeps it. Named rather than a four-tuple
/// because three of the four are `Option<i64>` and nothing in a tuple says which
/// is which.
struct RestColumns {
    low: i64,
    high: Option<i64>,
    superset_low: Option<i64>,
    superset_high: Option<i64>,
}

/// A rest as a pair of columns. `None` for the top of an exact rest.
fn rest_bounds(rest: Target<Duration>) -> Result<(i64, Option<i64>), StoreError> {
    let seconds = |value: Duration| {
        i64::try_from(value.as_seconds())
            .map_err(|_| corrupt(&"a rest longer than the store can hold"))
    };
    match rest {
        Target::Exactly(value) => Ok((seconds(value)?, None)),
        range @ Target::Range { .. } => {
            Ok((seconds(range.minimum())?, Some(seconds(range.maximum())?)))
        }
    }
}

/// One block's rest, from its four columns.
///
/// **`high` absent is one number, not a missing one**, and `ss_low` absent means
/// the block rests the same however its work is grouped — which is not the same
/// as resting for zero, and is why the superset pair is nullable rather than
/// defaulted.
fn section_rest(
    low: i64,
    high: Option<i64>,
    superset_low: Option<i64>,
    superset_high: Option<i64>,
) -> Result<Rest, StoreError> {
    Ok(Rest {
        between_sets: rest_target(low, high)?,
        after_superset: match superset_low {
            Some(low) => Some(rest_target(low, superset_high)?),
            None => None,
        },
    })
}

/// A stored rest, as the span the domain holds. `high` absent is an exact rest.
fn rest_target(low: i64, high: Option<i64>) -> Result<Target<Duration>, StoreError> {
    let seconds = |value: i64| {
        u64::try_from(value)
            .map(Duration::from_seconds)
            .map_err(|_| corrupt(&"a negative rest"))
    };
    match high {
        Some(high) => Target::between(seconds(low)?, seconds(high)?)
            .ok_or_else(|| corrupt(&"a stored rest range that does not span")),
        None => Ok(Target::Exactly(seconds(low)?)),
    }
}

/// A repetition target as a pair of columns. `None` for the top of an exact
/// count.
fn reps_bounds(reps: Target<RepCount>) -> (i64, Option<i64>) {
    let low = i64::from(reps.minimum().as_u32());
    match reps {
        Target::Exactly(_) => (low, None),
        range @ Target::Range { .. } => (low, Some(i64::from(range.maximum().as_u32()))),
    }
}

fn count_from_storage(value: i64) -> Result<RepCount, StoreError> {
    let narrowed = u32::try_from(value).map_err(|_| corrupt(&"a repetition count out of range"))?;
    RepCount::new(narrowed).map_err(|error| corrupt(&error))
}

fn count_for_storage(count: RepCount) -> i64 {
    i64::from(count.as_u32())
}

/// A stored repetition target. `high` absent is an exact count.
fn reps_target(low: i64, high: Option<i64>) -> Result<Target<RepCount>, StoreError> {
    match high {
        Some(high) => Target::between(count_from_storage(low)?, count_from_storage(high)?)
            .ok_or_else(|| corrupt(&"a stored repetition range that does not span")),
        None => Ok(Target::Exactly(count_from_storage(low)?)),
    }
}

fn scheme(reps_low: i64, reps_high: Option<i64>, sets: i64) -> Result<AccessoryScheme, StoreError> {
    Ok(AccessoryScheme {
        reps: reps_target(reps_low, reps_high)?,
        sets: count_from_storage(sets)?,
    })
}

fn stamp_from_storage(stamp: &str) -> Result<Timestamp, StoreError> {
    stamp
        .parse()
        .map_err(|_| corrupt(&"an authoring date that is not an instant"))
}

/// The latest of `stamps` at or before `at`, as stored.
///
/// **Compared as instants, not as text.** A timestamp prints only the
/// fractional digits it needs, so `…:51Z` sorts after `…:51.5Z` as a string,
/// and a `WHERE authored_at <= ?` would pick the wrong version at a boundary.
fn latest_at(stamps: Vec<String>, at: Timestamp) -> Result<Option<String>, StoreError> {
    let mut latest: Option<(Timestamp, String)> = None;
    for stamp in stamps {
        let parsed = stamp_from_storage(&stamp)?;
        if parsed <= at && latest.as_ref().is_none_or(|(best, _)| parsed > *best) {
            latest = Some((parsed, stamp));
        }
    }
    Ok(latest.map(|(_, stamp)| stamp))
}

fn ramp(steps: Vec<(i64, i64)>) -> Result<NonEmpty<WarmupStep>, StoreError> {
    let mut ramp = Vec::with_capacity(steps.len());
    for (of_top_set, reps) in steps {
        ramp.push(WarmupStep {
            of_top_set: bp_from_storage(of_top_set)?,
            reps: count_from_storage(reps)?,
        });
    }
    NonEmpty::new(ramp).map_err(|_| corrupt(&"a stored warm-up ramp with no step"))
}

/// The load scales and band ranges (§ 14), each workout (#192), and each
/// primary progression (#192), as one store.
#[derive(Debug, Clone)]
pub struct SqliteGenerationParameterStore {
    pool: SqlitePool,
}

impl SqliteGenerationParameterStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl GenerationParameterStore for SqliteGenerationParameterStore {
    async fn scales_on(&self, date: jiff::civil::Date) -> Result<Scales, StoreError> {
        Ok(read_facts(&self.pool).await?.scales_on(date))
    }

    async fn bands_on(
        &self,
        specific: &domain::gym::equipment::Specific,
        date: jiff::civil::Date,
    ) -> Result<Option<domain::gym::equipment::BandRange>, StoreError> {
        Ok(read_facts(&self.pool)
            .await?
            .bands
            .get(specific)
            .and_then(|timeline| timeline.on(date))
            .map(|fact| fact.value.clone()))
    }
}

/// Which level a stored row of a workout is stated at, and for what.
enum Named {
    Section(Section),
    Superset(Superset),
    Exercise(SlotId),
}

fn named(level: &str, name: String) -> Result<Named, StoreError> {
    match level {
        "section" => Section::try_from(name)
            .map(Named::Section)
            .map_err(|error| corrupt(&error)),
        "superset" => Superset::try_from(name)
            .map(Named::Superset)
            .map_err(|error| corrupt(&error)),
        "exercise" => SlotId::try_from(name)
            .map(Named::Exercise)
            .map_err(|error| corrupt(&error)),
        other => Err(corrupt(&format!("{other:?} is not a level of a workout"))),
    }
}

/// A workout's reps stated narrower than the workout, over its default.
async fn read_reps(
    pool: &SqlitePool,
    of: &str,
    stamp: &str,
    default: AccessoryScheme,
) -> Result<Levels<AccessoryScheme>, StoreError> {
    let mut reps = Levels::only(default);
    let rep_rows = sqlx::query!(
        r#"
        SELECT level AS "level!: String", name AS "name!: String",
               reps_low AS "reps_low!: i64", reps_high AS "reps_high: i64",
               sets AS "sets!: i64"
        FROM workout_reps
        WHERE intensity = ? AND authored_at = ?
        "#,
        of,
        stamp
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    for found in rep_rows {
        let stated = scheme(found.reps_low, found.reps_high, found.sets)?;
        match named(&found.level, found.name)? {
            Named::Section(section) => reps.sections.insert(section, stated),
            Named::Superset(superset) => reps.supersets.insert(superset, stated),
            Named::Exercise(slot) => reps.exercises.insert(slot, stated),
        };
    }
    Ok(reps)
}

/// A workout's rest stated narrower than the workout, over its default.
async fn read_rest(
    pool: &SqlitePool,
    of: &str,
    stamp: &str,
    workout: Rest,
) -> Result<RestScheme, StoreError> {
    let mut rest = RestScheme {
        workout,
        sections: BTreeMap::new(),
        supersets: BTreeMap::new(),
        exercises: BTreeMap::new(),
    };
    let rest_rows = sqlx::query!(
        r#"
        SELECT level AS "level!: String", name AS "name!: String",
               rest_low AS "rest_low!: i64", rest_high AS "rest_high: i64",
               rest_ss_low AS "rest_ss_low: i64", rest_ss_high AS "rest_ss_high: i64"
        FROM workout_rest
        WHERE intensity = ? AND authored_at = ?
        "#,
        of,
        stamp
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    for found in rest_rows {
        match named(&found.level, found.name)? {
            Named::Section(section) => {
                rest.sections.insert(
                    section,
                    section_rest(
                        found.rest_low,
                        found.rest_high,
                        found.rest_ss_low,
                        found.rest_ss_high,
                    )?,
                );
            }
            Named::Superset(superset) => {
                rest.supersets
                    .insert(superset, rest_target(found.rest_low, found.rest_high)?);
            }
            Named::Exercise(slot) => {
                rest.exercises
                    .insert(slot, rest_target(found.rest_low, found.rest_high)?);
            }
        }
    }
    Ok(rest)
}

async fn write_reps(
    conn: &mut sqlx::SqliteConnection,
    of: &str,
    stamp: &str,
    reps: &Levels<AccessoryScheme>,
) -> Result<(), StoreError> {
    let stated = reps
        .sections
        .iter()
        .map(|(section, scheme)| ("section", section.to_string(), scheme))
        .chain(
            reps.supersets
                .iter()
                .map(|(superset, scheme)| ("superset", superset.to_string(), scheme)),
        )
        .chain(
            reps.exercises
                .iter()
                .map(|(slot, scheme)| ("exercise", slot.to_string(), scheme)),
        );
    for (level, name, scheme) in stated {
        let (low, high) = reps_bounds(scheme.reps);
        let sets = count_for_storage(scheme.sets);
        sqlx::query!(
            r"
            INSERT INTO workout_reps (
                intensity, authored_at, level, name, reps_low, reps_high, sets
            )
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ",
            of,
            stamp,
            level,
            name,
            low,
            high,
            sets
        )
        .execute(&mut *conn)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// One rest for a superset or an exercise, which states no after-superset rest.
fn single_rest(rest: Target<Duration>) -> Result<RestColumns, StoreError> {
    let (low, high) = rest_bounds(rest)?;
    Ok(RestColumns {
        low,
        high,
        superset_low: None,
        superset_high: None,
    })
}

async fn write_rest(
    conn: &mut sqlx::SqliteConnection,
    of: &str,
    stamp: &str,
    rest: &RestScheme,
) -> Result<(), StoreError> {
    let mut stated = Vec::new();
    for (section, rest) in &rest.sections {
        stated.push(("section", section.to_string(), rest_columns(*rest)?));
    }
    for (superset, rest) in &rest.supersets {
        stated.push(("superset", superset.to_string(), single_rest(*rest)?));
    }
    for (slot, rest) in &rest.exercises {
        stated.push(("exercise", slot.to_string(), single_rest(*rest)?));
    }
    for (level, name, rest) in stated {
        sqlx::query!(
            r"
            INSERT INTO workout_rest (
                intensity, authored_at, level, name,
                rest_low, rest_high, rest_ss_low, rest_ss_high
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ",
            of,
            stamp,
            level,
            name,
            rest.low,
            rest.high,
            rest.superset_low,
            rest.superset_high
        )
        .execute(&mut *conn)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

async fn read_linear_rest(pool: &SqlitePool, stamp: &str) -> Result<LinearRest, StoreError> {
    let rest = sqlx::query!(
        r#"
        SELECT top_set_low AS "top_set_low!: i64", top_set_high AS "top_set_high: i64",
               back_off_low AS "back_off_low!: i64", back_off_high AS "back_off_high: i64",
               attempt_low AS "attempt_low!: i64", attempt_high AS "attempt_high: i64"
        FROM linear_rest
        WHERE authored_at = ?
        "#,
        stamp
    )
    .fetch_one(pool)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(LinearRest {
        top_set: rest_target(rest.top_set_low, rest.top_set_high)?,
        back_off: rest_target(rest.back_off_low, rest.back_off_high)?,
        attempt: rest_target(rest.attempt_low, rest.attempt_high)?,
    })
}

async fn write_linear_rest(
    conn: &mut sqlx::SqliteConnection,
    stamp: &str,
    rest: &LinearRest,
) -> Result<(), StoreError> {
    let (top_set_low, top_set_high) = rest_bounds(rest.top_set)?;
    let (back_off_low, back_off_high) = rest_bounds(rest.back_off)?;
    let (attempt_low, attempt_high) = rest_bounds(rest.attempt)?;
    sqlx::query!(
        r"
        INSERT INTO linear_rest (
            authored_at, top_set_low, top_set_high, back_off_low, back_off_high,
            attempt_low, attempt_high
        )
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ",
        stamp,
        top_set_low,
        top_set_high,
        back_off_low,
        back_off_high,
        attempt_low,
        attempt_high
    )
    .execute(&mut *conn)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

impl WorkoutStore for SqliteGenerationParameterStore {
    async fn workout_at(
        &self,
        intensity: Relative,
        at: Timestamp,
    ) -> Result<Option<(Timestamp, Workout)>, StoreError> {
        let of = intensity.as_str();
        let stamps = sqlx::query_scalar!(
            r#"SELECT authored_at AS "authored_at!: String" FROM workout WHERE intensity = ?"#,
            of
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let Some(stamp) = latest_at(stamps, at)? else {
            return Ok(None);
        };
        let row = sqlx::query!(
            r#"
            SELECT reps_low AS "reps_low!: i64", reps_high AS "reps_high: i64",
                   sets AS "sets!: i64",
                   rest_low AS "rest_low!: i64", rest_high AS "rest_high: i64",
                   rest_ss_low AS "rest_ss_low: i64", rest_ss_high AS "rest_ss_high: i64",
                   static_hold_seconds AS "static_hold_seconds!: i64"
            FROM workout
            WHERE intensity = ? AND authored_at = ?
            "#,
            of,
            stamp
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let reps = read_reps(
            &self.pool,
            of,
            &stamp,
            scheme(row.reps_low, row.reps_high, row.sets)?,
        )
        .await?;
        let rest = read_rest(
            &self.pool,
            of,
            &stamp,
            section_rest(
                row.rest_low,
                row.rest_high,
                row.rest_ss_low,
                row.rest_ss_high,
            )?,
        )
        .await?;

        Ok(Some((
            stamp_from_storage(&stamp)?,
            Workout {
                reps,
                rest,
                static_hold: Duration::from_seconds(
                    u64::try_from(row.static_hold_seconds)
                        .map_err(|_| corrupt(&"a negative static hold"))?,
                ),
            },
        )))
    }

    async fn author_workout(
        &self,
        intensity: Relative,
        at: Timestamp,
        workout: &Workout,
    ) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let of = intensity.as_str();
        let stamp = at.to_string();
        let (reps_low, reps_high) = reps_bounds(workout.reps.workout.reps);
        let sets = count_for_storage(workout.reps.workout.sets);
        let rest = rest_columns(workout.rest.workout)?;
        let static_hold = i64::try_from(workout.static_hold.as_seconds())
            .map_err(|_| corrupt(&"a static hold longer than the store can hold"))?;
        sqlx::query!(
            r"
            INSERT INTO workout (
                intensity, authored_at, reps_low, reps_high, sets,
                rest_low, rest_high, rest_ss_low, rest_ss_high, static_hold_seconds
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            of,
            stamp,
            reps_low,
            reps_high,
            sets,
            rest.low,
            rest.high,
            rest.superset_low,
            rest.superset_high,
            static_hold
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        write_reps(&mut tx, of, &stamp, &workout.reps).await?;
        write_rest(&mut tx, of, &stamp, &workout.rest).await?;

        tx.commit().await.map_err(|error| store_error(&error))
    }
}

impl ProgressionStore for SqliteGenerationParameterStore {
    async fn linear_at(
        &self,
        at: Timestamp,
    ) -> Result<Option<(Timestamp, LinearProgression)>, StoreError> {
        let stamps = sqlx::query_scalar!(
            r#"SELECT authored_at AS "authored_at!: String" FROM linear_progression"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let Some(stamp) = latest_at(stamps, at)? else {
            return Ok(None);
        };
        let row = sqlx::query!(
            r#"
            SELECT light_of_heavy_bp AS "light_of_heavy_bp!: i64",
                   ladder_climb_grams AS "ladder_climb_grams!: i64",
                   entry_drop_bp AS "entry_drop_bp!: i64",
                   higher_top_set_reps AS "higher_top_set_reps!: i64",
                   lower_top_set_reps AS "lower_top_set_reps!: i64",
                   higher_back_off_sets AS "higher_back_off_sets!: i64",
                   higher_back_off_reps AS "higher_back_off_reps!: i64",
                   higher_back_off_bp AS "higher_back_off_bp!: i64",
                   lower_back_off_sets AS "lower_back_off_sets!: i64",
                   lower_back_off_reps AS "lower_back_off_reps!: i64",
                   lower_back_off_bp AS "lower_back_off_bp!: i64",
                   first_reset_drop_bp AS "first_reset_drop_bp!: i64",
                   first_reset_reclimb_grams AS "first_reset_reclimb_grams!: i64",
                   second_reset_drop_bp AS "second_reset_drop_bp!: i64",
                   second_reset_reclimb_grams AS "second_reset_reclimb_grams!: i64"
            FROM linear_progression
            WHERE authored_at = ?
            "#,
            stamp
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let steps = sqlx::query!(
            r#"
            SELECT of_top_set_bp AS "of_top_set_bp!: i64", reps AS "reps!: i64"
            FROM linear_warmup_step
            WHERE authored_at = ?
            ORDER BY position ASC
            "#,
            stamp
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let rest = read_linear_rest(&self.pool, &stamp).await?;

        Ok(Some((
            stamp_from_storage(&stamp)?,
            LinearProgression {
                warmup: ramp(
                    steps
                        .into_iter()
                        .map(|step| (step.of_top_set_bp, step.reps))
                        .collect(),
                )?,
                top_set_reps: ByIntensity {
                    higher: TopSetReps::new(count_from_storage(row.higher_top_set_reps)?),
                    lower: TopSetReps::new(count_from_storage(row.lower_top_set_reps)?),
                },
                back_off: ByIntensity {
                    higher: BackOff {
                        sets: count_from_storage(row.higher_back_off_sets)?,
                        reps: count_from_storage(row.higher_back_off_reps)?,
                        of_top_set: bp_from_storage(row.higher_back_off_bp)?,
                    },
                    lower: BackOff {
                        sets: count_from_storage(row.lower_back_off_sets)?,
                        reps: count_from_storage(row.lower_back_off_reps)?,
                        of_top_set: bp_from_storage(row.lower_back_off_bp)?,
                    },
                },
                light_of_heavy: bp_from_storage(row.light_of_heavy_bp)?,
                ladder_climb_per_week: grams_from_storage(row.ladder_climb_grams)?,
                entry_drop: bp_from_storage(row.entry_drop_bp)?,
                first_reset: ResetProtocol {
                    drop: bp_from_storage(row.first_reset_drop_bp)?,
                    reclimb_per_week: grams_from_storage(row.first_reset_reclimb_grams)?,
                },
                second_reset: ResetProtocol {
                    drop: bp_from_storage(row.second_reset_drop_bp)?,
                    reclimb_per_week: grams_from_storage(row.second_reset_reclimb_grams)?,
                },
                rest,
            },
        )))
    }

    async fn author_linear(
        &self,
        at: Timestamp,
        linear: &LinearProgression,
    ) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let stamp = at.to_string();
        let light_of_heavy = bp_for_storage(linear.light_of_heavy);
        let ladder_climb = grams_for_storage(linear.ladder_climb_per_week)?;
        let entry_drop = bp_for_storage(linear.entry_drop);
        let higher_top = count_for_storage(linear.top_set_reps.higher.as_rep_count());
        let lower_top = count_for_storage(linear.top_set_reps.lower.as_rep_count());
        let higher = linear.back_off.higher;
        let (higher_sets, higher_reps, higher_bp) = (
            count_for_storage(higher.sets),
            count_for_storage(higher.reps),
            bp_for_storage(higher.of_top_set),
        );
        let lower = linear.back_off.lower;
        let (lower_sets, lower_reps, lower_bp) = (
            count_for_storage(lower.sets),
            count_for_storage(lower.reps),
            bp_for_storage(lower.of_top_set),
        );
        let first_drop = bp_for_storage(linear.first_reset.drop);
        let first_reclimb = grams_for_storage(linear.first_reset.reclimb_per_week)?;
        let second_drop = bp_for_storage(linear.second_reset.drop);
        let second_reclimb = grams_for_storage(linear.second_reset.reclimb_per_week)?;

        sqlx::query!(
            r"
            INSERT INTO linear_progression (
                authored_at, light_of_heavy_bp, ladder_climb_grams, entry_drop_bp,
                higher_top_set_reps, lower_top_set_reps,
                higher_back_off_sets, higher_back_off_reps, higher_back_off_bp,
                lower_back_off_sets, lower_back_off_reps, lower_back_off_bp,
                first_reset_drop_bp, first_reset_reclimb_grams,
                second_reset_drop_bp, second_reset_reclimb_grams
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            stamp,
            light_of_heavy,
            ladder_climb,
            entry_drop,
            higher_top,
            lower_top,
            higher_sets,
            higher_reps,
            higher_bp,
            lower_sets,
            lower_reps,
            lower_bp,
            first_drop,
            first_reclimb,
            second_drop,
            second_reclimb
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (position, step) in (0_i64..).zip(linear.warmup.iter()) {
            let of_top_set = bp_for_storage(step.of_top_set);
            let reps = count_for_storage(step.reps);
            sqlx::query!(
                r"
                INSERT INTO linear_warmup_step (authored_at, position, of_top_set_bp, reps)
                VALUES (?, ?, ?, ?)
                ",
                stamp,
                position,
                of_top_set,
                reps
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        write_linear_rest(&mut tx, &stamp, &linear.rest).await?;

        tx.commit().await.map_err(|error| store_error(&error))
    }

    async fn sbs_at(
        &self,
        at: Timestamp,
    ) -> Result<Option<(Timestamp, SbsProgression)>, StoreError> {
        let stamps = sqlx::query_scalar!(
            r#"SELECT authored_at AS "authored_at!: String" FROM sbs_progression"#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let Some(stamp) = latest_at(stamps, at)? else {
            return Ok(None);
        };
        let steps = sqlx::query!(
            r#"
            SELECT of_top_set_bp AS "of_top_set_bp!: i64", reps AS "reps!: i64"
            FROM sbs_warmup_step
            WHERE authored_at = ?
            ORDER BY position ASC
            "#,
            stamp
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let rest = sqlx::query!(
            r#"
            SELECT across_low AS "across_low!: i64", across_high AS "across_high: i64",
                   rep_max_low AS "rep_max_low!: i64", rep_max_high AS "rep_max_high: i64",
                   back_off_low AS "back_off_low!: i64", back_off_high AS "back_off_high: i64",
                   attempt_low AS "attempt_low!: i64", attempt_high AS "attempt_high: i64"
            FROM sbs_rest
            WHERE authored_at = ?
            "#,
            stamp
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(Some((
            stamp_from_storage(&stamp)?,
            SbsProgression {
                warmup: ramp(
                    steps
                        .into_iter()
                        .map(|step| (step.of_top_set_bp, step.reps))
                        .collect(),
                )?,
                rest: SbsRest {
                    across: rest_target(rest.across_low, rest.across_high)?,
                    rep_max: rest_target(rest.rep_max_low, rest.rep_max_high)?,
                    back_off: rest_target(rest.back_off_low, rest.back_off_high)?,
                    attempt: rest_target(rest.attempt_low, rest.attempt_high)?,
                },
            },
        )))
    }

    async fn author_sbs(&self, at: Timestamp, sbs: &SbsProgression) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let stamp = at.to_string();
        sqlx::query!(
            "INSERT INTO sbs_progression (authored_at) VALUES (?)",
            stamp
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (position, step) in (0_i64..).zip(sbs.warmup.iter()) {
            let of_top_set = bp_for_storage(step.of_top_set);
            let reps = count_for_storage(step.reps);
            sqlx::query!(
                r"
                INSERT INTO sbs_warmup_step (authored_at, position, of_top_set_bp, reps)
                VALUES (?, ?, ?, ?)
                ",
                stamp,
                position,
                of_top_set,
                reps
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        let (across_low, across_high) = rest_bounds(sbs.rest.across)?;
        let (rep_max_low, rep_max_high) = rest_bounds(sbs.rest.rep_max)?;
        let (back_off_low, back_off_high) = rest_bounds(sbs.rest.back_off)?;
        let (attempt_low, attempt_high) = rest_bounds(sbs.rest.attempt)?;
        sqlx::query!(
            r"
            INSERT INTO sbs_rest (
                authored_at, across_low, across_high, rep_max_low, rep_max_high,
                back_off_low, back_off_high, attempt_low, attempt_high
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            stamp,
            across_low,
            across_high,
            rep_max_low,
            rep_max_high,
            back_off_low,
            back_off_high,
            attempt_low,
            attempt_high
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        tx.commit().await.map_err(|error| store_error(&error))
    }
}
