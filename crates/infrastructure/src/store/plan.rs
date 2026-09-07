//! The authored plan (§ 12), and everything under it.
//!
//! **The plan is the authored unit** (issue #86). Until 2026-09-06 the store
//! held four rows per discipline and no plan at all: "the autumn" existed
//! nowhere, and what held four mesocycles together was successive start dates
//! and a shared stem in their names. A plan row holds them now, and re-authoring
//! it supersedes every mesocycle in it at once.
//!
//! **Written whole, in one transaction.** A plan half-written — its gym side
//! stored and its cycling side not — is not a state worth being able to hold,
//! and the alternative is an operator who has to know which of eight authorings
//! failed.
//!
//! **There is no table for the programme rung**, and that is deliberate. A row
//! holding a plan id and a discipline the table name already states would carry
//! nothing: the gym programme *is* the `gym_mesocycle` rows under a plan, in
//! `ordinal` order.

use application::{PlanStore, StoreError};
use domain::{
    gym::OperatorZone,
    plan::{Plan, PlanId, PlanName, PlanWindow, Programme},
};
use sqlx::SqlitePool;

use super::{corrupt, cycling_mesocycle, gym_mesocycle, store_error};

/// The authored plan, in SQLite.
#[derive(Debug, Clone)]
pub struct SqlitePlanStore {
    pool: SqlitePool,
    /// The zone the operator declares they train in.
    ///
    /// Configuration rather than plan data, so it is supplied here and not read
    /// from a row (§ II.3). The gym calendar needs one to place a date.
    zone: OperatorZone,
}

impl SqlitePlanStore {
    pub const fn new(pool: SqlitePool, zone: OperatorZone) -> Self {
        Self { pool, zone }
    }

    /// Every plan in force, with both its programmes.
    ///
    /// **The plan in force under a name is its latest authoring.** Earlier
    /// versions stay in the file — nothing is deleted (§ 12) — and are never
    /// read, which is what makes correcting the autumn legal without competing
    /// with the autumn it corrects.
    async fn in_force(&self) -> Result<Vec<(PlanId, Plan)>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64", name AS "name!: String",
                   authored_at AS "authored_at!: String"
            FROM plan AS p
            WHERE p.authored_at = (
                SELECT MAX(q.authored_at) FROM plan AS q WHERE q.name = p.name
            )
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let gym = gym_mesocycle::in_force(&self.pool, &self.zone).await?;
        let cycling = cycling_mesocycle::in_force(&self.pool).await?;

        let mut plans = Vec::with_capacity(rows.len());
        for row in rows {
            let name = PlanName::try_from(row.name).map_err(|error| corrupt(&error))?;
            let authored_at = row
                .authored_at
                .parse()
                .map_err(|_| corrupt(&"an authoring time that is not a timestamp"))?;

            // Both lists arrive ordered by start date, and a plan's mesocycles
            // do not overlap, so filtering preserves the order they are run in.
            let gym_side: Vec<_> = gym
                .iter()
                .filter(|(plan, ..)| *plan == row.id)
                .map(|(.., mesocycle)| mesocycle.clone())
                .collect();
            let cycling_side: Vec<_> = cycling
                .iter()
                .filter(|(plan, ..)| *plan == row.id)
                .map(|(.., mesocycle)| mesocycle.clone())
                .collect();

            // Empty is absence, not a fault: a gym-only plan is what the store
            // held all autumn, and a cycling-only one is what `fitness plan`
            // wrote before the two were joined.
            let gym_side = if gym_side.is_empty() {
                None
            } else {
                Some(Programme::new(gym_side).map_err(|error| corrupt(&error))?)
            };
            let cycling_side = if cycling_side.is_empty() {
                None
            } else {
                Some(Programme::new(cycling_side).map_err(|error| corrupt(&error))?)
            };

            let plan = Plan::new(name, authored_at, gym_side, cycling_side)
                .map_err(|error| corrupt(&error))?;
            plans.push((PlanId::new(row.id), plan));
        }
        Ok(plans)
    }
}

impl PlanStore for SqlitePlanStore {
    async fn windows(&self) -> Result<Vec<PlanWindow>, StoreError> {
        Ok(self
            .in_force()
            .await?
            .iter()
            .map(|(_, plan)| plan.window())
            .collect())
    }

    async fn named(&self, name: &PlanName) -> Result<Option<Plan>, StoreError> {
        Ok(self
            .in_force()
            .await?
            .into_iter()
            .find(|(_, plan)| plan.name() == name)
            .map(|(_, plan)| plan))
    }

    async fn author(&self, plan: &Plan) -> Result<PlanId, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let name = plan.name().to_string();
        let authored_at = plan.authored_at().to_string();
        let id = sqlx::query!(
            r#"
            INSERT INTO plan (name, authored_at)
            VALUES (?, ?)
            RETURNING id AS "id!: i64"
            "#,
            name,
            authored_at
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        if let Some(gym) = plan.gym() {
            for (at, mesocycle) in gym.mesocycles().enumerate() {
                gym_mesocycle::write(&mut tx, id, ordinal(at)?, mesocycle).await?;
            }
        }
        if let Some(cycling) = plan.cycling() {
            for (at, mesocycle) in cycling.mesocycles().enumerate() {
                cycling_mesocycle::write(&mut tx, id, ordinal(at)?, mesocycle).await?;
            }
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(PlanId::new(id))
    }
}

/// A mesocycle's place in its programme, counting from one as the schema does.
fn ordinal(at: usize) -> Result<i64, StoreError> {
    i64::try_from(at.saturating_add(1))
        .map_err(|_| corrupt(&"more mesocycles than the store can number"))
}
