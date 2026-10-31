//! Building the canonical layer from the normalised one: gym sessions (#247),
//! cycling sessions (#408) and weigh-ins (#409).
//!
//! § II.4's second derivation, and the shortest use case in the crate: read
//! every normalised gym session, match and merge them, replace the canonical
//! layer with the result. Or, for the gym once its layer stands, only the days
//! the normalised layer's changes reach (#610). It contacts nothing and refuses nothing — matching
//! reads entities already in our terms, so there is no translation to fail and
//! nothing to refuse (§ 9).
//!
//! **No refusals and no run log.** A refusal store is there because a source
//! can serve what the domain will not take, which cannot arise here: the inputs
//! are ours, and what is replaced is replaced in one transaction. When a rebuild
//! started and ended, and what it read and wrote, is informational, so it is a
//! `step` span in `telemetry.db` rather than a table in the store (#633).

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
    /// How many canonical sessions the layer holds once they are written:
    /// every one they made where the whole layer was rebuilt.
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

/// Rebuild the canonical gym sessions on the days the normalised layer's
/// changes reach, and leave the rest of the layer standing (#610).
///
/// **Over a layer that was derived whole**, which is the caller's to know: the
/// changes are recorded against one, and a layer never built has none.
/// `written` is how many sessions the layer then holds, as it is for a whole
/// rebuild.
///
/// # Errors
///
/// [`StoreError`] if either store is unavailable, or if the normalised layer
/// holds something unreadable.
pub async fn gym_sessions_reached<
    R: NormalisedGymSessionReader + Sync,
    C: CanonicalGymSessionStore + Sync,
>(
    normalised: &R,
    canonical: &C,
) -> Result<Canonicalised, StoreError> {
    let reach = canonical.reach().await?;
    if reach.is_empty() {
        return Ok(Canonicalised {
            read: SessionCount::from(0),
            written: canonical.count().await?,
        });
    }
    let accounts = normalised.on(reach.read()).await?;
    let read = SessionCount::from(accounts.len());
    let sessions = gym::canonical_sessions_within(accounts, &reach);
    let written = canonical.replace_within(&reach, sessions).await?;
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
