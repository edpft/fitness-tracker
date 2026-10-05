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
//! **A whole walk and then a whole read, both paced.** The two halves cost
//! different amounts, but neither is bounded by a count:
//!
//! - The *listing* walk is a page per hundred classes. It runs to exhaustion
//!   every refresh, which is what makes "every power zone class is in the
//!   store" true and what notices the ones published since the last run.
//! - The *detail* reads are one request per class, and every class that has not
//!   had one gets one, [`BETWEEN_REQUESTS`] apart.
//!
//! **It was fifty a run until #369, and that was wrong.** The operator, reading
//! the first refresh's output on 2026-10-05: *"What made you think I'd only
//! want to read 50 of the available classes?"* Fifty was sized to the latency
//! of his daily loop, which is the wrong thing to size it to: the whole library
//! is a one-off of about a thousand requests, and after it the steady state is
//! the handful of classes Peloton published that week. Three weeks of the tool
//! not being what it said it was bought nothing.
//!
//! **Because the first one is long, it is announced.** The walk and the read
//! are therefore two calls rather than one — the caller is the thing that can
//! print, and it prints [`Walked::reading_takes`] before the reading starts.
//!
//! **The skeletons' classes are read whether or not the walk lists them.**
//! `mapping` already records that one class of *Peak Your Power Zones* is
//! unavailable to the operator's account, and a catalogue built from the browse
//! listing alone could therefore leave `plan` with a hole in Peak — a mesocycle
//! scored from fourteen of sixteen classes looks exactly like one scored from
//! all of them, which is the thing `provider` exists to refuse. They are also
//! read first, ahead of the newest, because they are the classes something
//! actually depends on today.

use std::{collections::HashSet, time::Duration};

use application::SourceError;

use super::{class, class::PelotonClasses, skeleton};
use crate::store::{Held, SqlitePelotonClassStore};

/// How long to wait between one class's detail and the next.
///
/// **Politeness to the source, and the only thing bounding the read** (#369).
/// Peloton documents no rate limit, so this is the figure the Garmin adapters
/// already settled on for the same reason — `garmin::files` downloads two
/// thousand activity files a quarter-second apart.
///
/// **One fixed decision, not a knob** (§ 14.1): it is a fact about how hard a
/// source may be asked, not a parameter of the operator's training.
const BETWEEN_REQUESTS: Duration = Duration::from_millis(250);

/// What one detail read costs in wall-clock time, for the estimate alone.
///
/// [`BETWEEN_REQUESTS`] plus a third of a second for the request itself. **That
/// third of a second is unverified**: it is the figure this module asserted
/// when the fifty-a-run budget was sized against it, and nothing here has
/// timed the endpoint. It is good enough for its only purpose, which is to say
/// "about ten minutes" rather than appear to hang — and if it is out by half,
/// the announcement is out by half and the reading still finishes.
const PER_READ: Duration = Duration::from_millis(583);

/// How many pages the listing walk will take before giving up.
///
/// A walk ends when the source says there is no more. This is the guard against
/// a source that never says so, and it is sized to be unreachable: at a hundred
/// classes a page it allows ten thousand power zone classes, so hitting it means
/// the endpoint has stopped saying where the end is rather than that the library
/// is large. It also bounds what one `fitness next` can spend on the walk.
const PAGE_LIMIT: u32 = 100;

/// What the listing walk found, and what is left to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Walked {
    /// Classes the listing walk saw this time.
    pub listed: u64,
    /// Of those, the ones the catalogue had never held.
    pub added: u64,
    /// What the catalogue holds now the walk has been recorded.
    pub held: Held,
}

impl Walked {
    /// Roughly how long reading the outstanding details will take.
    ///
    /// **Printed before the reading starts**, because the first refresh against
    /// an empty catalogue is a thousand requests and ten minutes of silence is
    /// indistinguishable from a hang. Zero where there is nothing to read,
    /// which is the converged case and so the usual one.
    #[must_use]
    pub fn reading_takes(&self) -> Duration {
        PER_READ.saturating_mul(u32::try_from(self.held.outstanding()).unwrap_or(u32::MAX))
    }
}

/// What the detail reads did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    /// Classes whose detail was read this time.
    pub read: u64,
    /// Classes Peloton would not serve, recorded so they are not asked again.
    pub not_served: u64,
    /// Why the reading stopped early, where it did.
    ///
    /// **A partial read is reported, not thrown away.** Six hundred classes
    /// read and then an outage is six hundred classes the next refresh does not
    /// have to fetch, so the count stands and this says why there are more.
    pub stopped: Option<SourceError>,
    /// What the catalogue holds now.
    pub held: Held,
}

/// Walk Peloton's power zone classes into the store.
///
/// The cheap half, and the half that notices a newly published class. Call
/// [`read_details`] after it.
///
/// # Errors
///
/// [`SourceError`] if Peloton is unreachable or refuses the credential, and
/// [`SourceError::Malformed`] if the store will not accept what it served —
/// which the caller reports and steps past (§ 36), because a catalogue that is
/// one run out of date still answers.
pub async fn walk(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
) -> Result<Walked, SourceError> {
    let mut listed = 0_u64;
    let mut added = 0_u64;
    for page in 0..PAGE_LIMIT {
        let found = classes.power_zone_page(page).await?;
        listed += u64::try_from(found.classes.len()).unwrap_or_default();
        added += store
            .record_listed(&found.classes)
            .await
            .map_err(|error| unwritable(&error))?;
        if !found.more {
            break;
        }
    }
    let held = store.held().await.map_err(|error| unwritable(&error))?;
    Ok(Walked {
        listed,
        added,
        held,
    })
}

/// Read every class detail the catalogue does not hold, the depended-on first.
///
/// **No count bounds this** (#369): [`BETWEEN_REQUESTS`] does, and the whole
/// library is a one-off. What ends it early is the source stopping, which
/// comes back in [`Reading::stopped`] rather than as an error, because what was
/// read before it stopped is kept.
///
/// # Errors
///
/// [`SourceError::Malformed`] if the store will not accept what Peloton served.
pub async fn read_details(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
) -> Result<Reading, SourceError> {
    let mut outcome = Reading::default();

    // The placed classes first. Whether each is already accounted for is asked
    // of the store rather than remembered, so a run after the first fetches
    // none of them.
    let mut references = Vec::new();
    for reference in placed() {
        if !store
            .is_accounted_for(&reference)
            .await
            .map_err(|error| unwritable(&error))?
        {
            references.push(reference);
        }
    }
    // A placed class the walk also listed is in both lists, and asking twice
    // would cost a request and record the same row over itself.
    let already: HashSet<&str> = references.iter().map(String::as_str).collect();
    let listed: Vec<String> = store
        .unread()
        .await
        .map_err(|error| unwritable(&error))?
        .into_iter()
        .filter(|reference| !already.contains(reference.as_str()))
        .collect();
    references.extend(listed);

    for (asked, reference) in references.into_iter().enumerate() {
        if asked > 0 {
            tokio::time::sleep(BETWEEN_REQUESTS).await;
        }
        match store_detail(classes, store, &reference).await {
            Ok(Asked::Read) => outcome.read += 1,
            Ok(Asked::NotServed) => outcome.not_served += 1,
            Err(Failed::Store(error)) => return Err(unwritable(&error)),
            Err(Failed::Source(error)) => {
                outcome.stopped = Some(error);
                break;
            }
        }
    }

    outcome.held = store.held().await.map_err(|error| unwritable(&error))?;
    Ok(outcome)
}

/// A store that will not take what the source served.
fn unwritable(error: &application::StoreError) -> SourceError {
    SourceError::Malformed {
        detail: format!("the class catalogue could not be written: {error}"),
    }
}

/// What asking Peloton for one class produced.
enum Asked {
    Read,
    NotServed,
}

/// Why asking Peloton for one class produced nothing.
enum Failed {
    /// The source refused the credential or stopped answering. The reading
    /// stops: a thousand requests against a source that is failing is not
    /// politeness, and the next run resumes where this one left off.
    Source(SourceError),
    Store(application::StoreError),
}

/// Read one class's detail into the catalogue.
///
/// **A class Peloton will not serve is recorded as such, not stepped past.**
/// The library holds classes the operator's account cannot start — `mapping`
/// names one — and leaving it unread was harmless only while the reads were
/// bounded at fifty a run. Unbounded, it would be asked for on every `fitness
/// next` for ever and the catalogue could never say it had read everything.
async fn store_detail(
    classes: &PelotonClasses,
    store: &SqlitePelotonClassStore,
    reference: &str,
) -> Result<Asked, Failed> {
    let body = match classes.detail_if_served(reference).await {
        Ok(Some(body)) => body,
        Ok(None) => {
            store
                .record_not_served(reference)
                .await
                .map_err(Failed::Store)?;
            return Ok(Asked::NotServed);
        }
        Err(error) => return Err(Failed::Source(error)),
    };
    // **A body the reader cannot make a session of is still stored.** That is
    // the whole argument for keeping the response: a corrected reader costs a
    // re-read of the store rather than a re-fetch of the library, and this
    // module's own history is the evidence, since the transcription it replaced
    // had a cool-down five minutes out. Nor is it a class Peloton would not
    // serve: it served this one, so there is nothing left to ask it.
    //
    // The title and the length are only wanted for a class the browse listing
    // never carried, which has no row yet; see `record_detail`.
    let (title, duration) = class::named(&body).unwrap_or_else(|| (reference.to_owned(), 1));
    store
        .record_detail(reference, &title, duration, &body)
        .await
        .map_err(Failed::Store)?;
    Ok(Asked::Read)
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
/// the catalogue does not hold — a single request for a class already chosen,
/// not sixty-five to choose among.
///
/// **Since #369 that fall-through is for a class published since the last
/// refresh and nothing else**, because one refresh now reads every detail it
/// does not hold rather than fifty of them. On a converged store this contacts
/// Peloton not at all.
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
    /// costs reading a class published since the last refresh, and it costs
    /// delivering. It no longer costs *choosing*: the listing is in the store,
    /// so which classes could fill a holding role is answerable with
    /// `PELOTON_EMAIL` unset, which is what #246 asked for — nor, since #369,
    /// reading any class the catalogue has listed, because a refresh reads them
    /// all.
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
        let held = Held {
            listed: 7,
            read: 0,
            not_served: 0,
        };
        assert_eq!(held.outstanding(), 7);
    }

    /// **The number #369 exists to make reach zero.** A class Peloton will not
    /// serve has been asked all it can be asked, so counting it as outstanding
    /// would mean a converged catalogue never said it was read — and the
    /// refresh would ask about it again on every single run for ever.
    #[test]
    fn a_class_peloton_will_not_serve_is_accounted_for_rather_than_outstanding() {
        let held = Held {
            listed: 7,
            read: 6,
            not_served: 1,
        };
        assert_eq!(held.outstanding(), 0);
    }

    /// The estimate the caller prints before a long read, and the thing that
    /// makes it worth printing: a thousand classes is minutes, not seconds.
    #[test]
    fn the_whole_library_is_announced_in_minutes() {
        let walked = Walked {
            listed: 1_052,
            added: 1_019,
            held: Held {
                listed: 1_052,
                read: 0,
                not_served: 0,
            },
        };
        let takes = walked.reading_takes();
        assert!(
            takes >= Duration::from_mins(5),
            "a thousand details is minutes: {takes:?}"
        );
        assert!(takes <= Duration::from_mins(20), "and not hours: {takes:?}");
    }

    /// A converged catalogue reads nothing, so there is nothing to announce.
    #[test]
    fn a_converged_catalogue_has_nothing_to_read() {
        let walked = Walked {
            listed: 1_052,
            added: 0,
            held: Held {
                listed: 1_052,
                read: 1_051,
                not_served: 1,
            },
        };
        assert_eq!(walked.reading_takes(), Duration::ZERO);
    }

    /// The pace is what bounds the read, so it has to be a real wait.
    #[test]
    fn the_reads_are_paced() {
        assert!(BETWEEN_REQUESTS >= Duration::from_millis(100));
        assert!(
            PER_READ > BETWEEN_REQUESTS,
            "the estimate counts the request as well as the wait"
        );
    }
}
