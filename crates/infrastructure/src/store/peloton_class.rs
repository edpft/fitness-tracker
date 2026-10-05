//! Every power zone class Peloton serves, held locally (#246).
//!
//! **Outside § II, and not by omission.** § II governs observation data —
//! records of what happened — and a class is not one: it is what Peloton
//! publishes and what the operator is asked to ride, which § II.11 keeps on the
//! prescribed side. § II is explicit that data which is not an observation
//! "acquires no raw, normalised or canonical layer", so there is no landing
//! table here, no entry in `cli::catalogue`, no resumption point and no
//! normalised entity.
//!
//! § 14 is the section that reaches it. A published class is a fact about the
//! world the programme is run in rather than a fact about how it prescribes,
//! which is § 14.1's test, and § 14 already names this case: such a value
//! "arrives through an adapter behind a port (§ 16), and need not be persisted
//! at all". It was not persisted until now — `HoldingRides` asked Peloton every
//! time. Persisting it is what § 14 permits and this issue wants, and § 14 then
//! says what the table may be: **only the current value is required**, so one
//! row per class, replaced when Peloton serves it differently, with no history.
//!
//! **The detail response is kept beside the listing fields.** It is a column
//! and not a layer: nothing here claims to be raw. What it buys is the thing
//! § 7 wants — a corrected reader re-reads the store instead of re-fetching the
//! library — and `peloton::class`'s own history is why that matters, since the
//! transcription it replaced had a cool-down five minutes out.
//!
//! **Two states, and the second is reached a page at a time.** A class is
//! *listed* as soon as the browse walk sees it, and *read* once its detail has
//! been fetched. The walk is cheap and runs whole; the reads are one request
//! each and are bounded per run, because this happens inside `fitness next` and
//! the operator's daily loop may not become a library download.

use application::StoreError;
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::store_error;
use crate::peloton::{ClassSession, ClassSummary, class};

/// Peloton's class library, as this store holds it.
#[derive(Debug, Clone)]
pub struct SqlitePelotonClassStore {
    pool: SqlitePool,
}

/// What the catalogue holds, for the one line `fitness next` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Held {
    /// Every class the browse walk has seen.
    pub listed: u64,
    /// How many of those have had their detail read.
    pub read: u64,
}

impl Held {
    /// How many are listed but not yet read.
    #[must_use]
    pub const fn outstanding(&self) -> u64 {
        self.listed.saturating_sub(self.read)
    }
}

impl SqlitePelotonClassStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Record what the browse walk listed, and say how many were new.
    ///
    /// **An existing row keeps its detail.** A class Peloton re-lists is the
    /// same class, and discarding what was read of it would make every walk
    /// undo the reading the walks before it paid for. The listing fields are
    /// replaced, because § 14 holds the current value.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn record_listed(&self, classes: &[ClassSummary]) -> Result<u64, StoreError> {
        let listed_at = Timestamp::now().to_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        // **Counted, not inferred.** An upsert reports one row affected
        // whether it inserted or updated, so how many were new is the
        // difference the whole batch made to the table.
        let before = sqlx::query!(r#"SELECT COUNT(*) AS "held!: i64" FROM peloton_class"#)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?
            .held;
        for found in classes {
            let duration = i64::try_from(found.duration_seconds).unwrap_or(i64::MAX);
            sqlx::query!(
                r#"
                INSERT INTO peloton_class
                       (reference, title, duration, series, instructor, aired_at, listed_at)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT (reference) DO UPDATE SET
                       title      = excluded.title,
                       duration   = excluded.duration,
                       series     = excluded.series,
                       instructor = excluded.instructor,
                       aired_at   = excluded.aired_at,
                       listed_at  = excluded.listed_at
                "#,
                found.id,
                found.title,
                duration,
                found.series,
                found.instructor,
                found.aired_at,
                listed_at,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?;
        }
        let after = sqlx::query!(r#"SELECT COUNT(*) AS "held!: i64" FROM peloton_class"#)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?
            .held;
        transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))?;
        Ok(u64::try_from(after - before).unwrap_or_default())
    }

    /// Which classes are listed but not read, newest first, at most `limit`.
    ///
    /// **Newest first because that is what gets ridden.** A library read oldest
    /// first would spend its first runs on classes a decade old while this
    /// week's were still missing.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn unread(&self, limit: u32) -> Result<Vec<String>, StoreError> {
        let limit = i64::from(limit);
        let rows = sqlx::query!(
            r#"
            SELECT reference AS "reference!: String"
              FROM peloton_class
             WHERE detail IS NULL
             ORDER BY aired_at DESC NULLS LAST, reference ASC
             LIMIT ?
            "#,
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(rows.into_iter().map(|row| row.reference).collect())
    }

    /// Keep what `/api/ride/{id}/details` served for one class.
    ///
    /// **A class the listing never carried is held anyway.** `mapping` records
    /// that one class of *Peak Your Power Zones* is unavailable to the
    /// operator's account, and the browse listing is not obliged to carry it —
    /// so a detail may be the first and only thing the catalogue ever sees of a
    /// class the skeletons place. Refusing it would leave `plan` with a hole in
    /// Peak, which is the one thing `peloton::provider` exists to refuse.
    ///
    /// So the listing row is inserted if it is missing and **left alone if it
    /// is not**. That second half is load-bearing: the detail carries no
    /// `series_id`, and overwriting a walked row from it would wipe the series
    /// every candidate search turns on.
    ///
    /// `title` and `duration` are the caller's reading of the detail, used only
    /// when the row has to be created.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn record_detail(
        &self,
        reference: &str,
        title: &str,
        duration_seconds: u64,
        body: &str,
    ) -> Result<(), StoreError> {
        let now = Timestamp::now().to_string();
        let duration = i64::try_from(duration_seconds).unwrap_or(i64::MAX).max(1);
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!(
            r#"
            INSERT INTO peloton_class (reference, title, duration, listed_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (reference) DO NOTHING
            "#,
            reference,
            title,
            duration,
            now,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?;
        sqlx::query!(
            r#"
            UPDATE peloton_class
               SET detail = ?, read_at = ?
             WHERE reference = ?
            "#,
            body,
            now,
            reference,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?;
        transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))?;
        Ok(())
    }

    /// Whether the catalogue has read this class's detail.
    ///
    /// **Asked rather than derived from [`class`](Self::class).** That one
    /// parses the whole stored response, and this is the question the refresh
    /// asks of every class the skeletons place on every single run.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn is_read(&self, reference: &str) -> Result<bool, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT detail IS NOT NULL AS "read!: i64"
              FROM peloton_class
             WHERE reference = ?
            "#,
            reference,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(row.is_some_and(|row| row.read != 0))
    }

    /// What the catalogue holds.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn held(&self) -> Result<Held, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT COUNT(*)                                     AS "listed!: i64",
                   COALESCE(SUM(detail IS NOT NULL), 0)         AS "read!: i64"
              FROM peloton_class
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(Held {
            listed: u64::try_from(row.listed).unwrap_or_default(),
            read: u64::try_from(row.read).unwrap_or_default(),
        })
    }

    /// What one class prescribes, from the detail the catalogue holds.
    ///
    /// `None` where the catalogue does not hold the class, or holds it listed
    /// but not yet read. Both are real answers: the caller decides whether to
    /// wait for the next refresh or go to the source.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] where the stored detail will not read as a
    /// class, which means the store holds something this program did not put
    /// there, and [`StoreError`] if the store is unavailable.
    pub async fn class(&self, reference: &str) -> Result<Option<ClassSession>, StoreError> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT detail AS "detail: String"
              FROM peloton_class
             WHERE reference = ?
            "#,
            reference,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        else {
            return Ok(None);
        };
        let Some(detail) = row.detail else {
            return Ok(None);
        };
        class::derive(reference, &detail)
            .map(Some)
            .map_err(|error| StoreError::Corrupt {
                detail: format!("the catalogue's copy of class {reference} will not read: {error}"),
            })
    }

    /// Every class of one series and one length the catalogue holds, newest
    /// first.
    ///
    /// **The order Peloton's own browse gave**, newest first, because that is
    /// what "the newest that hasn't already been taken" means and it must not
    /// change now that the question is asked of the store.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn in_series(
        &self,
        series: &str,
        duration_seconds: u64,
    ) -> Result<Vec<ClassSummary>, StoreError> {
        let duration = i64::try_from(duration_seconds).unwrap_or(i64::MAX);
        let rows = sqlx::query!(
            r#"
            SELECT reference  AS "reference!: String",
                   title      AS "title!: String",
                   duration   AS "duration!: i64",
                   series     AS "series: String",
                   instructor AS "instructor: String",
                   aired_at   AS "aired_at: i64"
              FROM peloton_class
             WHERE series = ? AND duration = ?
             ORDER BY aired_at DESC NULLS LAST, reference ASC
            "#,
            series,
            duration,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(rows
            .into_iter()
            .map(|row| ClassSummary {
                id: row.reference,
                title: row.title,
                duration_seconds: u64::try_from(row.duration).unwrap_or_default(),
                series: row.series,
                instructor: row.instructor,
                aired_at: row.aired_at,
            })
            .collect())
    }
}
