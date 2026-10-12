//! `fitness cycling next` — which ride, and what it means in watts.
//!
//! **It reads the store and nothing else.** Until 2026-09-05 this command took
//! the programme's start date as a flag on every run and read a programme
//! compiled into the binary; now `fitness plan` authors the mesocycles once and
//! this answers from them. No network call is made — what a class contains was
//! read when the programme was authored and is stored in full, so a source being
//! unavailable costs this command nothing (§ 36) and a ride prescribed last
//! month stays reproducible (§ 13).
//!
//! **Not the gym's pipeline, and deliberately not pretending to be.** `gym next`
//! is porcelain over four steps because the gym has a source to collect from and
//! a sink to deliver to. Cycling has neither built: decision 0025 settled that
//! Peloton should be both, and until it is, this command prescribes and stops.
//!
//! **The FTP comes from the record** (issue #56, 2026-09-08). Every value in it
//! is a twenty-minute test's stated average power times 0.95, and the one that
//! applies is the one in force on the session's date — § 13's effect-dating,
//! resolved here rather than typed on every run. `--ftp` survives as an
//! override and asserts what it is given; without it and without a test, the
//! zones still print, because a session that cannot say what a zone is in watts
//! is still the session to ride.

use std::path::Path;

use application::{
    DiaryStore as _, PlanAuthor as _, PlanStore as _, RidesPlaced,
    cycling::{PrescribedRide, RideDeliveryError, Riding, RidingError},
};
use domain::{
    cycling::{CyclingMicrocycle, Ftp, PlannedRide, Ride, clock},
    measure::PositiveDuration,
    normalised::OperatorZone,
    plan::{Plan, Programme},
    schedule::{Relative, SessionRole},
};
use infrastructure::{
    SqliteDiaryStore, SqliteGenerationParameterStore, SqlitePelotonClassStore, SqlitePlanStore,
    SqliteRiddenVenues, connect,
    peloton::{
        PelotonClasses, PelotonHoldingRides, PelotonRideDestination, PelotonStack,
        catalogue::ClassCatalogue,
    },
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, output};

/// The next cycling session: what it is, and put where it is ridden.
///
/// **Porcelain, as `gym next` is.** That one collects, normalises, prescribes
/// and then delivers to Hevy; this one prescribes and delivers to Peloton,
/// because writing the workout to the app is most of what the tool is for. The
/// operator, 2026-09-06: *"one of the main benefits of this tool is to write
/// workouts to Peloton or Hevy, so that I don't have to do it manually myself,
/// why would I say no?"*
///
/// **The session prints before anything is sent**, and it is read from the
/// store rather than the network — so a Peloton that is unreachable costs the
/// delivery and not the answer (§ 36). The programme is already authored, so a
/// failed delivery costs a retry rather than anything derived.
///
/// **The absence of a destination carries its own reason**, which is why `to`
/// is a `Result` rather than an `Option` (#184). A `None` says only that
/// nothing will be sent, and the one thing an operator needs at that point is
/// *why* — so the reason travels with the absence and no call site can print
/// the session and forget to mention it.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no cycling programme covers the
/// date, if the programmes that do have no riding day left, or if the session
/// cannot be delivered.
pub async fn next(
    database: &Path,
    zone: &OperatorZone,
    from: Date,
    ftp: Option<Ftp>,
    to: Result<(&PelotonClasses, &PelotonStack), &str>,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let stored = SqlitePelotonClassStore::new(pool.clone());
    let catalogue = ClassCatalogue::over(&stored, to.ok().map(|(classes, _)| classes));
    let riding = configurator::next::riding(&pool, zone, catalogue)?;
    let ride = prescribed(&riding, &pool, zone, from, ftp).await?;

    report(&ride);
    println!();
    match to {
        // **`next` replaces whatever the stack holds** (the operator,
        // 2026-09-21: "The default behaviour should be replace"). It is the
        // command he trains from, and it named a `--replace` it did not take,
        // so there was no way through from it at all (#121). What it replaced
        // is said, not silently discarded.
        Ok((classes, stack)) => {
            let placed = riding
                .deliver(&ride, &PelotonRideDestination::new(classes, stack), true)
                .await
                .map_err(|error| delivery_failure(&error))?;
            delivered(&ride, &placed);
            Ok(())
        }
        Err(why) => {
            output::not_delivered(why);
            Ok(())
        }
    }
}

/// The ride at or after a date, as the plan now stands.
async fn prescribed<S, P, H, R, F, Y>(
    riding: &Riding<S, P, H, R, F, Y>,
    pool: &infrastructure::SqlitePool,
    zone: &OperatorZone,
    from: Date,
    ftp: Option<Ftp>,
) -> Result<PrescribedRide, Failure>
where
    S: application::CyclingMesocycleStore + Clone + Send + Sync,
    P: application::PlanStore + Clone + Send + Sync,
    H: application::HoldingRides + Sync,
    R: application::RiddenVenues + Sync,
    F: application::FtpHistory + Sync,
    Y: application::CyclingDeliveryStore + Sync,
{
    let reschedule = crate::rescheduling::reschedule(pool, zone).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    riding
        .next(from, reschedule, &diary, ftp)
        .await
        .map_err(|error| riding_failure(from, &error))
}

/// What the operator is told when the next ride cannot be said.
pub fn riding_failure(from: Date, error: &RidingError) -> Failure {
    match error {
        RidingError::Prescription(application::PrescriptionError::NoPlan { .. }) => {
            Failure::message(
                format!("no cycling programme covers {from}. Author one first: fitness plan"),
                exit::USAGE,
            )
        }
        RidingError::NoDay { .. } => Failure::message(
            format!("{error}. Record the week first: fitness schedule add"),
            exit::USAGE,
        ),
        RidingError::Holding(_) => Failure::message(error.to_string(), exit::SOURCE),
        RidingError::Store(_) => Failure::message(error.to_string(), exit::STORE),
        RidingError::Prescription(_) => Failure::message(error.to_string(), exit::USAGE),
    }
}

/// What the operator is told when a ride was not delivered.
fn delivery_failure(error: &RideDeliveryError) -> Failure {
    match error {
        RideDeliveryError::Occupied { .. } => Failure::message(
            format!("{error} — pass --replace to do it anyway"),
            exit::USAGE,
        ),
        RideDeliveryError::Source(_) => Failure::message(error.to_string(), exit::SOURCE),
        RideDeliveryError::Store(_) => Failure::message(error.to_string(), exit::STORE),
        RideDeliveryError::Unbuildable { .. } => Failure::usage(&error),
    }
}

/// `jiff`'s `Weekday` has no `Display` — a week has no universal first day and
/// no universal spelling. `crate::scheduling` names them the same way.
const fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

/// Where a ride sits, as its first line says it.
///
/// **Both numbers are the programme's own, and the publisher's are not
/// printed** (#211). Microcycle 3 of 4 may be Build's fourth, and session 2 of
/// 2 may be its third; the published numbering matters to plan authoring, which
/// maps a publisher's programme onto ours, and to nothing here. A holding ride
/// is no microcycle of the programme, so it says what it is instead.
pub fn heading(ride: &PrescribedRide) -> String {
    let next = &ride.next;
    let position = next.session.as_u8();
    if ride.holding() {
        return format!("a holding microcycle — session {position} of 2");
    }
    let sessions = ride
        .mesocycle
        .microcycle(next.microcycle)
        .map_or(0, CyclingMicrocycle::session_count);
    format!(
        "{} — microcycle {} of {}, session {position} of {sessions}",
        ride.mesocycle.provenance(),
        next.microcycle,
        ride.mesocycle.duration_weeks(),
    )
}

fn report(ride: &PrescribedRide) {
    let date = ride.next.date;
    println!("{}", heading(ride));
    println!("{}, {date}", weekday_name(date.weekday()));
    println!();
    session(ride);
}

/// The ride itself: where it is done, its intervals, and its zones in watts.
pub fn session(ride: &PrescribedRide) {
    let planned: &PlannedRide = &ride.next.ride;
    let session = &ride.session;
    let ftp = ride.ftp;

    // **The driving adapter knows where this build's rides are done.** A venue
    // is an opaque reference everywhere below here; that this build's cycling
    // comes from Peloton is a fact about the composition root, and turning a
    // reference into a link is the one place it is allowed to show.
    for venue in planned.at().iter() {
        println!("  {venue}");
        println!("  {}", infrastructure::peloton::url_for(venue.reference()));
        if infrastructure::peloton::is_known_unavailable(venue.reference()) {
            println!(
                "  ! this class reads Unavailable on the operator's account. \
                 A substitute has to match its zone profile."
            );
        }
    }
    println!();

    println!("  warm up   {}", clock(session.warm_up()));

    match session.ride() {
        Ride::Effort(duration) => {
            println!("  ride      {} — as hard as you can hold", clock(*duration));
            println!();
            println!("  No zones: this is the test that measures what a zone is a share of.");
        }
        Ride::Intervals(intervals) => {
            println!("  ride      {}", clock(session.ride().duration()));
            println!();
            for interval in intervals.iter() {
                // Watts only where an FTP was given. No placeholder stands in:
                // a zone with no FTP behind it has no watts, and inventing one
                // would print a number nobody decided.
                let watts = ftp.map_or_else(String::new, |ftp| {
                    format!("   {}", interval.zone().band().watts_at(ftp))
                });
                println!(
                    "    {:<3} {:>6}{watts}",
                    interval.zone().to_string(),
                    clock(interval.duration()),
                );
            }
            println!();
            println!("  time in zone");
            for (zone, seconds) in session.ride().time_in_zone() {
                let spent = PositiveDuration::from_seconds(seconds)
                    .map_or_else(|_| "\u{2014}".to_owned(), clock);
                println!("    {zone:<3} {spent:>6}   {}", zone.purpose());
            }
        }
    }

    if let Some(cool_down) = session.cool_down() {
        println!();
        println!("  cool down {}", clock(cool_down));
    }
    println!();
    println!("  total     {}", clock(session.total()));

    match ftp {
        Some(ftp) => println!("  at FTP    {ftp}"),
        None => println!(
            "  No FTP test precedes this session, so the zones print without watts. \
             Ride one and collect it, or pass --ftp."
        ),
    }
}

/// Deliver the next cycling session to Peloton.
///
/// **The same act `deliver` performs for the gym**, and named the same. What
/// differs is only where a session lands: Hevy holds a routine per date, and
/// Peloton holds one stack for the rider. Naming this `stack` would have put
/// the vendor's noun in the command surface, which is the mistake migration
/// 0020 avoided when it made `discipline` the activity and never the vendor.
///
/// **One real asymmetry, and it is why this asks.** The stack is not per date
/// and not ours: it is shared across disciplines and devices, and Peloton
/// offers no way to add to it without replacing it. So delivering over a stack
/// the operator filled by hand would discard it silently, and `--replace` is
/// how he says to do it anyway. Hevy needs no such question, because a routine
/// for a date is a place this tool put something in.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no cycling programme covers the
/// date, if Peloton will not answer, or if the stack holds something and
/// `replace` was not given.
pub async fn deliver(
    database: &Path,
    zone: &OperatorZone,
    from: Date,
    replace: bool,
    classes: &PelotonClasses,
    stack: &PelotonStack,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let stored = SqlitePelotonClassStore::new(pool.clone());
    let riding = configurator::next::riding(&pool, zone, ClassCatalogue::new(&stored, classes))?;
    let ride = prescribed(&riding, &pool, zone, from, None).await?;

    println!("{}", heading(&ride));
    println!(
        "{}, {}",
        weekday_name(ride.next.date.weekday()),
        ride.next.date
    );
    println!();
    let placed = riding
        .deliver(&ride, &PelotonRideDestination::new(classes, stack), replace)
        .await
        .map_err(|error| delivery_failure(&error))?;
    delivered(&ride, &placed);
    Ok(())
}

/// What went to the stack, and what it now holds.
pub fn delivered(ride: &PrescribedRide, placed: &RidesPlaced) {
    if placed.replaced > 0 {
        println!(
            "  replaced   the {} class(es) that were in the stack",
            placed.replaced
        );
    }
    for venue in ride.next.ride.at().iter() {
        println!("  delivered  {venue}");
    }
    println!(
        "  delivered  {} — {}",
        placed.cool_down.called(),
        whoever(placed.taught_by.as_deref())
    );
    println!();
    match placed.total {
        Some(total) => println!("  {} classes in the stack, {}", placed.holds, clock(total)),
        None => println!(
            "  {} classes in the stack, and Peloton gives it no total",
            placed.holds
        ),
    }
}

/// Who taught it, or that nobody said.
fn whoever(instructor: Option<&str>) -> String {
    instructor.map_or_else(
        || "instructor unknown, so this is the one every session falls back to".to_owned(),
        str::to_owned,
    )
}

/// `fitness cycling hold` — give cycling a week that holds its place.
///
/// **What a discipline does while the other repeats a week** (#177 rule 4).
/// The gym missed its entry test and runs that microcycle again; cycling has
/// nothing to repeat and must not run ahead, so it rides a week that trains
/// without advancing the programme. The two stay aligned at their mesocycle
/// boundaries, which is decision 0034.
///
/// **It takes the newest class of each kind the operator has not ridden.** The
/// catalogue says which classes could be the harder and the easier ride; the
/// record says which he has already done. Neither question is asked of the
/// other.
///
/// **Later cycling mesocycles move back a week.** A holding week occupies one,
/// and a plan refuses two mesocycles over one day — so inserting one without
/// shifting what follows would be refused, and shifting is what "holds its
/// place" means. The gym side is untouched: it is the discipline that is
/// repeating, and its dates are already where they should be.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no plan covers the week, if
/// Peloton cannot be reached, or if every candidate class has been ridden.
pub async fn hold(
    database: &Path,
    zone: &OperatorZone,
    start: Date,
    classes: &PelotonClasses,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let parameters = SqliteGenerationParameterStore::new(pool.clone());
    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());

    // The Monday of the week asked for. A microcycle begins on a Monday and a
    // date in the middle of one names that week rather than a new one.
    let start = monday_of(start)?;

    // **The plan is found by the date, not named on the command line.** One
    // plan answers for a day — that is the overlap rule — so asking which would
    // be asking a question the store has already settled.
    let windows = plans
        .windows()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let name = windows
        .iter()
        .find(|window| window.span().covers(start))
        .map(|window| window.name().clone())
        .ok_or_else(|| {
            Failure::message(
                format!("no plan covers the week of {start}. Author one first: fitness plan"),
                exit::USAGE,
            )
        })?;
    let plan = plans
        .named(&name)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
        .ok_or_else(|| {
            Failure::message(format!("no plan is authored under {name}"), exit::STORE)
        })?;

    let stored = SqlitePelotonClassStore::new(pool.clone());
    let held = application::holding::mesocycle(
        &PelotonHoldingRides::new(ClassCatalogue::new(&stored, classes)),
        &SqliteRiddenVenues::new(pool.clone()),
        start,
        SessionRole::new(Relative::Higher, Relative::Lower),
        SessionRole::new(Relative::Lower, Relative::Higher),
    )
    .await
    .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;

    report_holding(&held, start);

    let cycling = with_holding(&plan, held, start)?;
    let gym = plan
        .gym()
        .map(|programme| programme.mesocycles().cloned().collect::<Vec<_>>());

    let rewritten = Plan::new(
        name.clone(),
        jiff::Timestamp::now(),
        gym.map(Programme::new)
            .transpose()
            .map_err(|error| Failure::usage(&error))?,
        Some(Programme::new(cycling).map_err(|error| Failure::usage(&error))?),
    )
    .map_err(|error| Failure::usage(&error))?;

    let settings = crate::wizard::ready(&parameters).await?;
    application::prescribe::Authoring::new(plans, parameters)
        .author(&rewritten, &settings)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    println!("  {name} re-authored. fitness cycling next answers from the holding week.");
    Ok(())
}

/// The plan's cycling side with a holding week put into it.
///
/// **Everything on or after the held week moves back by one.** A plan refuses
/// two mesocycles over one day, so a week inserted into an occupied calendar
/// has to push rather than overlap — and pushing is what holding the place
/// means. Anything that ended before the held week is left exactly where it is:
/// it already happened.
fn with_holding(
    plan: &Plan,
    held: domain::cycling::CyclingMesocycle,
    start: Date,
) -> Result<Vec<domain::cycling::CyclingMesocycle>, Failure> {
    let mut mesocycles = Vec::new();
    let mut later = Vec::new();
    for mesocycle in plan.cycling().into_iter().flat_map(Programme::mesocycles) {
        if mesocycle.start() < start {
            mesocycles.push(mesocycle.clone());
        } else {
            later.push(mesocycle);
        }
    }
    mesocycles.push(held);
    for mesocycle in later {
        let moved = mesocycle
            .start()
            .checked_add(jiff::Span::new().weeks(1))
            .map_err(|error| Failure::usage(&error))?;
        mesocycles.push(mesocycle.starting_on(moved));
    }
    Ok(mesocycles)
}

/// The Monday of the week a date falls in.
fn monday_of(date: Date) -> Result<Date, Failure> {
    let back = i64::from(date.weekday().to_monday_zero_offset());
    date.checked_sub(jiff::Span::new().days(back))
        .map_err(|error| Failure::usage(&error))
}

/// What the holding week holds, before it is authored.
fn report_holding(held: &domain::cycling::CyclingMesocycle, start: Date) {
    println!("a holding microcycle, from Monday {start}\n");
    let Some(week) = held.microcycle(1) else {
        return;
    };
    for (position, ride) in week.rides() {
        let minutes = ride.session().total().as_seconds() / 60;
        println!(
            "  {position}   {:<44} {} min   {}",
            ride.at().first().called(),
            minutes,
            ride.role(),
        );
    }
    println!();
}
