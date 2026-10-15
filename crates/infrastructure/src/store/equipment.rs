//! Equipment facts, bitemporally (issue #450).
//!
//! A row is one fact: a kind or a specific, true from `valid_from` (null for
//! since before anything was recorded), entered at `recorded_at`. A correction
//! is a row naming the one it supersedes in `corrects`, and what is now known
//! is every row no other row corrects. Nothing is updated or deleted.

use std::collections::BTreeMap;

use application::{EquipmentStore, StoreError};
use domain::{
    gym::{
        equipment::{
            EquipmentFacts, Fact, FactId, LevelName, Settings, SpecificName, Statement, Timeline,
        },
        exercise::Implement,
    },
    measure::Kg,
    prescription::{LoadSteps, Step},
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use super::{corrupt, store_error};

fn grams(stored: i64) -> Result<Kg, StoreError> {
    u64::try_from(stored)
        .map(Kg::from_grams)
        .map_err(|_| corrupt(&"a negative mass"))
}

fn stored(mass: Kg) -> Result<i64, StoreError> {
    i64::try_from(mass.as_grams()).map_err(|_| corrupt(&"a mass larger than the store can hold"))
}

/// Every stack's and kind's bands, by fact.
async fn read_bands(pool: &SqlitePool) -> Result<BTreeMap<i64, Vec<Step>>, StoreError> {
    let steps = sqlx::query!(
        r#"
        SELECT fact AS "fact!: i64",
               from_grams AS "from_grams!: i64",
               size_grams AS "size_grams!: i64"
        FROM equipment_fact_step
        ORDER BY fact, band
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    let mut bands: BTreeMap<i64, Vec<Step>> = BTreeMap::new();
    for row in steps {
        bands.entry(row.fact).or_default().push(Step {
            from: grams(row.from_grams)?,
            size: grams(row.size_grams)?,
        });
    }
    Ok(bands)
}

/// Every range's levels, by fact, in order.
async fn read_levels(pool: &SqlitePool) -> Result<BTreeMap<i64, Vec<LevelName>>, StoreError> {
    let levels = sqlx::query!(
        r#"
        SELECT fact AS "fact!: i64", name AS "name!: String"
        FROM equipment_fact_level
        ORDER BY fact, position
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    let mut named: BTreeMap<i64, Vec<LevelName>> = BTreeMap::new();
    for row in levels {
        named
            .entry(row.fact)
            .or_default()
            .push(LevelName::try_from(row.name).map_err(|error| corrupt(&error))?);
    }
    Ok(named)
}

/// Everything now known, read in three queries and assembled here.
pub(crate) async fn read_facts(pool: &SqlitePool) -> Result<EquipmentFacts, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT id AS "id!: i64",
               implement,
               specific,
               valid_from,
               recorded_at AS "recorded_at!: String",
               most_grams
        FROM equipment_fact
        WHERE id NOT IN (SELECT corrects FROM equipment_fact WHERE corrects IS NOT NULL)
        ORDER BY id
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut bands = read_bands(pool).await?;
    let mut named = read_levels(pool).await?;

    let mut kinds: BTreeMap<Implement, Vec<Fact<LoadSteps>>> = BTreeMap::new();
    let mut specifics: BTreeMap<SpecificName, Vec<Fact<Settings>>> = BTreeMap::new();
    for row in rows {
        let id = FactId::new(row.id);
        let from = row
            .valid_from
            .map(|date| date.parse::<Date>())
            .transpose()
            .map_err(|_| corrupt(&"a period that does not start on a date"))?;
        let recorded_at: Timestamp = row
            .recorded_at
            .parse()
            .map_err(|_| corrupt(&"a recording time that is not an instant"))?;
        let steps = bands.remove(&row.id);
        let levels = named.remove(&row.id);

        match (row.implement, row.specific) {
            (Some(implement), None) => {
                let implement = Implement::try_from(implement).map_err(|error| corrupt(&error))?;
                let value =
                    LoadSteps::new(steps.unwrap_or_default()).map_err(|error| corrupt(&error))?;
                kinds.entry(implement).or_default().push(Fact {
                    id,
                    from,
                    recorded_at,
                    value,
                });
            }
            (None, Some(specific)) => {
                let name = SpecificName::try_from(specific).map_err(|error| corrupt(&error))?;
                let value = match (steps, levels, row.most_grams) {
                    (Some(steps), None, Some(most)) => Settings::stack(
                        LoadSteps::new(steps).map_err(|error| corrupt(&error))?,
                        grams(most)?,
                    ),
                    (None, Some(levels), None) => Settings::levels(levels),
                    _ => return Err(corrupt(&"a specific that is neither a stack nor levels")),
                }
                .map_err(|error| corrupt(&error))?;
                specifics.entry(name).or_default().push(Fact {
                    id,
                    from,
                    recorded_at,
                    value,
                });
            }
            _ => return Err(corrupt(&"a fact about neither a kind nor a specific")),
        }
    }

    Ok(EquipmentFacts {
        kinds: kinds
            .into_iter()
            .map(|(implement, facts)| (implement, Timeline::new(facts)))
            .collect(),
        specifics: specifics
            .into_iter()
            .map(|(name, facts)| (name, Timeline::new(facts)))
            .collect(),
    })
}

async fn write_steps(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    fact: i64,
    steps: &LoadSteps,
) -> Result<(), StoreError> {
    for (band, step) in steps.bands().iter().enumerate() {
        let band = i64::try_from(band)
            .map_err(|_| corrupt(&"a scale with more bands than the store can hold"))?;
        let from = stored(step.from)?;
        let size = stored(step.size)?;
        sqlx::query!(
            r"
            INSERT INTO equipment_fact_step (fact, band, from_grams, size_grams)
            VALUES (?, ?, ?, ?)
            ",
            fact,
            band,
            from,
            size
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// Write one fact inside a transaction the caller owns.
///
/// Shared with the parameter store, which states a version's scales as the
/// kinds of a store that knows none.
pub(crate) async fn insert(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    statement: &Statement,
    from: Option<Date>,
    recorded_at: Timestamp,
    corrects: Option<FactId>,
) -> Result<FactId, StoreError> {
    let (implement, specific, most) = match statement {
        Statement::Kind(implement, _) => (Some(implement.as_str()), None, None),
        Statement::Specific(name, Settings::Stack { most, .. }) => {
            (None, Some(name.as_str()), Some(stored(*most)?))
        }
        Statement::Specific(name, Settings::Levels(_)) => (None, Some(name.as_str()), None),
    };
    let valid_from = from.map(|date| date.to_string());
    let recorded_at = recorded_at.to_string();
    let corrects = corrects.map(FactId::get);

    // `corrects` is UNIQUE, so a second correction of one fact is refused by
    // the schema: what is corrected is what is now believed, never an
    // already-superseded row.
    let fact = sqlx::query_scalar!(
        r#"
        INSERT INTO equipment_fact
            (implement, specific, valid_from, recorded_at, corrects, most_grams)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        implement,
        specific,
        valid_from,
        recorded_at,
        corrects,
        most
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    match statement {
        Statement::Kind(_, steps) | Statement::Specific(_, Settings::Stack { steps, .. }) => {
            write_steps(tx, fact, steps).await?;
        }
        Statement::Specific(_, Settings::Levels(levels)) => {
            for (position, level) in levels.iter().enumerate() {
                let position = i64::try_from(position)
                    .map_err(|_| corrupt(&"more levels than the store can hold"))?;
                let name = level.as_str();
                sqlx::query!(
                    r"
                    INSERT INTO equipment_fact_level (fact, position, name)
                    VALUES (?, ?, ?)
                    ",
                    fact,
                    position,
                    name
                )
                .execute(&mut **tx)
                .await
                .map_err(|error| store_error(&error))?;
            }
        }
    }
    Ok(FactId::new(fact))
}

/// Whether any kind is stated.
pub(crate) async fn knows_kinds(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<bool, StoreError> {
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!: i64" FROM equipment_fact WHERE implement IS NOT NULL"#
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(count > 0)
}

#[derive(Debug, Clone)]
pub struct SqliteEquipmentStore {
    pool: SqlitePool,
}

impl SqliteEquipmentStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl EquipmentStore for SqliteEquipmentStore {
    async fn facts(&self) -> Result<EquipmentFacts, StoreError> {
        read_facts(&self.pool).await
    }

    async fn record(
        &self,
        statement: &Statement,
        from: Option<Date>,
        recorded_at: Timestamp,
        corrects: Option<FactId>,
    ) -> Result<FactId, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        let fact = insert(&mut tx, statement, from, recorded_at, corrects).await?;
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(fact)
    }
}
