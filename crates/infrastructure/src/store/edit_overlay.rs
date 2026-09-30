//! The edit overlay, in the store (§ II.2).
//!
//! **A peer of raw, not a derivation of it.** It sits in its own tables, it is
//! read while the normalised layer is rebuilt, and nothing in it is ever
//! written into a derived row — so removing a correction and re-deriving
//! restores exactly what the source said. That is the whole reason it is an
//! input and not an update.
//!
//! **Unlike raw it is not append-only**, and it carries no triggers to make it
//! so: retraction is the point of an override you can undo. The landing tables
//! guard an input a source owns; this one holds what the operator says, and he
//! is allowed to change his mind.
//!
//! **No port, for the reason [`super::settings`] has none.** An overlay is
//! read while working out what a derivation means and handed to the translator
//! already resolved, which is composition rather than application. A port in
//! `application` would be a port no use case calls.
//!
//! **Two tables because bulk is the action and not the record.** One assertion
//! is dated and given a reason once; the observations it matched when it was
//! made are rows beneath it. A record landed afterwards is not among them,
//! which is § II.2's rule that overrides do not propagate.

use application::StoreError;
use domain::gym::exercise::Exercise;
use domain::{
    landing::{InvalidStream, LandingStream, SourceRecordId},
    normalised::{
        CorrectedTerm, Correction, CorrectionId, CorrectionReason, EditOverlay, SourceTerm,
    },
    sequence::NonEmpty,
};
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::store_error;

/// A correction read back from the store that the vocabulary no longer holds.
///
/// Its own error rather than a `StoreError`: the store is working perfectly
/// well, and what has gone wrong is that a key was renamed without the
/// migration that renames it here too. Naming the key is the point — it is the
/// column to go and fix.
#[derive(Debug, thiserror::Error)]
#[error("correction {id} names exercise {key}, which this vocabulary has no member for")]
pub struct UnknownCorrectedExercise {
    pub id: i64,
    pub key: String,
}

/// The corrections, in the store.
#[derive(Debug, Clone)]
pub struct SqliteEditOverlayStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteEditOverlayStore {
    /// The stream is a constructor argument for [`super::refusals`]'s reason:
    /// every stream's corrections share a table, because what a correction is
    /// does not vary by source and a table per stream would grow with the
    /// catalogue.
    ///
    /// # Errors
    ///
    /// [`InvalidStream`] if the stream constant it is given is not a stream
    /// name.
    pub fn new(pool: SqlitePool, stream: &str) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(stream)?,
        })
    }

    /// Every correction in force for this stream, oldest assertion first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read, and
    /// [`UnknownCorrectedExercise`] if a stored key is not in the vocabulary.
    pub async fn all(&self) -> Result<Vec<Correction>, OverlayError> {
        let stream = self.stream.to_string();
        let rows = sqlx::query!(
            r#"
            SELECT c.id          AS "id!: i64",
                   c.exercise    AS "exercise!: String",
                   c.asserted_at AS "asserted_at!: String",
                   c.reason      AS "reason!: String",
                   t.source_record_id AS "source_record_id!: String",
                   t.term             AS "term!: String"
            FROM exercise_correction c
            JOIN exercise_correction_term t ON t.correction = c.id
            WHERE c.stream = ?
            ORDER BY c.asserted_at, c.id, t.source_record_id, t.term
            "#,
            stream
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| OverlayError::Store(store_error(&error)))?;

        // Grouped in one pass, which the ORDER BY above makes possible: the rows
        // for one correction are contiguous, so a new id starts a new correction
        // and nothing needs a map.
        let mut corrections: Vec<Correction> = Vec::new();
        let mut gathering: Option<Gathering> = None;

        for row in rows {
            match gathering.as_mut() {
                Some(held) if held.id == row.id => {
                    held.terms
                        .push(corrected_term(&row.source_record_id, &row.term)?);
                }
                _ => {
                    if let Some(held) = gathering.take() {
                        corrections.push(held.built()?);
                    }
                    gathering = Some(Gathering {
                        id: row.id,
                        exercise: row.exercise,
                        asserted_at: row.asserted_at,
                        reason: row.reason,
                        terms: vec![corrected_term(&row.source_record_id, &row.term)?],
                    });
                }
            }
        }
        if let Some(held) = gathering.take() {
            corrections.push(held.built()?);
        }

        Ok(corrections)
    }

    /// The overlay those corrections make, ready for a translator.
    ///
    /// # Errors
    ///
    /// As [`Self::all`].
    pub async fn overlay(&self) -> Result<EditOverlay, OverlayError> {
        Ok(EditOverlay::of(&self.all().await?))
    }

    /// Record one assertion, returning what the store calls it.
    ///
    /// One transaction: an assertion whose terms did not all land would be an
    /// assertion reaching fewer observations than the operator was told.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn assert(
        &self,
        exercise: Exercise,
        asserted_at: Timestamp,
        reason: &CorrectionReason,
        terms: &NonEmpty<CorrectedTerm>,
    ) -> Result<CorrectionId, StoreError> {
        let stream = self.stream.to_string();
        let key = exercise.as_str().to_owned();
        let at = asserted_at.to_string();
        let reason = reason.as_str().to_owned();

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let id = sqlx::query!(
            r#"
            INSERT INTO exercise_correction (stream, exercise, asserted_at, reason)
            VALUES (?, ?, ?, ?)
            RETURNING id AS "id!: i64"
            "#,
            stream,
            key,
            at,
            reason
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        // `terms.iter()` rather than `&terms`: the borrowing
        // `IntoIterator` hands back a boxed `dyn Iterator`, which is not `Send`,
        // and holding one across the await below makes the whole future unusable
        // from a task.
        for term in terms.iter() {
            let record = term.record().as_str().to_owned();
            let word = term.term().as_str().to_owned();
            sqlx::query!(
                r#"
                INSERT INTO exercise_correction_term (correction, source_record_id, term)
                VALUES (?, ?, ?)
                ON CONFLICT DO NOTHING
                "#,
                id,
                record,
                word
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))?;

        Ok(CorrectionId::from(id))
    }

    /// Retract one assertion. Says whether there was one to retract.
    ///
    /// The terms go with it, by the foreign key's cascade.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn retract(&self, id: CorrectionId) -> Result<bool, StoreError> {
        let stream = self.stream.to_string();
        let id = id.as_i64();
        let affected = sqlx::query!(
            "DELETE FROM exercise_correction WHERE id = ? AND stream = ?",
            id,
            stream
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        .rows_affected();

        Ok(affected > 0)
    }
}

/// What reading the overlay can go wrong with.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    UnknownExercise(#[from] UnknownCorrectedExercise),
}

fn corrected_term(record: &str, term: &str) -> Result<CorrectedTerm, OverlayError> {
    // Both were validated on the way in and neither column can be empty in a
    // row this wrote. A row that is empty anyway is a store we cannot read,
    // which is what `StoreError` says.
    let record = SourceRecordId::try_from(record).map_err(|error| {
        OverlayError::Store(StoreError::Corrupt {
            detail: error.to_string(),
        })
    })?;
    let term = SourceTerm::try_from(term).map_err(|error| {
        OverlayError::Store(StoreError::Corrupt {
            detail: error.to_string(),
        })
    })?;
    Ok(CorrectedTerm::new(record, term))
}

/// One correction's rows as they arrive, before the last of its terms is in.
///
/// Named rather than a tuple: five fields in positional order is exactly the
/// shape a later edit silently transposes.
struct Gathering {
    id: i64,
    exercise: String,
    asserted_at: String,
    reason: String,
    terms: Vec<CorrectedTerm>,
}

impl Gathering {
    fn built(self) -> Result<Correction, OverlayError> {
        let Self {
            id,
            exercise: key,
            asserted_at: at,
            reason,
            terms,
        } = self;

        let exercise = Exercise::named(&key).ok_or_else(|| UnknownCorrectedExercise {
            id,
            key: key.clone(),
        })?;
        let asserted_at: Timestamp = at.parse().map_err(|_| {
            OverlayError::Store(StoreError::Corrupt {
                detail: format!("correction {id} was asserted at {at}, which is not an instant"),
            })
        })?;
        let reason = CorrectionReason::try_from(reason).map_err(|error| {
            OverlayError::Store(StoreError::Corrupt {
                detail: error.to_string(),
            })
        })?;
        // Non-empty by construction: a correction only exists here because the
        // join found at least one term for it.
        let terms = NonEmpty::new(terms).map_err(|_| {
            OverlayError::Store(StoreError::Corrupt {
                detail: format!("correction {id} holds no terms"),
            })
        })?;

        Ok(Correction::new(
            CorrectionId::from(id),
            exercise,
            asserted_at,
            reason,
            terms,
        ))
    }
}
