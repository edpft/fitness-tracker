//! The edit overlay, and the source-side queries that find what a correction
//! reaches.
//!
//! **What a correction stores is the source's own identity for each entry**
//! (§ II.2), so finding them is a question about one source's records: Hevy's
//! templates for an exercise, Garmin's sets for a day. This build answers both,
//! and the frontend names neither source.

use application::StoreError;
use domain::{
    gym::exercise::Exercise,
    landing::InvalidStream,
    normalised::{
        CorrectedTerm, Correction, CorrectionId, CorrectionReason, OperatorZone, SetFigures,
    },
    sequence::NonEmpty,
};
use infrastructure::{
    GarminExerciseSetLandingStore, GarminRecordedSets, HevyStandIns, HevyWorkoutLandingStore,
    OverlayError, SqliteEditOverlayStore,
};
use jiff::{Timestamp, civil::Date};

use crate::store::Store;

/// What one source's entries read as an exercise, as found for a correction.
pub use infrastructure::hevy::StandIn;

/// A set one source recorded with given figures, as found for a correction.
pub use infrastructure::garmin::FoundSet;

/// Why a correction could not be found, recorded or read back.
#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    #[error(transparent)]
    Stream(#[from] InvalidStream),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Overlay(#[from] OverlayError),
}

/// The streams a correction can be asserted against.
///
/// Not every stream the catalogue knows: `garmin.exercise_sets` is not a
/// stream the operator collects — it is fetched per activity behind
/// `garmin.activities` (#172) — and it is the one whose records a figures
/// correction anchors in. A stream nobody has corrected answers with nothing,
/// so reading all of them costs one query each and keeps the numbering one
/// space.
const CORRECTED: [&str; 2] = [
    HevyWorkoutLandingStore::STREAM,
    GarminExerciseSetLandingStore::STREAM,
];

/// Every entry that reads as an exercise within the days given.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be read.
pub async fn reading_as(
    store: &Store,
    exercise: Exercise,
    zone: &OperatorZone,
    from: Option<Date>,
    until: Option<Date>,
) -> Result<Vec<StandIn>, CorrectionError> {
    Ok(HevyStandIns::new(store.pool().clone())
        .reading_as(exercise, zone, from, until)
        .await?)
}

/// Assert that the entries found read as another exercise.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be written.
pub async fn assert_exercise(
    store: &Store,
    actually: Exercise,
    reason: &CorrectionReason,
    terms: &NonEmpty<CorrectedTerm>,
) -> Result<CorrectionId, CorrectionError> {
    Ok(
        SqliteEditOverlayStore::new(store.pool().clone(), HevyWorkoutLandingStore::STREAM)?
            .assert(actually, Timestamp::now(), reason, terms)
            .await?,
    )
}

/// Every set recorded with the given figures on a day.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be read.
pub async fn recording(
    store: &Store,
    recorded: SetFigures,
    zone: &OperatorZone,
    on: Date,
) -> Result<Vec<FoundSet>, CorrectionError> {
    let mut found = GarminRecordedSets::new(store.pool().clone())
        .recording(recorded, zone, on)
        .await?;
    found.extend(
        infrastructure::hevy::HevyStandIns::new(store.pool().clone())
            .recording(recorded, zone, on)
            .await?,
    );
    Ok(found)
}

/// Assert that the sets found were performed with other figures: one
/// assertion per source they were found in, since each source's overlay is
/// its own.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be written.
pub async fn assert_figures(
    store: &Store,
    recorded: SetFigures,
    actually: SetFigures,
    reason: &CorrectionReason,
    found: &[FoundSet],
) -> Result<Vec<CorrectionId>, CorrectionError> {
    let at = Timestamp::now();
    let mut ids = Vec::new();
    for stream in CORRECTED {
        let terms: Vec<CorrectedTerm> = found
            .iter()
            .filter(|set| set.stream == stream)
            .map(|set| set.set.clone())
            .collect();
        let Ok(sets) = NonEmpty::new(terms) else {
            continue;
        };
        ids.push(
            SqliteEditOverlayStore::new(store.pool().clone(), stream)?
                .assert_figures(recorded, actually, at, reason, &sets)
                .await?,
        );
    }
    Ok(ids)
}

/// Every correction in force, whichever source it was asserted against, in
/// the order they were asserted.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be read or holds a key the
/// vocabulary no longer has.
pub async fn all(store: &Store) -> Result<Vec<Correction>, CorrectionError> {
    let mut corrections = Vec::new();
    for stream in CORRECTED {
        corrections.extend(
            SqliteEditOverlayStore::new(store.pool().clone(), stream)?
                .all()
                .await?,
        );
    }
    corrections.sort_by_key(|correction| (correction.asserted_at(), correction.id()));
    Ok(corrections)
}

/// Retract one assertion, by its number. `false` if there is none.
///
/// **One numbering across every stream**, so the number `show` printed is the
/// one that is retracted whichever source it was asserted against.
///
/// # Errors
///
/// [`CorrectionError`] if the store cannot be written.
pub async fn retract(store: &Store, id: CorrectionId) -> Result<bool, CorrectionError> {
    for stream in CORRECTED {
        if SqliteEditOverlayStore::new(store.pool().clone(), stream)?
            .retract(id)
            .await?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
