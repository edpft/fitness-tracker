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
//! **The FTP is still an argument.** It is optional and its absence is not
//! filled in — a session prints its zones either way, and prints watts only when
//! told what a zone is a share of. A default here would be a number nobody
//! decided. Taking it from the record instead is issue #56.

use std::path::Path;

use domain::{
    cycling::{
        CyclingMicrocycle, CyclingProgramme, CyclingSession, Ftp, PlannedRide, Ride,
        SessionPosition, clock,
    },
    gym::PositiveDuration,
};
use infrastructure::{SqliteCyclingProgrammeStore, connect};
use jiff::civil::{Date, Weekday};

use crate::{Failure, exit};

/// The operator's own cool-down, ridden after the minute Peloton builds in.
///
/// A generation parameter (§ 14) held here rather than in the authored
/// programme, so the record of what a class actually contains stays faithful.
const EXTRA_COOL_DOWN_SECONDS: u64 = 300;

/// Print the next cycling session at or after `from`.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no cycling programme covers the
/// date, or if the programmes that do have no riding day left.
pub async fn next(database: &Path, from: Date, ftp: Option<Ftp>) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let store = SqliteCyclingProgrammeStore::new(pool);

    let (programme, next) = application::cycling::next_ride(&store, from)
        .await
        .map_err(|error| match error {
            application::PrescriptionError::NoProgramme { .. } => Failure::message(
                format!(
                    "no cycling programme covers {from}. \
                     Author one first: fitness plan"
                ),
                exit::USAGE,
            ),
            other => Failure::message(other.to_string(), exit::USAGE),
        })?;

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
    Ok(())
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
    programme: &CyclingProgramme,
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
    let published = week.map_or_else(String::new, |one| {
        format!("{} session {}", one.from(), planned.published_session())
    });
    println!(
        "{} — microcycle {microcycle} of {}, session {position_number} of {sessions}",
        programme.name(),
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
        None => println!("  Pass --ftp to see what each zone means in watts."),
    }
}

/// Put the next cycling session in the Peloton stack.
///
/// **A separate verb from `next`, as `deliver` is from `prescribe`.** Printing a
/// session reads; stacking it writes to the operator's account, and a read
/// command that quietly writes is the wrong shape however convenient.
///
/// **It refuses rather than discarding.** `modifyStack` replaces the whole list
/// — there is no append and no remove — so stacking on top of a stack the
/// operator queued by hand would throw his away without saying so. `--replace`
/// is how he says to do it anyway.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no cycling programme covers the
/// date, if Peloton will not answer, or if the stack holds something and
/// `replace` was not given.
pub async fn stack(
    database: &Path,
    from: Date,
    replace: bool,
    classes: &infrastructure::peloton::PelotonClasses,
    stack: &infrastructure::peloton::PelotonStack,
) -> Result<(), Failure> {
    let pool = connect(database).await?;
    let store = SqliteCyclingProgrammeStore::new(pool);
    let (programme, next) = application::cycling::next_ride(&store, from)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    let held = stack
        .view()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::SOURCE))?;
    if !held.is_empty() && !replace {
        return Err(Failure::message(
            format!(
                "the stack already holds {} class(es). Stacking replaces the whole list, \
                 so this would discard them — pass --replace to do it anyway",
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

    println!(
        "{} — microcycle {} of {}, session {} of {}",
        programme.name(),
        next.microcycle,
        programme.duration_weeks(),
        next.session.as_u8(),
        programme
            .microcycle(next.microcycle)
            .map_or(0, CyclingMicrocycle::session_count),
    );
    println!("{}, {}", weekday_name(next.date.weekday()), next.date);
    println!();
    for venue in next.ride.at().iter() {
        println!("  stacked  {venue}");
    }
    println!(
        "  stacked  {} — {}",
        cool_down.title,
        whoever(taught_by.as_ref())
    );
    println!();
    // **A total of zero is Peloton's answer, not a failure.** It counts a
    // 45-minute ride as 2700 and the FTP warm-up and test pair as nothing, so
    // the total is reported where there is one and the class count carries the
    // rest. Forcing it through `PositiveDuration` turned a stacked session into
    // an error after the write had already landed.
    match PositiveDuration::from_seconds(stacked.total_seconds) {
        Ok(total) => println!("  {} classes, {}", stacked.count(), clock(total)),
        Err(_) => println!(
            "  {} classes, and Peloton gives the stack no total",
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
