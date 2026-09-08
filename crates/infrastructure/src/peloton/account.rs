//! Everything Peloton says about one session.
//!
//! **Two things compose here, at two levels, and § 3.1 allows both.** A ride's
//! summary comes from the workout list and its samples from a performance
//! graph, which is one thing served at two endpoints; a session is one, two or
//! three of those rides, which is one thing filed as several records. The
//! constitution covered only the first until 3.2.0.
//!
//! Peloton's shape rather than the store's, which is why it is declared here
//! and not beside the query that fills it. What one source's account of one
//! session consists of is a fact about that source.

use application::SourceAccount;
use domain::{landing::LandedRecord, sequence::NonEmpty};

/// One ride, as far as Peloton has been asked about it.
#[derive(Debug, Clone)]
pub struct LandedRide {
    /// The workout record. What identifies the ride, and the only one of the
    /// two the source names.
    pub ride: LandedRecord,
    /// The performance graph, where one has landed.
    ///
    /// Optional because a graph is a second request per ride. Since both are
    /// now collected by one command that is no longer an ordering an operator
    /// can get wrong, but it stays possible: a walk that failed part-way has
    /// landed rides whose graphs it had not reached, and that is a reason to
    /// run it again rather than a ride with its samples left out.
    pub samples: Option<LandedRecord>,
}

impl LandedRide {
    /// How many landing records this ride is composed from: one or two.
    const fn records(&self) -> usize {
        if self.samples.is_some() { 2 } else { 1 }
    }
}

/// One session, as far as Peloton has been asked about it.
///
/// Its rides are in the order they were ridden, which is what makes a
/// warm-up a warm-up and a cool-down a cool-down: the roles are positional as
/// well as declared, and a session's shape is read off the sequence.
#[derive(Debug, Clone)]
pub struct SessionAccount {
    rides: NonEmpty<LandedRide>,
    /// Earlier servings of rides in this session, kept only to be counted.
    ///
    /// **§ 10, and § 3.1 is explicit that they do not compose.** Two records
    /// sharing a source id are one source contradicting itself; treating the
    /// second as another ride would turn one ride told twice into a session of
    /// two. They are carried rather than dropped because a record with no
    /// outcome is what § 38's reconciliation exists to catch.
    superseded: Vec<LandedRide>,
}

impl SessionAccount {
    /// A session of the rides given, or `None` where there are none.
    ///
    /// Non-empty by construction rather than by check: an account of no records
    /// would read as a record with no outcome in the run's reconciliation.
    /// A session of the rides given, or `None` where there are none.
    ///
    /// `superseded` is the earlier servings of those same rides, carried only
    /// so that they are counted.
    pub fn of(rides: Vec<LandedRide>, superseded: Vec<LandedRide>) -> Option<Self> {
        NonEmpty::new(rides)
            .ok()
            .map(|rides| Self { rides, superseded })
    }

    /// The session's rides, in the order they were ridden.
    ///
    /// Non-empty as a type rather than as a promise, so reading the first one
    /// needs no index and can raise no panic (§ 26).
    pub const fn rides(&self) -> &NonEmpty<LandedRide> {
        &self.rides
    }
}

impl SourceAccount for SessionAccount {
    fn records(&self) -> usize {
        self.rides
            .iter()
            .chain(self.superseded.iter())
            .map(LandedRide::records)
            .sum()
    }

    fn superseded(&self) -> usize {
        self.superseded.iter().map(LandedRide::records).sum()
    }
}

/// Keep the last serving of each ride and set the earlier ones aside.
///
/// **Across everything, before anything is grouped.** Two servings of one ride
/// state the same times, so they would land in the same session and read as two
/// rides — but a serving of a *stretching* class, which is never grouped at
/// all, would instead become two accounts of one, each carrying the same graph
/// record, and that record would then be counted twice in a run's arithmetic.
/// Deduplicating first is what makes every landing record appear in exactly one
/// account.
///
/// Last by landing id, which is the order the source served them, because raw
/// is append-only.
pub fn supersede(rides: Vec<LandedRide>) -> (Vec<LandedRide>, Vec<LandedRide>) {
    let mut current: Vec<LandedRide> = Vec::with_capacity(rides.len());
    let mut superseded = Vec::new();

    for ride in rides {
        let id = ride.ride.source_record_id().clone();
        if let Some(existing) = current
            .iter_mut()
            .find(|held| *held.ride.source_record_id() == id)
        {
            if ride.ride.id() > existing.ride.id() {
                superseded.push(std::mem::replace(existing, ride));
            } else {
                superseded.push(ride);
            }
        } else {
            current.push(ride);
        }
    }

    // **The graph goes with the serving that stands.** Both servings of one
    // ride were paired with the same graph record — there is one, filed under
    // the source id they share — and leaving it on both would count that record
    // twice in a run's arithmetic.
    for ride in &mut superseded {
        ride.samples = None;
    }

    (current, superseded)
}
