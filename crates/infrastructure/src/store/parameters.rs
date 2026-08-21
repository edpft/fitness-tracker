//! The § 14 generation parameters.
//!
//! **Superseded by date, never overwritten.** Only the current value is required
//! — that is § 14 — and the reason it holds is that an issued prescription
//! records what these produced. So a superseded percentage answers no question,
//! and keeping it costs nothing.
//!
//! "The one in force" is therefore the greatest `authored_at`, which is a `WHERE`
//! clause rather than a mutable flag. Same reasoning as the normalised layer
//! having no `is_current` column.

use std::collections::BTreeMap;

use application::{GenerationParameterStore, StoreError};
use domain::{
    gym::{Kg, NonEmpty, RepCount, exercise::Implement},
    prescription::{
        BackOff, GenerationParameters, LoadSteps, PerRole, Percentage, ResetProtocol, Scales,
        SessionRole, Step, TopSetReps, WarmupStep,
    },
};
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::{corrupt, store_error};

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

fn reps_from_storage(reps: i64) -> Result<RepCount, StoreError> {
    let count =
        u32::try_from(reps).map_err(|_| corrupt(&"a repetition count the domain cannot hold"))?;
    RepCount::new(count).map_err(|error| corrupt(&error))
}

/// The warm-up ramp for one parameter version.
async fn read_warmup(
    pool: &SqlitePool,
    authored_at: &str,
) -> Result<NonEmpty<WarmupStep>, StoreError> {
    let steps = sqlx::query!(
        r#"
        SELECT of_top_set_bp AS "of_top_set_bp!: i64", reps AS "reps!: i64"
        FROM generation_warmup_step
        WHERE parameters_authored_at = ?
        ORDER BY position ASC
        "#,
        authored_at
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut warmup = Vec::with_capacity(steps.len());
    for step in steps {
        warmup.push(WarmupStep {
            of_top_set: bp_from_storage(step.of_top_set_bp)?,
            reps: reps_from_storage(step.reps)?,
        });
    }
    NonEmpty::new(warmup).map_err(|_| corrupt(&"generation parameters with no warm-up ramp"))
}

/// The top-set repetitions for both session roles.
///
/// Both or nothing. `PerRole` is a struct precisely so a missing role is
/// unrepresentable in Rust, which makes this boundary the only place a row deleted
/// by hand can be caught — and it is reported rather than defaulted.
/// What one session role is prescribed: its top set, and its back-off pattern.
///
/// Read together because they are stored together, and stored together because
/// both are per role. Splitting them would be two queries answering one
/// question.
type RoleParameters = (TopSetReps, BackOff);

async fn read_role_reps(
    pool: &SqlitePool,
    authored_at: &str,
) -> Result<(RoleParameters, RoleParameters), StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT role AS "role!: String",
               top_set_reps AS "top_set_reps!: i64",
               back_off_sets AS "back_off_sets!: i64",
               back_off_reps AS "back_off_reps!: i64",
               back_off_bp AS "back_off_bp!: i64"
        FROM generation_role_reps
        WHERE parameters_authored_at = ?
        "#,
        authored_at
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut light = None;
    let mut heavy = None;
    for row in rows {
        let read = (
            TopSetReps::new(reps_from_storage(row.top_set_reps)?),
            BackOff {
                sets: reps_from_storage(row.back_off_sets)?,
                reps: reps_from_storage(row.back_off_reps)?,
                of_top_set: bp_from_storage(row.back_off_bp)?,
            },
        );
        match SessionRole::try_from(row.role.clone()) {
            Ok(SessionRole::Light) => light = Some(read),
            Ok(SessionRole::Heavy) => heavy = Some(read),
            Err(error) => return Err(corrupt(&error)),
        }
    }
    let (Some(light), Some(heavy)) = (light, heavy) else {
        return Err(corrupt(
            &"generation parameters missing a session role's repetitions",
        ));
    };
    Ok((light, heavy))
}

/// Every implement's scale, as authored against one parameter set.
///
/// Ordered by band, which is what makes the domain's "bands ascend" check a
/// statement about the data rather than about the order rows came back in.
async fn read_scales(pool: &SqlitePool, authored_at: &str) -> Result<Scales, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT implement AS "implement!: String",
               from_grams AS "from_grams!: i64",
               size_grams AS "size_grams!: i64"
        FROM generation_load_scale
        WHERE parameters_authored_at = ?
        ORDER BY implement, band
        "#,
        authored_at
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut bands: BTreeMap<Implement, Vec<Step>> = BTreeMap::new();
    for row in rows {
        let implement =
            Implement::try_from(row.implement.clone()).map_err(|error| corrupt(&error))?;
        bands.entry(implement).or_default().push(Step {
            from: grams_from_storage(row.from_grams)?,
            size: grams_from_storage(row.size_grams)?,
        });
    }

    let mut scales = BTreeMap::new();
    for (implement, bands) in bands {
        scales.insert(
            implement,
            LoadSteps::new(bands).map_err(|error| corrupt(&error))?,
        );
    }
    Ok(Scales::new(scales))
}

/// One row per session role: its top set, and its back-off pattern.
///
/// Lifted out of `author` because the three writes it makes are three
/// independent shapes and reading them interleaved is what pushed that function
/// past the length the gate allows.
async fn write_role_reps(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    stamp: &str,
    parameters: &GenerationParameters,
) -> Result<(), StoreError> {
    for role in SessionRole::ALL {
        let key = role.as_str();
        let reps = i64::from(parameters.top_set_reps.get(*role).as_rep_count().as_u32());
        let back_off = parameters.back_off.get(*role);
        let back_off_sets = i64::from(back_off.sets.as_u32());
        let back_off_reps = i64::from(back_off.reps.as_u32());
        let back_off_bp = bp_for_storage(back_off.of_top_set);
        sqlx::query!(
            r"
            INSERT INTO generation_role_reps (
                parameters_authored_at, role, top_set_reps,
                back_off_sets, back_off_reps, back_off_bp
            )
            VALUES (?, ?, ?, ?, ?, ?)
            ",
            stamp,
            key,
            reps,
            back_off_sets,
            back_off_reps,
            back_off_bp
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }

    Ok(())
}

/// One row per band of each implement's scale.
///
/// Nothing is written for an implement with no scale, which is the state that
/// makes an exercise loaded on it report as underivable rather than borrowing
/// the barbell's steps.
async fn write_scales(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    stamp: &str,
    parameters: &GenerationParameters,
) -> Result<(), StoreError> {
    for (implement, steps) in parameters.scales.iter() {
        let key = implement.as_str();
        for (band, step) in steps.bands().iter().enumerate() {
            let band = i64::try_from(band)
                .map_err(|_| corrupt(&"a scale with more bands than the store can hold"))?;
            let from = grams_for_storage(step.from)?;
            let size = grams_for_storage(step.size)?;
            sqlx::query!(
                r"
                INSERT INTO generation_load_scale (
                    parameters_authored_at, implement, band, from_grams, size_grams
                )
                VALUES (?, ?, ?, ?, ?)
                ",
                stamp,
                key,
                band,
                from,
                size
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
    }

    Ok(())
}

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
    async fn current(&self) -> Result<Option<(Timestamp, GenerationParameters)>, StoreError> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT authored_at AS "authored_at!: String",
                   light_of_heavy_bp AS "light_of_heavy_bp!: i64",
                   ladder_climb_grams AS "ladder_climb_grams!: i64",
                   entry_drop_bp AS "entry_drop_bp!: i64",
                   strength_low AS "strength_low!: i64",
                   strength_high AS "strength_high!: i64",
                   strength_sets AS "strength_sets!: i64",
                   hypertrophy_low AS "hypertrophy_low!: i64",
                   hypertrophy_high AS "hypertrophy_high!: i64",
                   hypertrophy_sets AS "hypertrophy_sets!: i64",
                   static_hold_seconds AS "static_hold_seconds!: i64",
                   reset1_drop_bp AS "reset1_drop_bp!: i64",
                   reset1_reclimb_grams AS "reset1_reclimb_grams!: i64",
                   reset2_drop_bp AS "reset2_drop_bp!: i64",
                   reset2_reclimb_grams AS "reset2_reclimb_grams!: i64"
            FROM generation_parameters
            ORDER BY authored_at DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        else {
            return Ok(None);
        };

        let authored_at: Timestamp = row
            .authored_at
            .parse()
            .map_err(|_| corrupt(&"an authoring date that is not an instant"))?;

        let warmup = read_warmup(&self.pool, &row.authored_at).await?;
        let ((light_reps, light_back_off), (heavy_reps, heavy_back_off)) =
            read_role_reps(&self.pool, &row.authored_at).await?;
        let scales = read_scales(&self.pool, &row.authored_at).await?;

        Ok(Some((
            authored_at,
            GenerationParameters {
                warmup,
                back_off: PerRole {
                    light: light_back_off,
                    heavy: heavy_back_off,
                },
                light_of_heavy: bp_from_storage(row.light_of_heavy_bp)?,
                ladder_climb_per_week: grams_from_storage(row.ladder_climb_grams)?,
                entry_drop: bp_from_storage(row.entry_drop_bp)?,
                top_set_reps: PerRole {
                    light: light_reps,
                    heavy: heavy_reps,
                },
                strength: domain::prescription::AccessoryScheme {
                    low: reps_from_storage(row.strength_low)?,
                    high: reps_from_storage(row.strength_high)?,
                    sets: reps_from_storage(row.strength_sets)?,
                },
                hypertrophy: domain::prescription::AccessoryScheme {
                    low: reps_from_storage(row.hypertrophy_low)?,
                    high: reps_from_storage(row.hypertrophy_high)?,
                    sets: reps_from_storage(row.hypertrophy_sets)?,
                },
                static_hold: domain::gym::Duration::from_seconds(
                    u64::try_from(row.static_hold_seconds)
                        .map_err(|_| corrupt(&"a negative static hold"))?,
                ),
                scales,
                first_reset: ResetProtocol {
                    drop: bp_from_storage(row.reset1_drop_bp)?,
                    reclimb_per_week: grams_from_storage(row.reset1_reclimb_grams)?,
                },
                second_reset: ResetProtocol {
                    drop: bp_from_storage(row.reset2_drop_bp)?,
                    reclimb_per_week: grams_from_storage(row.reset2_reclimb_grams)?,
                },
            },
        )))
    }

    async fn author(
        &self,
        authored_at: Timestamp,
        parameters: &GenerationParameters,
    ) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let stamp = authored_at.to_string();
        let light_of_heavy = bp_for_storage(parameters.light_of_heavy);
        let ladder_climb = grams_for_storage(parameters.ladder_climb_per_week)?;
        let entry_drop = bp_for_storage(parameters.entry_drop);
        let strength_low = i64::from(parameters.strength.low.as_u32());
        let strength_high = i64::from(parameters.strength.high.as_u32());
        let strength_sets = i64::from(parameters.strength.sets.as_u32());
        let hypertrophy_low = i64::from(parameters.hypertrophy.low.as_u32());
        let hypertrophy_high = i64::from(parameters.hypertrophy.high.as_u32());
        let hypertrophy_sets = i64::from(parameters.hypertrophy.sets.as_u32());
        let static_hold = i64::try_from(parameters.static_hold.as_seconds())
            .map_err(|_| corrupt(&"a static hold longer than the store can hold"))?;
        let reset1_drop = bp_for_storage(parameters.first_reset.drop);
        let reset1_reclimb = grams_for_storage(parameters.first_reset.reclimb_per_week)?;
        let reset2_drop = bp_for_storage(parameters.second_reset.drop);
        let reset2_reclimb = grams_for_storage(parameters.second_reset.reclimb_per_week)?;

        sqlx::query!(
            r"
            INSERT INTO generation_parameters (
                authored_at, light_of_heavy_bp,
                ladder_climb_grams, entry_drop_bp,
                strength_low, strength_high, strength_sets,
                hypertrophy_low, hypertrophy_high, hypertrophy_sets,
                static_hold_seconds,
                reset1_drop_bp, reset1_reclimb_grams,
                reset2_drop_bp, reset2_reclimb_grams
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            stamp,
            light_of_heavy,
            ladder_climb,
            entry_drop,
            strength_low,
            strength_high,
            strength_sets,
            hypertrophy_low,
            hypertrophy_high,
            hypertrophy_sets,
            static_hold,
            reset1_drop,
            reset1_reclimb,
            reset2_drop,
            reset2_reclimb
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (position, step) in parameters.warmup.iter().enumerate() {
            let position = i64::try_from(position)
                .map_err(|_| corrupt(&"a warm-up ramp longer than the store can hold"))?;
            let of_top_set = bp_for_storage(step.of_top_set);
            let reps = i64::from(step.reps.as_u32());
            sqlx::query!(
                r"
                INSERT INTO generation_warmup_step (
                    parameters_authored_at, position, of_top_set_bp, reps
                )
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

        write_role_reps(&mut tx, &stamp, parameters).await?;
        write_scales(&mut tx, &stamp, parameters).await?;

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(())
    }
}
