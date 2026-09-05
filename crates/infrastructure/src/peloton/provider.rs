//! Reading a published programme out of Peloton, as a shape the planner can ask.
//!
//! **The skeleton is given and the content is fetched** (decision 0033). What
//! comes back is a [`Programme`] of zone profiles — no class ids, no titles,
//! nothing of Peloton's — because a provider answers with a shape and the domain
//! holds no vendor's identifiers (§ II.3).
//!
//! **It fails loudly.** Every class is fetched; one that will not read stops the
//! whole programme rather than yielding a shape with a hole in it, because a
//! mesocycle scored from fourteen of sixteen classes looks exactly like one
//! scored from all of them.

use std::collections::BTreeMap;

use application::SourceError;
use domain::cycling::{Programme, ZoneProfile};

use super::{class::PelotonClasses, skeleton::Placement};

/// Fetch every class a skeleton places, and return what the programme trains.
///
/// # Errors
///
/// [`SourceError`] from the first class that will not fetch or will not read.
pub async fn programme(
    classes: &PelotonClasses,
    placements: &[Placement],
) -> Result<Programme, SourceError> {
    let mut cells: BTreeMap<(u32, u32), Vec<_>> = BTreeMap::new();
    for placement in placements {
        let class = classes.class(placement.class_id).await?;
        // **A session is one or more classes** (0033): the FTP warm-up and test
        // pair share a cell, so rides accumulate rather than replace.
        cells
            .entry((
                u32::from(placement.microcycle),
                u32::from(placement.session),
            ))
            .or_default()
            .extend(class.ride);
    }
    Ok(Programme::new(
        cells
            .into_iter()
            .map(|(at, rides)| (at, ZoneProfile::of(rides.iter()))),
    ))
}
