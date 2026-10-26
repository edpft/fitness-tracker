//! The § 14 generation parameters, the programme and each primary progression
//! (#192).
//!
//! **Superseded by date, never overwritten** (§ 12). The programme and each
//! progression are dated versions, and a plan takes the latest of each authored
//! on or before it was made — a lookup rather than a reference stored on the
//! plan, and rather than a mutable flag. The load scales and bands are
//! equipment facts (#450) and are read from there.

use std::collections::BTreeMap;

use application::{GenerationParameterStore, ProgrammeStore, ProgressionStore, StoreError};
use domain::{
    measure::{Duration, Kg, RepCount},
    prescription::{
        AccessoryScheme, BackOff, ByIntensity, LinearProgression, Percentage, Programme,
        ResetProtocol, RestScheme, SbsChart, SbsDay, SbsProgression, SbsSession, SbsWeek, Scales,
        SectionRest, SlotId, Target, TopSetReps, WarmupStep,
    },
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
fn rest_columns(rest: SectionRest) -> Result<RestColumns, StoreError> {
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
) -> Result<SectionRest, StoreError> {
    Ok(SectionRest {
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

/// The slots a programme version gives a scheme of their own.
async fn slot_schemes(
    pool: &SqlitePool,
    stamp: &str,
) -> Result<BTreeMap<SlotId, AccessoryScheme>, StoreError> {
    let slot_rows = sqlx::query!(
        r#"
        SELECT slot AS "slot!: String", reps_low AS "reps_low!: i64",
               reps_high AS "reps_high: i64", sets AS "sets!: i64"
        FROM programme_slot_scheme
        WHERE authored_at = ?
        "#,
        stamp
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    let mut slots = BTreeMap::new();
    for slot in slot_rows {
        slots.insert(
            SlotId::try_from(slot.slot).map_err(|error| corrupt(&error))?,
            scheme(slot.reps_low, slot.reps_high, slot.sets)?,
        );
    }

    Ok(slots)
}

/// The load scales and band ranges (§ 14), the programme (#192), and each
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

impl ProgrammeStore for SqliteGenerationParameterStore {
    async fn programme_at(
        &self,
        at: Timestamp,
    ) -> Result<Option<(Timestamp, Programme)>, StoreError> {
        let stamps =
            sqlx::query_scalar!(r#"SELECT authored_at AS "authored_at!: String" FROM programme"#)
                .fetch_all(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
        let Some(stamp) = latest_at(stamps, at)? else {
            return Ok(None);
        };
        let row = sqlx::query!(
            r#"
            SELECT strength_reps_low AS "strength_reps_low!: i64",
                   strength_reps_high AS "strength_reps_high: i64",
                   strength_sets AS "strength_sets!: i64",
                   hypertrophy_reps_low AS "hypertrophy_reps_low!: i64",
                   hypertrophy_reps_high AS "hypertrophy_reps_high: i64",
                   hypertrophy_sets AS "hypertrophy_sets!: i64",
                   static_hold_seconds AS "static_hold_seconds!: i64",
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
                   rest_mobility_ss_high AS "rest_mobility_ss_high: i64"
            FROM programme
            WHERE authored_at = ?
            "#,
            stamp
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let slots = slot_schemes(&self.pool, &stamp).await?;

        Ok(Some((
            stamp_from_storage(&stamp)?,
            Programme {
                strength: scheme(
                    row.strength_reps_low,
                    row.strength_reps_high,
                    row.strength_sets,
                )?,
                hypertrophy: scheme(
                    row.hypertrophy_reps_low,
                    row.hypertrophy_reps_high,
                    row.hypertrophy_sets,
                )?,
                slots,
                rest: RestScheme {
                    plyometric: section_rest(
                        row.rest_plyometric_low,
                        row.rest_plyometric_high,
                        row.rest_plyometric_ss_low,
                        row.rest_plyometric_ss_high,
                    )?,
                    power: section_rest(
                        row.rest_power_low,
                        row.rest_power_high,
                        row.rest_power_ss_low,
                        row.rest_power_ss_high,
                    )?,
                    strength: section_rest(
                        row.rest_strength_low,
                        row.rest_strength_high,
                        row.rest_strength_ss_low,
                        row.rest_strength_ss_high,
                    )?,
                    hypertrophy: section_rest(
                        row.rest_hypertrophy_low,
                        row.rest_hypertrophy_high,
                        row.rest_hypertrophy_ss_low,
                        row.rest_hypertrophy_ss_high,
                    )?,
                    mobility: section_rest(
                        row.rest_mobility_low,
                        row.rest_mobility_high,
                        row.rest_mobility_ss_low,
                        row.rest_mobility_ss_high,
                    )?,
                },
                static_hold: Duration::from_seconds(
                    u64::try_from(row.static_hold_seconds)
                        .map_err(|_| corrupt(&"a negative static hold"))?,
                ),
            },
        )))
    }

    async fn author_programme(
        &self,
        at: Timestamp,
        programme: &Programme,
    ) -> Result<(), StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let stamp = at.to_string();
        let (strength_low, strength_high) = reps_bounds(programme.strength.reps);
        let strength_sets = count_for_storage(programme.strength.sets);
        let (hypertrophy_low, hypertrophy_high) = reps_bounds(programme.hypertrophy.reps);
        let hypertrophy_sets = count_for_storage(programme.hypertrophy.sets);
        let static_hold = i64::try_from(programme.static_hold.as_seconds())
            .map_err(|_| corrupt(&"a static hold longer than the store can hold"))?;
        let rest_plyometric = rest_columns(programme.rest.plyometric)?;
        let rest_power = rest_columns(programme.rest.power)?;
        let rest_strength = rest_columns(programme.rest.strength)?;
        let rest_hypertrophy = rest_columns(programme.rest.hypertrophy)?;
        let rest_mobility = rest_columns(programme.rest.mobility)?;

        sqlx::query!(
            r"
            INSERT INTO programme (
                authored_at,
                strength_reps_low, strength_reps_high, strength_sets,
                hypertrophy_reps_low, hypertrophy_reps_high, hypertrophy_sets,
                static_hold_seconds,
                rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
                rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
                rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
                rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
                rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ",
            stamp,
            strength_low,
            strength_high,
            strength_sets,
            hypertrophy_low,
            hypertrophy_high,
            hypertrophy_sets,
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

        for (slot, scheme) in &programme.slots {
            let slot = slot.as_str();
            let (low, high) = reps_bounds(scheme.reps);
            let sets = count_for_storage(scheme.sets);
            sqlx::query!(
                r"
                INSERT INTO programme_slot_scheme (authored_at, slot, reps_low, reps_high, sets)
                VALUES (?, ?, ?, ?, ?)
                ",
                stamp,
                slot,
                low,
                high,
                sets
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

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
        let days = sqlx::query!(
            r#"
            SELECT week AS "week!: i64", session AS "session!: String", kind AS "kind!: String",
                   reps AS "reps!: i64", sets AS "sets: i64", share_bp AS "share_bp: i64",
                   back_off_sets AS "back_off_sets: i64",
                   back_off_reps_low AS "back_off_reps_low: i64",
                   back_off_reps_high AS "back_off_reps_high: i64"
            FROM sbs_chart_day
            WHERE authored_at = ?
            "#,
            stamp
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut found: BTreeMap<(i64, SbsSession), SbsDay> = BTreeMap::new();
        for day in days {
            let session = match day.session.as_str() {
                "first" => SbsSession::First,
                "second" => SbsSession::Second,
                other => return Err(corrupt(&format!("{other:?} is not a session of the chart"))),
            };
            let reps = count_from_storage(day.reps)?;
            let read = match (
                day.kind.as_str(),
                day.sets,
                day.share_bp,
                day.back_off_sets,
                day.back_off_reps_low,
            ) {
                ("percentage", Some(sets), Some(share), None, None) => SbsDay::Percentage {
                    sets: count_from_storage(sets)?,
                    reps,
                    share: bp_from_storage(share)?,
                },
                ("rep_max", None, None, Some(sets), Some(low)) => SbsDay::RepMax {
                    reps,
                    back_off_sets: count_from_storage(sets)?,
                    back_off_reps: reps_target(low, day.back_off_reps_high)?,
                },
                ("test", None, None, None, None) => SbsDay::Test { reps },
                _ => {
                    return Err(corrupt(
                        &"a stored chart day whose columns do not fit its kind",
                    ));
                }
            };
            found.insert((day.week, session), read);
        }
        let mut week = |at: i64| -> Result<SbsWeek, StoreError> {
            let mut day = |session| {
                found.remove(&(at, session)).ok_or_else(|| {
                    corrupt(&format!(
                        "the stored chart has no week {at} {session} session"
                    ))
                })
            };
            Ok(SbsWeek {
                first: day(SbsSession::First)?,
                second: day(SbsSession::Second)?,
            })
        };
        let chart = SbsChart::new([week(1)?, week(2)?, week(3)?, week(4)?])
            .map_err(|error| corrupt(&error))?;

        Ok(Some((
            stamp_from_storage(&stamp)?,
            SbsProgression {
                warmup: ramp(
                    steps
                        .into_iter()
                        .map(|step| (step.of_top_set_bp, step.reps))
                        .collect(),
                )?,
                chart,
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

        for (week, session, day) in sbs.chart.days() {
            let week = i64::from(week);
            let session = session.as_str();
            let (kind, reps, sets, share, back_off_sets, back_off_low, back_off_high) = match day {
                SbsDay::Percentage { sets, reps, share } => (
                    "percentage",
                    count_for_storage(reps),
                    Some(count_for_storage(sets)),
                    Some(bp_for_storage(share)),
                    None,
                    None,
                    None,
                ),
                SbsDay::RepMax {
                    reps,
                    back_off_sets,
                    back_off_reps,
                } => {
                    let (low, high) = reps_bounds(back_off_reps);
                    (
                        "rep_max",
                        count_for_storage(reps),
                        None,
                        None,
                        Some(count_for_storage(back_off_sets)),
                        Some(low),
                        high,
                    )
                }
                SbsDay::Test { reps } => (
                    "test",
                    count_for_storage(reps),
                    None,
                    None,
                    None,
                    None,
                    None,
                ),
            };
            sqlx::query!(
                r"
                INSERT INTO sbs_chart_day (
                    authored_at, week, session, kind, reps, sets, share_bp,
                    back_off_sets, back_off_reps_low, back_off_reps_high
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                stamp,
                week,
                session,
                kind,
                reps,
                sets,
                share,
                back_off_sets,
                back_off_low,
                back_off_high
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        }

        tx.commit().await.map_err(|error| store_error(&error))
    }
}
