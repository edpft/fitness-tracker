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
//! **Three states, and the third is how the second one ends.** A class is
//! *listed* as soon as the browse walk sees it, *read* once its detail has been
//! fetched, and *not served* where Peloton has no detail to give for it — a 404
//! or a 410 on an id the listing itself handed us. `mapping` already records a
//! candidate: a *Peak Your Power Zones* ride the operator cannot start. Before
//! #369 such a class was simply left unread, which was invisible while the
//! reads were bounded at fifty a run. Unbounded, it would be asked for on every
//! single `fitness next` and the catalogue could never say it had read
//! everything there is.
//!
//! **Only those two statuses, and a 403 is not one of them.** `class` has the
//! argument: #368 records that Peloton answers 403 for throttling as readily as
//! for refusal, and a thousand requests in a row is when throttling happens, so
//! reading it as "no detail" would write off the library silently and for
//! good.
//!
//! **Not served, rather than refused.** A *refusal* in this codebase is the
//! system declining something — what the domain will not accept
//! (`store::refusals`), what a constraint will not admit, a credential this
//! build turned down. This is the other direction, and one word for two
//! directions is the drift `CLAUDE.md` warns about. It is also not a refusal in
//! plain English: Peloton is not withholding the class, it has nothing to give
//! for that id.
//!
//! So it is recorded, and such a class is neither offered as unread nor counted
//! as outstanding. **It is not reconsidered**: what has been seen so far is a
//! fact about what the account may read, and re-asking on a schedule would be a
//! knob (§ 14.1) bought with a request per run. A body that Peloton *does*
//! serve is stored even where the reader cannot make a session of it, for the
//! reason above — a corrected reader should cost a re-read and not a re-fetch —
//! so a reader fault is never recorded as the source withholding anything.

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
    /// How many Peloton has no detail to serve for.
    pub not_served: u64,
}

impl Held {
    /// How many are listed and still worth asking for.
    ///
    /// **A class the source has no detail for is accounted for, not outstanding.**
    /// Counting it as outstanding would mean the catalogue could never report
    /// that it had read everything there is to read, which is the number #369
    /// exists to make reach zero.
    #[must_use]
    pub const fn outstanding(&self) -> u64 {
        self.listed
            .saturating_sub(self.read)
            .saturating_sub(self.not_served)
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

    /// Which classes are listed and still worth asking Peloton for, newest first.
    ///
    /// **Newest first because that is what gets ridden.** A library read oldest
    /// first would spend its first runs on classes a decade old while this
    /// week's were still missing — which is what the order buys now that the
    /// whole set is read in one go: a refresh interrupted part-way through has
    /// still read the classes most likely to be chosen.
    ///
    /// **No bound** (#369). It was `at most limit` until the limit turned out
    /// to be a count per run rather than a pace, which left 1,002 of the
    /// operator's 1,052 classes with no zone structure three weeks after the
    /// catalogue was built. Pacing belongs to the caller, which is making one
    /// request per answer; this is one query.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn unread(&self) -> Result<Vec<String>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT reference AS "reference!: String"
              FROM peloton_class
             WHERE detail IS NULL AND not_served_at IS NULL
             ORDER BY aired_at DESC NULLS LAST, reference ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(rows.into_iter().map(|row| row.reference).collect())
    }

    /// Record that Peloton has no detail to serve for this class.
    ///
    /// **A row is created where there is none.** A class the skeletons place
    /// but the browse listing never carried is exactly the case this exists
    /// for, and it has no listing row — so the fact would have nowhere to live
    /// and the class would be asked for again on every run.
    ///
    /// Such a row carries the class's id as its title and a length of one
    /// second, because that is all the catalogue knows of it and the columns
    /// are `NOT NULL`. Nothing reads them: it has no series and no plausible
    /// length, so no candidate search can return it, and with no detail it is
    /// never prescribed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn record_not_served(&self, reference: &str) -> Result<(), StoreError> {
        let now = Timestamp::now().to_string();
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!(
            r#"
            INSERT INTO peloton_class (reference, title, duration, listed_at)
            VALUES (?, ?, 1, ?)
            ON CONFLICT (reference) DO NOTHING
            "#,
            reference,
            reference,
            now,
        )
        .execute(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?;
        sqlx::query!(
            r#"
            UPDATE peloton_class
               SET not_served_at = ?
             WHERE reference = ?
            "#,
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
               SET detail = ?, read_at = ?, not_served_at = NULL
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

    /// Whether this class has been asked for and answered, one way or another.
    ///
    /// True where the catalogue holds the detail, and true where Peloton had
    /// none to give. **Both are answers**, and the one question the
    /// refresh asks of each class the skeletons place is whether there is
    /// anything left to ask Peloton about it.
    ///
    /// **Asked rather than derived from [`class`](Self::class).** That one
    /// parses the whole stored response, and this runs over every placed class
    /// on every single run.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn is_accounted_for(&self, reference: &str) -> Result<bool, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT detail IS NOT NULL OR not_served_at IS NOT NULL AS "accounted!: i64"
              FROM peloton_class
             WHERE reference = ?
            "#,
            reference,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(row.is_some_and(|row| row.accounted != 0))
    }

    /// What the catalogue holds.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn held(&self) -> Result<Held, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT COUNT(*)                                             AS "listed!: i64",
                   COALESCE(SUM(detail IS NOT NULL), 0)                 AS "read!: i64",
                   COALESCE(SUM(detail IS NULL AND not_served_at IS NOT NULL), 0)
                                                                        AS "not_served!: i64"
              FROM peloton_class
            "#,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(Held {
            listed: u64::try_from(row.listed).unwrap_or_default(),
            read: u64::try_from(row.read).unwrap_or_default(),
            not_served: u64::try_from(row.not_served).unwrap_or_default(),
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
