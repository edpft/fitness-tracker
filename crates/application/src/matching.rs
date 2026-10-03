//! Building the canonical layer for gym sessions from the normalised one
//! (#247).
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

use domain::{canonical::SessionCount, gym::canonical_sessions};

use crate::{
    error::StoreError,
    ports::{CanonicalGymSessionStore, NormalisedGymSessionReader},
};

/// What one run of the matching did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Matched {
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
) -> Result<Matched, StoreError> {
    let accounts = normalised.all().await?;
    let read = SessionCount::from(accounts.len());
    let sessions = canonical_sessions(accounts);
    let written = canonical.replace(sessions).await?;
    Ok(Matched { read, written })
}
