//! Building the canonical layer from the normalised one: gym sessions (#247),
//! cycling sessions (#408) and weigh-ins (#409).
//!
//! § II.4's second derivation, and the shortest use case in the crate: read
//! every normalised gym session, match and merge them, replace the canonical
//! layer with the result. It contacts nothing and refuses nothing — matching
//! reads entities already in our terms, so there is no translation to fail and
//! nothing to refuse (§ 9).
//!
//! **No run log, and no refusals.** Every derivation from raw carries a run
//! because § 38 wants a broken one visible rather than merely absent, and a
//! refusal store because a source can serve what the domain will not take.
//! Neither arises here: the inputs are ours, the whole layer is replaced in one
//! transaction, and a count of what was written is the whole of what there is
//! to report.

use domain::{
    body::{self, LeftOut, WeighInCount},
    canonical::SessionCount,
    cycling, gym,
};

use crate::{
    error::StoreError,
    ports::{
        CanonicalCyclingSessionStore, CanonicalGymSessionStore, CanonicalWeighInStore,
        NormalisedCyclingSessionReader, NormalisedGymSessionReader, NormalisedWeighInReader,
    },
};

/// What one run of the canonicalising did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canonicalised {
    /// How many normalised sessions were read.
    pub read: SessionCount,
    /// How many canonical sessions they made.
    ///
    /// **Fewer than were read, and not only because sessions merge.** A visit
    /// no account holds an exercise for is no canonical session — the
    /// operator, 2026-10-01: *"Heart rate only isn't a meaningful gym
    /// session."* — so a watch recording alone writes nothing.
    pub written: SessionCount,
}

/// Replace the canonical gym layer with what the normalised layer now gives.
///
/// # Errors
///
/// [`StoreError`] if either store is unavailable, or if the normalised layer
/// holds something unreadable.
pub async fn gym_sessions<
    R: NormalisedGymSessionReader + Sync,
    C: CanonicalGymSessionStore + Sync,
>(
    normalised: &R,
    canonical: &C,
) -> Result<Canonicalised, StoreError> {
    let accounts = normalised.all().await?;
    let read = SessionCount::from(accounts.len());
    let sessions = gym::canonical_sessions(accounts);
    let written = canonical.replace(sessions).await?;
    Ok(Canonicalised { read, written })
}

/// Replace the canonical cycling layer with what the normalised layer now
/// gives.
///
/// # Errors
///
/// [`StoreError`] if either store is unavailable, or if the normalised layer
/// holds something unreadable.
pub async fn cycling_sessions<
    R: NormalisedCyclingSessionReader + Sync,
    C: CanonicalCyclingSessionStore + Sync,
>(
    normalised: &R,
    canonical: &C,
) -> Result<Canonicalised, StoreError> {
    let accounts = normalised.all().await?;
    let read = SessionCount::from(accounts.len());
    let sessions = cycling::canonical_sessions(accounts);
    let written = canonical.replace(sessions).await?;
    Ok(Canonicalised { read, written })
}

/// What one run of canonicalising the weigh-ins did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeighInsCanonicalised {
    /// How many normalised weigh-ins were read.
    pub read: WeighInCount,
    /// How many weighings they made. Fewer than were read wherever one day is
    /// in several copies.
    pub written: WeighInCount,
    /// Every weighing the merge could not settle, named (§ 10).
    pub left_out: Vec<LeftOut>,
}

/// Replace the canonical weigh-in layer with what the normalised layer now
/// gives.
///
/// # Errors
///
/// [`StoreError`] if either store is unavailable, or if the normalised layer
/// holds something unreadable.
pub async fn weigh_ins<R: NormalisedWeighInReader + Sync, C: CanonicalWeighInStore + Sync>(
    normalised: &R,
    canonical: &C,
) -> Result<WeighInsCanonicalised, StoreError> {
    let accounts = normalised.all().await?;
    let read = WeighInCount::from(accounts.len());
    let merged = body::canonical_weigh_ins(accounts);
    let written = canonical.replace(merged.weighed).await?;
    Ok(WeighInsCanonicalised {
        read,
        written,
        left_out: merged.left_out,
    })
}
