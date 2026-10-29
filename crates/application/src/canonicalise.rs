//! Building the canonical layer from the normalised one: gym sessions (#247),
//! cycling sessions (#408) and weigh-ins (#409).
//!
//! § II.4's second derivation, and the shortest use case in the crate: read
//! every normalised gym session, match and merge them, replace the canonical
//! layer with the result. It contacts nothing and refuses nothing — matching
//! reads entities already in our terms, so there is no translation to fail and
//! nothing to refuse (§ 9).
//!
//! **A run log, and no refusals** (#609). A refusal store is there because a
//! source can serve what the domain will not take, which cannot arise here:
//! the inputs are ours, and the whole layer is replaced in one transaction. A
//! run is still logged, with when it started and finished, because a rebuild's
//! timing was otherwise printed and lost.

use domain::{
    body::{self, LeftOut, WeighInCount},
    canonical::SessionCount,
    cycling, gym,
};

use crate::{
    error::StoreError,
    ports::{
        CanonicalCyclingSessionStore, CanonicalGymSessionStore, CanonicalLayer,
        CanonicalWeighInStore, CanonicalisationOutcome, CanonicalisationRunLog, Clock,
        NormalisedCyclingSessionReader, NormalisedGymSessionReader, NormalisedWeighInReader,
    },
};

/// Where each rebuild is logged, and the clock it is timed by.
pub struct Logged<'a, L, K> {
    pub runs: &'a L,
    pub clock: &'a K,
}

impl<L: CanonicalisationRunLog + Sync, K: Clock + Sync> Logged<'_, L, K> {
    /// Rebuild one layer, logging when it started and how it ended.
    ///
    /// **A rebuild that failed is logged as failed**, and the failure handed
    /// back is the rebuild's: if logging it fails too, the store being
    /// unreachable is why we are here.
    async fn run<T>(
        &self,
        layer: CanonicalLayer,
        rebuild: impl Future<Output = Result<T, StoreError>>,
        counts: impl Fn(&T) -> (usize, usize),
    ) -> Result<T, StoreError> {
        let run = self.runs.begin(layer, self.clock.now()).await?;
        match rebuild.await {
            Ok(done) => {
                let (read, written) = counts(&done);
                let outcome = CanonicalisationOutcome::Succeeded {
                    finished_at: self.clock.now(),
                    read,
                    written,
                };
                self.runs.finish(run, outcome).await?;
                Ok(done)
            }
            Err(error) => {
                let outcome = CanonicalisationOutcome::Failed {
                    finished_at: self.clock.now(),
                };
                let _ = self.runs.finish(run, outcome).await;
                Err(error)
            }
        }
    }
}

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

impl Canonicalised {
    const fn counts(&self) -> (usize, usize) {
        (self.read.as_usize(), self.written.as_usize())
    }
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
    L: CanonicalisationRunLog + Sync,
    K: Clock + Sync,
>(
    normalised: &R,
    canonical: &C,
    logged: &Logged<'_, L, K>,
) -> Result<Canonicalised, StoreError> {
    let rebuild = async {
        let accounts = normalised.all().await?;
        let read = SessionCount::from(accounts.len());
        let sessions = gym::canonical_sessions(accounts);
        let written = canonical.replace(sessions).await?;
        Ok(Canonicalised { read, written })
    };
    logged
        .run(CanonicalLayer::Gym, rebuild, Canonicalised::counts)
        .await
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
    L: CanonicalisationRunLog + Sync,
    K: Clock + Sync,
>(
    normalised: &R,
    canonical: &C,
    logged: &Logged<'_, L, K>,
) -> Result<Canonicalised, StoreError> {
    let rebuild = async {
        let accounts = normalised.all().await?;
        let read = SessionCount::from(accounts.len());
        let sessions = cycling::canonical_sessions(accounts);
        let written = canonical.replace(sessions).await?;
        Ok(Canonicalised { read, written })
    };
    logged
        .run(CanonicalLayer::Cycling, rebuild, Canonicalised::counts)
        .await
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
pub async fn weigh_ins<
    R: NormalisedWeighInReader + Sync,
    C: CanonicalWeighInStore + Sync,
    L: CanonicalisationRunLog + Sync,
    K: Clock + Sync,
>(
    normalised: &R,
    canonical: &C,
    logged: &Logged<'_, L, K>,
) -> Result<WeighInsCanonicalised, StoreError> {
    let rebuild = async {
        let accounts = normalised.all().await?;
        let read = WeighInCount::from(accounts.len());
        let merged = body::canonical_weigh_ins(accounts);
        let written = canonical.replace(merged.weighed).await?;
        Ok(WeighInsCanonicalised {
            read,
            written,
            left_out: merged.left_out,
        })
    };
    logged
        .run(CanonicalLayer::WeighIns, rebuild, |done| {
            (done.read.as_usize(), done.written.as_usize())
        })
        .await
}
