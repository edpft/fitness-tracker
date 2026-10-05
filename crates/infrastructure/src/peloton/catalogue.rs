//! Keeping the local class catalogue current (#246).
//!
//! The operator, 2026-09-26: *"what I actually want is to store all power zone
//! rides and fetch, on a schedule, any new ones, so that we always have a local
//! catalogue to pick from."*
//!
//! **No command of its own.** He ruled that out on 2026-10-05 — *"I don't want
//! a separate command to do this, it should happen as a side effect of `fitness
//! next`"* — so this is called where cycling's record is already collected, with
//! the Peloton credential already resolved and § 36 already handling a source
//! that cannot be reached.
//!
//! **A whole walk and a bounded read.** The two halves cost very different
//! amounts, so they are paced differently:
//!
//! - The *listing* walk runs to exhaustion every refresh. It is a page per
//!   hundred classes, which is what makes "every power zone class is in the
//!   store" true and what notices the ones published since the last run.
//! - The *detail* reads are one request per class, and the whole library's
//!   worth of them is not something to put inside a daily loop. So each refresh
//!   reads at most [`READS_PER_REFRESH`] of the classes it has not read yet,
//!   newest first, and the catalogue converges over a handful of runs.
//!
//! **The skeletons' classes are read whether or not the walk lists them.**
//! `mapping` already records that one class of *Peak Your Power Zones* is
//! unavailable to the operator's account, and a catalogue built from the browse
//! listing alone could therefore leave `plan` with a hole in Peak — a mesocycle
//! scored from fourteen of sixteen classes looks exactly like one scored from
//! all of them, which is the thing `provider` exists to refuse. They are also
//! read first, ahead of the newest, because they are the classes something
//! actually depends on today.

use application::SourceError;

use super::{class, class::PelotonClasses, skeleton};
use crate::store::{Held, SqlitePelotonClassStore};

/// How many class details one refresh will fetch.
///
/// **One fixed decision, not a knob** (§ 14.1): it is a fact about how long the
/// operator's daily loop may take, not a parameter of his training. At roughly
/// a third of a second each, fifty is some fifteen seconds added to a run that
/// already contacts four sources, and a library of a thousand classes is read
/// inside a month of ordinary use — sooner, because the classes that matter are
/// the newest and they are read first.
const READS_PER_REFRESH: u32 = 50;

/// How many pages the listing walk will take before giving up.
///
/// A walk ends when the source says there is no more. This is the guard against
/// a source that never says so, and it is sized to be unreachable: at a hundred
/// classes a page it allows ten thousand power zone classes, so hitting it means
/// the endpoint has stopped saying where the end is rather than that the library
/// is large. It also bounds what one `fitness next` can spend on the walk.
const PAGE_LIMIT: u32 = 100;

/// What one refresh did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Refreshed {
    /// Classes the listing walk saw this time.
    pub listed: u64,
    /// Of those, the ones the catalogue had never held.
    pub added: u64,
    /// Classes whose detail was read this time.
    pub read: u64,
    /// What the catalogue holds now.
    pub held: Held,
}

/// Walk Peloton's power zone classes into the store, then read what fits.
///
/// # Errors
///
/// [`SourceError`] if Peloton is unreachable or refuses the credential, and
/// [`SourceError::Malformed`] if the store will not accept what it served —
/// which the caller reports and steps past (§ 36), because a catalogue that is
/// one run out of date still answers.
pub async fn refresh(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
) -> Result<Refreshed, SourceError> {
    let unavailable = |error: application::StoreError| SourceError::Malformed {
        detail: format!("the class catalogue could not be written: {error}"),
    };

    let mut listed = 0_u64;
    let mut added = 0_u64;
    for page in 0..PAGE_LIMIT {
        let found = classes.power_zone_page(page).await?;
        listed += u64::try_from(found.classes.len()).unwrap_or_default();
        added += store
            .record_listed(&found.classes)
            .await
            .map_err(unavailable)?;
        if !found.more {
            break;
        }
    }

    let read = read_details(classes, store).await.map_err(unavailable)?;
    let held = store.held().await.map_err(unavailable)?;
    Ok(Refreshed {
        listed,
        added,
        read,
        held,
    })
}

/// Read up to [`READS_PER_REFRESH`] class details, the depended-on first.
async fn read_details(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
) -> Result<u64, application::StoreError> {
    // **Attempts are what the budget counts, not successes.** A class that
    // will not fetch has still cost a request, and budgeting on successes alone
    // would let a run of failures spend the whole allowance twice over.
    let mut attempted = 0_u32;
    let mut read = 0_u64;

    // The placed classes first. Whether each is already read is asked of the
    // store rather than remembered, so a run after the first fetches none of
    // them.
    for reference in placed() {
        if attempted >= READS_PER_REFRESH {
            return Ok(read);
        }
        if store.is_read(&reference).await? {
            continue;
        }
        attempted += 1;
        if store_detail(classes, store, &reference).await? {
            read += 1;
        }
    }

    let budget = READS_PER_REFRESH.saturating_sub(attempted);
    if budget == 0 {
        return Ok(read);
    }
    for reference in store.unread(budget).await? {
        if store_detail(classes, store, &reference).await? {
            read += 1;
        }
    }
    Ok(read)
}

/// Read one class's detail into the catalogue.
///
/// **A class that will not fetch is stepped past rather than fatal.** The
/// library holds classes the operator's account cannot start, and one of them
/// refusing is a fact about the catalogue; stopping the whole refresh on it
/// would mean the first such class froze the catalogue permanently, one short.
async fn store_detail(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
    reference: &str,
) -> Result<bool, application::StoreError> {
    let Ok(body) = classes.detail(reference).await else {
        return Ok(false);
    };
    // **Read before it is stored, for the title and the length.** A class the
    // browse listing never carried has no row yet, and those two are all the
    // store needs to make one — see `record_detail`. A body that will not read
    // is not stored at all: it would be a row that claims to be read and
    // answers nothing.
    let Ok(class) = class::derive(reference, &body) else {
        return Ok(false);
    };
    let duration = class.warm_up_seconds + class.ride_seconds + class.cool_down_seconds;
    store
        .record_detail(reference, &class.title, duration, &body)
        .await?;
    Ok(true)
}

/// Every class id a published programme's skeleton places.
fn placed() -> Vec<String> {
    let peak = skeleton::peak_your_power_zones();
    let mut found: Vec<String> = skeleton::SKELETONS
        .iter()
        .flat_map(|skeleton| skeleton.placements().iter())
        .chain(peak.iter())
        .map(|placement| placement.class_id.to_owned())
        .collect();
    found.sort_unstable();
    found.dedup();
    found
}

/// Peloton's class library, read locally.
///
/// **The catalogue answers, and the source only where the catalogue has not
/// caught up yet.** Which way round that is matters: choosing among candidates
/// is what #246 wanted off the network, and it is answered from the store
/// alone, because the listing walk fills every row on the first refresh. What
/// may still fall through to Peloton is reading one *named* class whose detail
/// the bounded reads have not reached — a single request for a class already
/// chosen, not sixty-five to choose among.
///
/// So this gets quieter as the catalogue converges, and on a store whose
/// classes have all been read it contacts Peloton not at all.
#[derive(Debug, Clone, Copy)]
pub struct ClassCatalogue<'a> {
    stored: &'a SqlitePelotonClassStore,
    source: Option<&'a PelotonClasses>,
}

impl<'a> ClassCatalogue<'a> {
    /// A catalogue that may fall through to Peloton for a class it has not read.
    pub const fn new(stored: &'a SqlitePelotonClassStore, source: &'a PelotonClasses) -> Self {
        Self {
            stored,
            source: Some(source),
        }
    }

    /// A catalogue over whatever credential there is.
    ///
    /// **The constructor the driving adapters want**, because "is there a
    /// Peloton login" is the shape of their answer: `plan` and `committing`
    /// both resolve one and carry on without it, and both were branching on the
    /// `Option` to pick between [`new`](Self::new) and [`stored`](Self::stored).
    /// That branch belongs here, once, rather than at every call site.
    pub const fn over(
        stored: &'a SqlitePelotonClassStore,
        source: Option<&'a PelotonClasses>,
    ) -> Self {
        Self { stored, source }
    }

    /// A catalogue with nowhere to fall through to.
    ///
    /// **What an absent credential now costs, and what it no longer does.** It
    /// costs reading a class the bounded refreshes have not reached yet, and it
    /// costs delivering. It no longer costs *choosing*: the listing is in the
    /// store, so which classes could fill a holding role is answerable with
    /// `PELOTON_EMAIL` unset, which is what #246 asked for.
    pub const fn stored(stored: &'a SqlitePelotonClassStore) -> Self {
        Self {
            stored,
            source: None,
        }
    }

    /// What one class prescribes.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if the catalogue holds something unreadable,
    /// and whatever [`PelotonClasses::class`] gives where the class has to be
    /// fetched.
    pub async fn class(&self, reference: &str) -> Result<super::ClassSession, SourceError> {
        match self.stored.class(reference).await {
            Ok(Some(found)) => return Ok(found),
            Ok(None) => {}
            Err(error) => {
                return Err(SourceError::Malformed {
                    detail: format!("the class catalogue could not be read: {error}"),
                });
            }
        }
        let Some(source) = self.source else {
            return Err(SourceError::Unavailable {
                detail: format!(
                    "the catalogue has not read class {reference} yet, and there is no \
                     credential to read it with. The next `fitness next` will read it"
                ),
            });
        };
        source.class(reference).await
    }

    /// Every class of one series and one length, newest first.
    ///
    /// **The store only.** This is the picking, and picking is the thing that
    /// was to stop needing Peloton. An empty answer on a catalogue that has
    /// never been refreshed is indistinguishable here from a series Peloton has
    /// no class of; [`Held::listed`] is what tells those apart, and the caller
    /// reports it.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if the catalogue cannot be read.
    pub async fn in_series(
        &self,
        series: &str,
        duration_seconds: u64,
    ) -> Result<Vec<super::ClassSummary>, SourceError> {
        self.stored
            .in_series(series, duration_seconds)
            .await
            .map_err(|error| SourceError::Malformed {
                detail: format!("the class catalogue could not be read: {error}"),
            })
    }

    /// What the catalogue holds, for a message that has to explain an empty
    /// answer.
    ///
    /// # Errors
    ///
    /// [`SourceError::Malformed`] if the catalogue cannot be read.
    pub async fn held(&self) -> Result<Held, SourceError> {
        self.stored
            .held()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: format!("the class catalogue could not be read: {error}"),
            })
    }

    /// The cool-down ride a session ends with.
    ///
    /// **Still the source's answer, and deliberately.** What the operator rides
    /// is the *most recent* five-minute cool-down by an instructor, resolved at
    /// the moment of delivery — `class::PelotonClasses::cool_down_for` says why
    /// a table of ids would be wrong as soon as Peloton published another. A
    /// cool-down is not a power zone class and is not in this catalogue.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unavailable`] where there is no credential, and whatever
    /// [`PelotonClasses::cool_down_after`] gives otherwise.
    pub async fn cool_down_after(
        &self,
        instructor: Option<&str>,
    ) -> Result<super::ClassSummary, SourceError> {
        let Some(source) = self.source else {
            return Err(SourceError::Unavailable {
                detail: "a cool-down ride is the most recent one Peloton serves, \
                         so finding it needs a credential"
                    .to_owned(),
            });
        };
        source.cool_down_after(instructor).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason this list exists: Peak places a class the account cannot
    /// start, so a catalogue built from the browse listing alone could miss it.
    #[test]
    fn every_placed_class_is_wanted_once() {
        let placed = placed();
        let mut sorted = placed.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(placed, sorted);
        assert!(
            placed.len() >= 40,
            "Boost places 24 and Build 16, so at least 40: {}",
            placed.len()
        );
    }

    #[test]
    fn an_unread_catalogue_has_everything_outstanding() {
        let held = Held { listed: 7, read: 0 };
        assert_eq!(held.outstanding(), 7);
    }
}
