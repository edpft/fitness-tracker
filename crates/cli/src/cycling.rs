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

use application::{DiaryStore as _, FtpHistory, PlanAuthor as _, PlanStore as _};
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingSession, DeliveredRide, Ftp, PlannedRide, Ride,
        RideVenue, SessionPosition, clock,
    },
    measure::PositiveDuration,
    normalised::OperatorZone,
    plan::{Plan, Programme},
    schedule::{Diary, Discipline, Relative, SessionRole, TrainingWeek},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore, SqliteDiaryStore, SqliteFtpHistory,
    SqliteGenerationParameterStore, SqlitePlanStore, SqliteRiddenVenues, connect,
    peloton::{PelotonClasses, PelotonHoldingRides, PelotonStack},
};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit, output};

/// The operator's own cool-down, ridden after the minute Peloton builds in.
///
/// A generation parameter (§ 14) held here rather than in the authored
/// programme, so the record of what a class actually contains stays faithful.
const EXTRA_COOL_DOWN_SECONDS: u64 = 300;

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
    from: Date,
    ftp: Option<Ftp>,
    to: Result<(&PelotonClasses, &PelotonStack), &str>,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let store = SqliteCyclingMesocycleStore::new(pool.clone());
    let (week, diary) = cycling_week(&SqliteDiaryStore::new(pool.clone()), from).await?;

    let (programme, next) = application::cycling::next_ride(&store, from, &week, &diary)
        .await
        .map_err(|error| match error {
            application::PrescriptionError::NoPlan { .. } => Failure::message(
                format!(
                    "no cycling programme covers {from}. \
                     Author one first: fitness plan"
                ),
                exit::USAGE,
            ),
            other => Failure::message(other.to_string(), exit::USAGE),
        })?;

    // **In force on the session's date, not on today's.** The next ride may be
    // a fortnight away, and a test ridden between now and then is the one that
    // applies to it. An override stands whatever the record says.
    let ftp = match ftp {
        Some(asserted) => Some(asserted),
        None => {
            SqliteFtpHistory::new(pool.clone())
                .in_force_on(next.date)
                .await?
        }
    };

    let extra = PositiveDuration::from_seconds(EXTRA_COOL_DOWN_SECONDS)
        .map_err(|error| Failure::usage(&error))?;
    let session = next.ride.session().with_extra_cool_down(extra);

    report(
        &programme,
        next.date,
        next.microcycle,
        next.session,
        &next.ride,
        &session,
        ftp,
    );

    println!();
    match to {
        Ok((classes, stack)) => {
            deliver_ride(
                &SqliteCyclingDeliveryStore::new(pool),
                &programme,
                &next,
                classes,
                stack,
                false,
            )
            .await
        }
        Err(why) => {
            output::not_delivered(why);
            Ok(())
        }
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

fn report(
    programme: &CyclingMesocycle,
    date: Date,
    microcycle: usize,
    position: SessionPosition,
    planned: &PlannedRide,
    session: &CyclingSession,
    ftp: Option<Ftp>,
) {
    // **Both numbers are the programme's own, and the provenance line carries
    // the published ones.** Microcycle 3 of 4 may be Build's fourth, and session
    // 2 of 2 may be its third; printing the published numbering as the headline
    // is what had a two-session week reporting "session 3".
    let week = programme.microcycle(microcycle);
    let sessions = week.map_or(0, CyclingMicrocycle::session_count);
    // **Blank for a week nobody published** (#180). A holding microcycle's
    // rides come out of the catalogue one at a time; there is no µ5 session 3
    // to point back at, and printing the provenance of the *classes* here would
    // claim a programme that does not exist.
    let published = planned.published().map_or_else(String::new, |at| {
        programme
            .programme()
            .map_or_else(|| at.to_string(), |named| format!("{named} {at}"))
    });
    println!(
        "{} — microcycle {microcycle} of {}, session {position_number} of {sessions}",
        programme.provenance(),
        programme.duration_weeks(),
        position_number = position.as_u8(),
    );
    println!("{}, {date}   {published}", weekday_name(date.weekday()));
    println!();

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
    from: Date,
    replace: bool,
    classes: &PelotonClasses,
    stack: &PelotonStack,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let store = SqliteCyclingMesocycleStore::new(pool.clone());
    let (week, diary) = cycling_week(&SqliteDiaryStore::new(pool.clone()), from).await?;
    let (programme, next) = application::cycling::next_ride(&store, from, &week, &diary)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    println!(
        "{} — microcycle {} of {}, session {} of {}",
        programme.provenance(),
        next.microcycle,
        programme.duration_weeks(),
        next.session.as_u8(),
        programme
            .microcycle(next.microcycle)
            .map_or(0, CyclingMicrocycle::session_count),
    );
    println!("{}, {}", weekday_name(next.date.weekday()), next.date);
    println!();
    deliver_ride(
        &SqliteCyclingDeliveryStore::new(pool),
        &programme,
        &next,
        classes,
        stack,
        replace,
    )
    .await
}

/// Put one session's classes in the stack, in the order they are ridden, and
/// record what went.
///
/// **The record is written after the stack, and only if the stack took it.**
/// It is the evidence that a ride was prescribed — nothing else in the store
/// says so, because a cycling session is authored in full and has no derived
/// prescription to leave behind (#185). Writing it before the delivery would
/// report a session as prescribed that Peloton refused.
///
/// **What is recorded is what went, not what the programme holds.** The
/// cool-down is chosen here, from the last class's instructor, and appears in
/// no authored record at all.
async fn deliver_ride<S: application::CyclingDeliveryStore + Sync>(
    recording: &S,
    programme: &CyclingMesocycle,
    next: &application::cycling::NextRide,
    classes: &PelotonClasses,
    stack: &PelotonStack,
    replace: bool,
) -> Result<(), Failure> {
    let held = stack
        .view()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
    if !held.is_empty() && !replace {
        return Err(Failure::message(
            format!(
                "the Peloton stack already holds {} class(es), and it is shared with \
                 everything else you queue. Delivering replaces the whole list, so this \
                 would discard them — pass --replace to do it anyway",
                held.count()
            ),
            exit::USAGE,
        ));
    }

    // **The cool down is resolved from the session's own instructor**, which the
    // authored programme does not carry — it stores where a ride is done and
    // what it is called, not who teaches it. So the last class is read back to
    // find out. Two requests, and the alternative is a migration to store a
    // field only this command wants.
    let last = next
        .ride
        .at()
        .iter()
        .fold(String::new(), |_, venue| venue.reference().to_owned());
    let taught_by = classes
        .class(&last)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?
        .instructor;
    let cool_down = classes
        .cool_down_after(taught_by.as_ref().map(|who| who.id.as_str()))
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;

    let mut rides: Vec<String> = next
        .ride
        .at()
        .iter()
        .map(|venue| venue.reference().to_owned())
        .collect();
    rides.push(cool_down.id.clone());

    let stacked = stack
        .set(&rides)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;

    for venue in next.ride.at().iter() {
        println!("  delivered  {venue}");
    }
    println!(
        "  delivered  {} — {}",
        cool_down.title,
        whoever(taught_by.as_ref())
    );

    record_delivery(recording, programme, next, &cool_down).await?;
    println!();

    // **A total of zero is Peloton's answer, not a failure.** It counts a
    // 45-minute ride as 2700 and the FTP warm-up and test pair as nothing, so
    // the total is reported where there is one and the class count carries the
    // rest. Forcing it through `PositiveDuration` turned a delivered session
    // into an error after the write had already landed.
    match PositiveDuration::from_seconds(stacked.total_seconds) {
        Ok(total) => println!(
            "  {} classes in the stack, {}",
            stacked.count(),
            clock(total)
        ),
        Err(_) => println!(
            "  {} classes in the stack, and Peloton gives it no total",
            stacked.count()
        ),
    }
    Ok(())
}

/// Who taught it, or that nobody said.
fn whoever(instructor: Option<&infrastructure::peloton::Instructor>) -> String {
    instructor.map_or_else(
        || "instructor unknown, so this is the one every session falls back to".to_owned(),
        |who| who.name.clone(),
    )
}

/// Cycling's week as of a date, which is what places a ride on a weekday.
///
/// **Read here rather than carried by the programme.** A cycling mesocycle
/// stated its own weekday map until 2026-09-20 — `cycling_weekday`, keyed on a
/// bare session ordinal — and issue #63 replaced it with a role on each ride
/// and a role on each slot. Which weekday takes which is the schedule's, so it
/// is read at the point of asking.
/// Cycling's week, and the diary it came from.
///
/// **Both, because the ride depends on both.** The week says which weekday
/// rides which role; the diary says whether a slot that week was lost to
/// illness, which eases what survives it (#180). Reading the diary twice would
/// be two reads that could disagree.
async fn cycling_week(
    diary: &SqliteDiaryStore,
    from: Date,
) -> Result<(TrainingWeek, Diary), Failure> {
    let diary = diary
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let week = diary
        .training_week(from, Discipline::Cycling)
        .ok_or_else(|| {
            Failure::message(
                format!(
                    "the schedule gives cycling no day of the week as of {from}, so there \
                     is no day to ride on. Record the week first: fitness schedule add"
                ),
                exit::USAGE,
            )
        })?;
    Ok((week, diary))
}

/// Write down the session that has just gone to the stack.
///
/// The destination is named from the catalogue rather than spelled here, for
/// the reason every other name is: one entry says where a discipline delivers,
/// and a second spelling of it is a second thing that can disagree.
async fn record_delivery<S: application::CyclingDeliveryStore + Sync>(
    recording: &S,
    programme: &CyclingMesocycle,
    next: &application::cycling::NextRide,
    cool_down: &infrastructure::peloton::ClassSummary,
) -> Result<(), Failure> {
    let destination = crate::catalogue::discipline(Discipline::Cycling.as_str())
        .map(|known| known.delivers_to().name())
        .ok_or_else(|| {
            Failure::message(
                "this build has no cycling destination to record against",
                exit::USAGE,
            )
        })?;
    let destination = application::DestinationName::try_from(destination.to_owned())
        .map_err(|error| Failure::usage(&error))?;

    let mut written: Vec<RideVenue> = next
        .ride
        .at()
        .iter()
        .map(|venue| RideVenue::new(venue.reference(), venue.called()))
        .collect::<Result<_, _>>()
        .map_err(|error| Failure::usage(&error))?;
    written.push(
        RideVenue::new(&cool_down.id, &cool_down.title).map_err(|error| Failure::usage(&error))?,
    );
    let written = NonEmpty::new(written).map_err(|error| Failure::usage(&error))?;

    let microcycle = u32::try_from(next.microcycle).map_err(|_| {
        Failure::message(
            format!("microcycle {} is past counting", next.microcycle),
            exit::USAGE,
        )
    })?;

    recording
        .record(&DeliveredRide {
            prescribed_for: next.date,
            destination,
            programme: programme.programme().map(|named| named.name().clone()),
            microcycle,
            session: next.session,
            classes: written,
            delivered_at: jiff::Timestamp::now(),
        })
        .await
        .map_err(Failure::from)
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

    let held = application::holding::mesocycle(
        &PelotonHoldingRides::new(classes),
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
