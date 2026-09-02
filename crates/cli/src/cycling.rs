//! `fitness cycling next` — which ride, and what it means in watts.
//!
//! **Not the gym's pipeline, and deliberately not pretending to be.** `gym next`
//! is porcelain over four steps because the gym has a source to collect from and
//! a sink to deliver to. Cycling has neither built: decision 0025 settled that
//! Peloton *should* be both, and until it is, this command prescribes and stops.
//! Wiring it through [`crate::gym::next`] would need a landing stream that does
//! not exist, and inventing one to satisfy the shape would be the tail wagging
//! the dog.
//!
//! **What it needs that the store does not yet hold**: when the programme
//! starts, and the operator's FTP. Both arrive as arguments for now. The FTP is
//! optional and its absence is not filled in — a session prints its zones either
//! way, and prints watts only when told what a zone is a share of. **A default
//! here would be a number nobody decided**, which is exactly what decision 0024
//! records going wrong before.

use jiff::civil::{Date, Weekday};

use domain::{
    cycling::{CycleDay, CyclingSession, Ftp, Ride, Selection, clock, peak_your_power_zones},
    gym::PositiveDuration,
};

use crate::{Failure, exit};

/// The operator's own cool-down, ridden after the minute Peloton builds in.
///
/// A generation parameter (§ 14) held here rather than in the transcribed
/// programme, so the record of what a class actually contains stays faithful.
const EXTRA_COOL_DOWN_SECONDS: u64 = 300;

/// Which cycle days the operator rides, and on which weekday.
///
/// **Days 1 and 6** (decision 0025), on two independent grounds: the pairing
/// preserves the programme's zone distribution four times better than days 1+3,
/// and it is the only pairing that takes the week 8 FTP retest. Sunday morning
/// gets day 6 because day 6 is the long ride and Sunday is the only slot long
/// enough — which is the operator's own constraint, and matches the cycling
/// slots already authored into `training_slot` on 2026-08-25.
fn selection() -> Result<Selection, Failure> {
    let days = vec![
        (
            Weekday::Wednesday,
            CycleDay::new(1).map_err(|error| Failure::usage(&error))?,
        ),
        (
            Weekday::Sunday,
            CycleDay::new(6).map_err(|error| Failure::usage(&error))?,
        ),
    ];
    Selection::new(days).map_err(|error| Failure::usage(&error))
}

/// Print the next cycling session at or after `from`.
///
/// # Errors
///
/// [`Failure`] if the programme will not build, if the date is not one the
/// programme covers, or if the arguments do not parse.
pub fn next(from: Date, start: Date, ftp: Option<Ftp>) -> Result<(), Failure> {
    let programme = peak_your_power_zones().map_err(|error| Failure::usage(&error))?;
    let selection = selection()?;

    if from < start {
        return Err(Failure::message(
            format!("the programme starts on {start} and {from} is before it"),
            exit::USAGE,
        ));
    }

    let Some((date, day)) = next_riding_day(from, &selection) else {
        return Err(Failure::message(
            "no cycling day falls in the week after that date",
            exit::USAGE,
        ));
    };

    let week_number = week_of(date, start);
    let Some(week) = programme.week(week_number) else {
        return Err(Failure::message(
            format!(
                "{} runs {} weeks and {date} would be week {week_number}",
                programme.name(),
                programme.duration_weeks(),
            ),
            exit::USAGE,
        ));
    };
    let Some(session) = week.session(day) else {
        return Err(Failure::message(
            format!("week {week_number} has no {day}"),
            exit::USAGE,
        ));
    };

    let extra = PositiveDuration::from_seconds(EXTRA_COOL_DOWN_SECONDS)
        .map_err(|error| Failure::usage(&error))?;
    let session = session.with_extra_cool_down(extra);

    report(
        programme.name().as_ref(),
        date,
        week_number,
        day,
        &session,
        ftp,
    );
    Ok(())
}

/// The first date at or after `from` that the selection rides.
///
/// Looks a week ahead and no further: a selection names weekdays, so if none of
/// the next seven days rides, none ever will.
fn next_riding_day(from: Date, selection: &Selection) -> Option<(Date, CycleDay)> {
    (0..7).find_map(|offset| {
        let date = from.checked_add(jiff::Span::new().days(offset)).ok()?;
        selection.cycle_day(date).map(|day| (date, day))
    })
}

/// Which programme week a date falls in, counting from one.
fn week_of(date: Date, start: Date) -> usize {
    let days = (date.since(start)).map_or(0, |span| span.get_days().max(0));
    usize::try_from(days / 7 + 1).unwrap_or(1)
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
    programme: &str,
    date: Date,
    week: usize,
    day: CycleDay,
    session: &CyclingSession,
    ftp: Option<Ftp>,
) {
    println!("{programme} — week {week}, {day}");
    println!("{}, {date}", weekday_name(date.weekday()));
    println!();

    if let Some(class) =
        infrastructure::peloton::mapping::session(u8::try_from(week).unwrap_or(0), day.as_u8())
    {
        for peloton in class.classes() {
            println!("  {} — {}", peloton.title(), peloton.instructor());
            println!("  {}", peloton.url());
        }
        if !class.available() {
            println!();
            println!(
                "  ! this class reads Unavailable on the operator's account. \
                 A substitute has to match its zone profile."
            );
        }
        println!();
    }

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
