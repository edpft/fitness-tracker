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

use application::{DiaryStore as _, FtpHistory};
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingSession, DeliveredRide, Ftp, PlannedRide, Ride,
        RideVenue, SessionPosition, clock,
    },
    measure::PositiveDuration,
    schedule::{Discipline, TrainingWeek},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore, SqliteDiaryStore, SqliteFtpHistory,
    connect,
    peloton::{PelotonClasses, PelotonStack},
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
    let week = cycling_week(&SqliteDiaryStore::new(pool.clone()), from).await?;

    let (programme, next) = application::cycling::next_ride(&store, from, &week)
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
    let published = week.map_or_else(String::new, |one| {
        format!(
            "{} µ{} session {}",
            programme.programme(),
            one.published_ordinal(),
            planned.published_session()
        )
    });
    println!(
        "{} — microcycle {microcycle} of {}, session {position_number} of {sessions}",
        programme.programme(),
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
    let week = cycling_week(&SqliteDiaryStore::new(pool.clone()), from).await?;
    let (programme, next) = application::cycling::next_ride(&store, from, &week)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::USAGE))?;

    println!(
        "{} — microcycle {} of {}, session {} of {}",
        programme.programme(),
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
async fn cycling_week(diary: &SqliteDiaryStore, from: Date) -> Result<TrainingWeek, Failure> {
    let diary = diary
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    diary
        .training_week(from, Discipline::Cycling)
        .ok_or_else(|| {
            Failure::message(
                format!(
                    "the schedule gives cycling no day of the week as of {from}, so there \
                     is no day to ride on. Record the week first: fitness schedule add"
                ),
                exit::USAGE,
            )
        })
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
            programme: programme.programme().name().clone(),
            microcycle,
            session: next.session,
            classes: written,
            delivered_at: jiff::Timestamp::now(),
        })
        .await
        .map_err(Failure::from)
}
