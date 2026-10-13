//! `fitness canonicalise` — the canonical layer, built from the normalised one
//! (#247).
//!
//! **A plumbing command, so it takes what it derives and not a stream.** The
//! canonical layer is cross-source by definition — one entry per visit,
//! whatever number of sources recorded it — so there is no stream to name here
//! where `extract` and `normalise` both have one. What it takes instead is the
//! entity: `gym` or `cycling`.
//!
//! It contacts nothing and holds no run of its own. The inputs are already in
//! our terms, so there is nothing to refuse and nothing to resume.

use std::path::Path;

use application::canonicalise;
use configurator::store::Store;
use domain::schedule::Discipline;

use crate::{Failure, output};

/// Rebuild the canonical gym layer and report what it now holds.
pub async fn gym(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database).await?;
    let normalised = store.normalised_gym();
    let canonical = store.canonical_gym();

    output::canonicalising_started(Discipline::Gym);
    let done = canonicalise::gym_sessions(&normalised, &canonical).await?;
    output::canonicalised(done.read, done.written);
    Ok(())
}

/// Rebuild the canonical cycling layer and report what it now holds.
pub async fn cycling(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database).await?;
    let normalised = store.normalised_cycling();
    let canonical = store.canonical_cycling();

    output::canonicalising_started(Discipline::Cycling);
    let done = canonicalise::cycling_sessions(&normalised, &canonical).await?;
    output::canonicalised(done.read, done.written);
    Ok(())
}
