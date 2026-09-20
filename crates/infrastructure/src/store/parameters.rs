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
    gym::exercise::Implement,
    measure::{Duration, Kg},
    prescription::{
        BlockRest, GenerationParameters, LoadSteps, Percentage, RestScheme, Scales, Step, Target,
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

/// One block's rest, as the four columns the schema keeps.
///
/// The inverse of [`block_rest`]. An exact rest writes no `high`, and a block
/// that states no superset rest writes no superset pair at all — null there
/// means "the same however it is grouped", which is a different fact from zero.
fn rest_columns(rest: BlockRest) -> Result<RestColumns, StoreError> {
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
fn block_rest(
    low: i64,
    high: Option<i64>,
    superset_low: Option<i64>,
    superset_high: Option<i64>,
) -> Result<BlockRest, StoreError> {
    Ok(BlockRest {
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

/// The stored pair of bounds, back as the span the domain holds.
///
/// **Two columns, one type.** The schema keeps `low` and `high` because that is
/// what a range is queried by, and the domain keeps a minimum and an extent
/// because that is what cannot be written down wrong. This is the boundary
/// between them, and it is the one place the pair can fail to be a range — the
/// column check should already have refused it, so reaching the error arm means
/// the row is corrupt.
/// The warm-up ramp for one parameter version.
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
                   rest_plyometric_low AS "rest_plyometric_low!: i64",
                   rest_plyometric_high AS "rest_plyometric_high: i64",
                   rest_plyometric_ss_low AS "rest_plyometric_ss_low: i64",
                   rest_plyometric_ss_high AS "rest_plyometric_ss_high: i64",
                   rest_power_low AS "rest_power_low!: i64",
                   rest_power_high AS "rest_power_high: i64",
                   rest_power_ss_low AS "rest_power_ss_low: i64",
                   rest_power_ss_high AS "rest_power_ss_high: i64",
                   rest_strength_low AS "rest_strength_low!: i64",
                   rest_strength_high AS "rest_strength_high: i64",
                   rest_strength_ss_low AS "rest_strength_ss_low: i64",
                   rest_strength_ss_high AS "rest_strength_ss_high: i64",
                   rest_hypertrophy_low AS "rest_hypertrophy_low!: i64",
                   rest_hypertrophy_high AS "rest_hypertrophy_high: i64",
                   rest_hypertrophy_ss_low AS "rest_hypertrophy_ss_low: i64",
                   rest_hypertrophy_ss_high AS "rest_hypertrophy_ss_high: i64",
                   rest_mobility_low AS "rest_mobility_low!: i64",
                   rest_mobility_high AS "rest_mobility_high: i64",
                   rest_mobility_ss_low AS "rest_mobility_ss_low: i64",
                   rest_mobility_ss_high AS "rest_mobility_ss_high: i64",
                   static_hold_seconds AS "static_hold_seconds!: i64"
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

        let scales = read_scales(&self.pool, &row.authored_at).await?;

        Ok(Some((
            authored_at,
            GenerationParameters {
                rest: RestScheme {
                    plyometric: block_rest(
                        row.rest_plyometric_low,
                        row.rest_plyometric_high,
                        row.rest_plyometric_ss_low,
                        row.rest_plyometric_ss_high,
                    )?,
                    power: block_rest(
                        row.rest_power_low,
                        row.rest_power_high,
                        row.rest_power_ss_low,
                        row.rest_power_ss_high,
                    )?,
                    strength: block_rest(
                        row.rest_strength_low,
                        row.rest_strength_high,
                        row.rest_strength_ss_low,
                        row.rest_strength_ss_high,
                    )?,
                    hypertrophy: block_rest(
                        row.rest_hypertrophy_low,
                        row.rest_hypertrophy_high,
                        row.rest_hypertrophy_ss_low,
                        row.rest_hypertrophy_ss_high,
                    )?,
                    mobility: block_rest(
                        row.rest_mobility_low,
                        row.rest_mobility_high,
                        row.rest_mobility_ss_low,
                        row.rest_mobility_ss_high,
                    )?,
                },
                light_of_heavy: bp_from_storage(row.light_of_heavy_bp)?,
                ladder_climb_per_week: grams_from_storage(row.ladder_climb_grams)?,
                entry_drop: bp_from_storage(row.entry_drop_bp)?,
                static_hold: domain::measure::Duration::from_seconds(
                    u64::try_from(row.static_hold_seconds)
                        .map_err(|_| corrupt(&"a negative static hold"))?,
                ),
                scales,
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
        let static_hold = i64::try_from(parameters.static_hold.as_seconds())
            .map_err(|_| corrupt(&"a static hold longer than the store can hold"))?;
        let rest_plyometric = rest_columns(parameters.rest.plyometric)?;
        let rest_power = rest_columns(parameters.rest.power)?;
        let rest_strength = rest_columns(parameters.rest.strength)?;
        let rest_hypertrophy = rest_columns(parameters.rest.hypertrophy)?;
        let rest_mobility = rest_columns(parameters.rest.mobility)?;

        sqlx::query!(
            r"
            INSERT INTO generation_parameters (
                authored_at, light_of_heavy_bp,
                ladder_climb_grams, entry_drop_bp,
                static_hold_seconds,
                rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
                rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
                rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
                rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
                rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
            )
            VALUES (?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            stamp,
            light_of_heavy,
            ladder_climb,
            entry_drop,
            static_hold,
            rest_plyometric.low,
            rest_plyometric.high,
            rest_plyometric.superset_low,
            rest_plyometric.superset_high,
            rest_power.low,
            rest_power.high,
            rest_power.superset_low,
            rest_power.superset_high,
            rest_strength.low,
            rest_strength.high,
            rest_strength.superset_low,
            rest_strength.superset_high,
            rest_hypertrophy.low,
            rest_hypertrophy.high,
            rest_hypertrophy.superset_low,
            rest_hypertrophy.superset_high,
            rest_mobility.low,
            rest_mobility.high,
            rest_mobility.superset_low,
            rest_mobility.superset_high
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        write_scales(&mut tx, &stamp, parameters).await?;

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(())
    }
}
