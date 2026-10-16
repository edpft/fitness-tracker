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
            Band, BandName, BandRange, Combination, EquipmentFacts, Fact, FactId, Maker, Model,
            Specific, Stack, Statement, Timeline,
        },
        exercise::Implement,
    },
    measure::Kg,
    prescription::{LoadSteps, Step},
    sequence::NonEmpty,
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

/// Every range's bands, by fact, in order.
async fn read_band_ranges(pool: &SqlitePool) -> Result<BTreeMap<i64, Vec<Band>>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT fact AS "fact!: i64", name AS "name!: String", twice_grams AS "twice_grams!: i64"
        FROM equipment_fact_band
        ORDER BY fact, position
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    let mut ranges: BTreeMap<i64, Vec<Band>> = BTreeMap::new();
    for row in rows {
        ranges.entry(row.fact).or_default().push(Band {
            name: BandName::try_from(row.name).map_err(|error| corrupt(&error))?,
            at_twice_length: grams(row.twice_grams)?,
        });
    }
    Ok(ranges)
}

/// Every range's declared steps, by fact, in order.
async fn read_band_steps(pool: &SqlitePool) -> Result<BTreeMap<i64, Vec<Combination>>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT fact AS "fact!: i64", step AS "step!: i64", band AS "band!: String"
        FROM equipment_fact_band_step
        ORDER BY fact, step, part
        "#
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;
    let mut parts: BTreeMap<(i64, i64), Vec<BandName>> = BTreeMap::new();
    for row in rows {
        parts
            .entry((row.fact, row.step))
            .or_default()
            .push(BandName::try_from(row.band).map_err(|error| corrupt(&error))?);
    }
    let mut steps: BTreeMap<i64, Vec<Combination>> = BTreeMap::new();
    for ((fact, _), bands) in parts {
        let bands = NonEmpty::new(bands).map_err(|_| corrupt(&"a step of no bands"))?;
        steps.entry(fact).or_default().push(Combination::new(bands));
    }
    Ok(steps)
}

/// Everything now known, read in four queries and assembled here.
pub(crate) async fn read_facts(pool: &SqlitePool) -> Result<EquipmentFacts, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT id AS "id!: i64",
               category AS "category!: String",
               implement,
               maker,
               model,
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
    let mut ranges = read_band_ranges(pool).await?;
    let mut charts = read_band_steps(pool).await?;

    let mut kinds: BTreeMap<Implement, Vec<Fact<LoadSteps>>> = BTreeMap::new();
    let mut machines: BTreeMap<Specific, Vec<Fact<Stack>>> = BTreeMap::new();
    let mut ranges_of: BTreeMap<Specific, Vec<Fact<BandRange>>> = BTreeMap::new();
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
        let bands = ranges.remove(&row.id);
        let chart = charts.remove(&row.id);

        match row.category.as_str() {
            "kind" => {
                let implement = row
                    .implement
                    .ok_or_else(|| corrupt(&"a kind with no implement"))?;
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
            "machine" => {
                let most = row
                    .most_grams
                    .ok_or_else(|| corrupt(&"a machine with no most"))?;
                let value = Stack::new(
                    LoadSteps::new(steps.unwrap_or_default()).map_err(|error| corrupt(&error))?,
                    grams(most)?,
                )
                .map_err(|error| corrupt(&error))?;
                machines
                    .entry(specific_of(row.maker, row.model)?)
                    .or_default()
                    .push(Fact {
                        id,
                        from,
                        recorded_at,
                        value,
                    });
            }
            "bands" => {
                let value = BandRange::new(bands.unwrap_or_default(), chart.unwrap_or_default())
                    .map_err(|error| corrupt(&error))?;
                ranges_of
                    .entry(specific_of(row.maker, row.model)?)
                    .or_default()
                    .push(Fact {
                        id,
                        from,
                        recorded_at,
                        value,
                    });
            }
            other => {
                return Err(corrupt(&format!(
                    "{other:?} is not a category of equipment"
                )));
            }
        }
    }

    Ok(EquipmentFacts {
        kinds: timelines(kinds),
        machines: timelines(machines),
        bands: timelines(ranges_of),
    })
}

fn specific_of(maker: Option<String>, model: Option<String>) -> Result<Specific, StoreError> {
    match (maker, model) {
        (Some(maker), Some(model)) => Ok(Specific {
            maker: Maker::try_from(maker).map_err(|error| corrupt(&error))?,
            model: Model::try_from(model).map_err(|error| corrupt(&error))?,
        }),
        _ => Err(corrupt(
            &"a machine or a range of bands with no maker and model",
        )),
    }
}

fn timelines<K: Ord, T>(facts: BTreeMap<K, Vec<Fact<T>>>) -> BTreeMap<K, Timeline<T>> {
    facts
        .into_iter()
        .map(|(key, facts)| (key, Timeline::new(facts)))
        .collect()
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

async fn write_bands(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    fact: i64,
    bands: &NonEmpty<Band>,
    steps: &NonEmpty<Combination>,
) -> Result<(), StoreError> {
    let index =
        |at: usize| i64::try_from(at).map_err(|_| corrupt(&"more bands than the store can hold"));
    for (position, band) in bands.iter().enumerate() {
        let position = index(position)?;
        let name = band.name.as_str();
        let twice = stored(band.at_twice_length)?;
        sqlx::query!(
            r"
            INSERT INTO equipment_fact_band (fact, position, name, twice_grams)
            VALUES (?, ?, ?, ?)
            ",
            fact,
            position,
            name,
            twice
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    }
    for (step, combination) in steps.iter().enumerate() {
        let step = index(step)?;
        for (part, band) in combination.iter().enumerate() {
            let part = index(part)?;
            let band = band.as_str();
            sqlx::query!(
                r"
                INSERT INTO equipment_fact_band_step (fact, step, part, band)
                VALUES (?, ?, ?, ?)
                ",
                fact,
                step,
                part,
                band
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
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
    let (category, implement, specific, most) = match statement {
        Statement::Kind(implement, _) => ("kind", Some(implement.as_str()), None, None),
        Statement::Machine(specific, stack) => {
            ("machine", None, Some(specific), Some(stored(stack.most())?))
        }
        Statement::Bands(specific, _) => ("bands", None, Some(specific), None),
    };
    let maker = specific.map(|specific| specific.maker.as_str());
    let model = specific.map(|specific| specific.model.as_str());
    let valid_from = from.map(|date| date.to_string());
    let recorded_at = recorded_at.to_string();
    let corrects = corrects.map(FactId::get);

    // `corrects` is UNIQUE, so a second correction of one fact is refused by
    // the schema: what is corrected is what is now believed, never an
    // already-superseded row.
    let fact = sqlx::query_scalar!(
        r#"
        INSERT INTO equipment_fact
            (category, implement, maker, model, valid_from, recorded_at, corrects, most_grams)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        category,
        implement,
        maker,
        model,
        valid_from,
        recorded_at,
        corrects,
        most
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    match statement {
        Statement::Kind(_, steps) => write_steps(tx, fact, steps).await?,
        Statement::Machine(_, stack) => write_steps(tx, fact, stack.steps()).await?,
        Statement::Bands(_, range) => {
            write_bands(tx, fact, range.bands(), range.steps()).await?;
        }
    }
    Ok(FactId::new(fact))
}

/// Whether any kind is stated.
pub(crate) async fn knows_kinds(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<bool, StoreError> {
    let count = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!: i64" FROM equipment_fact WHERE category = 'kind'"#
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
