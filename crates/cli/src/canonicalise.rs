//! `fitness canonicalise` — the canonical layer, built from the normalised one
//! (#247).
//!
//! **A plumbing command, so it takes what it derives and not a stream.** The
//! canonical layer is cross-source by definition — one entry per visit,
//! whatever number of sources recorded it — so there is no stream to name here
//! where `extract` and `normalise` both have one. What it takes instead is the
//! entity: `gym` or `cycling`.
//!
//! It contacts nothing, and there is nothing to refuse or resume: the inputs are
//! already in our terms. Each rebuild is a `step` span, as `next`'s are (#633).

use std::path::Path;

use application::{Rederivation, next::Canonicalising};
use configurator::store::Store;
use domain::schedule::Discipline;

use crate::{Failure, output};

/// Rebuild the canonical gym layer and report what it now holds.
pub async fn gym(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database).await?;
    output::canonicalising_started(Discipline::Gym);
    let done = store.canonicalising().gym(Rederivation::Whole).await?;
    output::canonicalised(done.read, done.written);
    Ok(())
}

/// Rebuild the canonical weigh-ins and report what they now hold, naming every
/// weighing left out (#409).
pub async fn body_weight(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database).await?;
    output::canonicalising_weigh_ins_started();
    let done = store
        .canonicalising()
        .weigh_ins(Rederivation::Whole)
        .await?;
    output::weigh_ins_canonicalised(&done);
    Ok(())
}

/// Rebuild the canonical cycling layer and report what it now holds.
pub async fn cycling(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database).await?;
    output::canonicalising_started(Discipline::Cycling);
    let done = store.canonicalising().cycling(Rederivation::Whole).await?;
    output::canonicalised(done.read, done.written);
    Ok(())
}
