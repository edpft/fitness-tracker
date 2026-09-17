//! The gym's porcelain: the daily loop, and relative strength.
//!
//! **Porcelain over the plumbing, and nothing more.** `extract`, `normalise`,
//! `prescribe` and `deliver` are the right internal steps and are untouched;
//! what was wrong was making the operator hold the pipeline in their head to
//! answer one question — *give me my next session*. This asks it for them.
//!
//! **Nested under a discipline because that is the level at which a pipeline is
//! coherent.** Within the gym there is one source and one sink, so `next` never
//! chooses where to collect from or where to deliver to. That is also why the
//! fan-in problem does not arise: a gym prescription reads gym history, so a
//! second discipline's source being unreachable is not something this run has to
//! degrade past — it is not in this pipeline at all. Cycling would be a second
//! entry in [`crate::catalogue::DISCIPLINES`] and a second arm here.
//!
//! **Each step reports its own outcome and the first failure stops the run.**
//! Decision 0017 rejected folding delivery into `prescribe`, on the grounds that
//! one exit code would then answer for a programme problem and a network problem
//! alike. That objection is answered by composing rather than folding: the steps
//! keep their own output and their own exit codes, `prescribe` still cannot
//! deliver, and a delivery that fails costs a retry rather than a ladder
//! position — the prescription is in the store before anything is sent.
//!
//! Re-running is therefore the remedy for every partial run, and costs nothing:
//! `extract` resumes from its watermark, `normalise` re-derives what raw holds,
//! `prescribe` finds the session unchanged and writes nothing (decision 0021),
//! and `deliver` sends only into a place it does not already hold (0022).

use std::path::Path;

use crate::{
    Failure, catalogue, catalogue::KnownDiscipline, config::SourceAccess, output, prescribing,
    wiring, wiring::Command,
};
use application::strength::RelativeStrength;
use domain::{
    analytical::Rts,
    normalised::{OperatorZone, StartedAt},
};
use infrastructure::{SqlitePerformedWorkoutReader, SqliteWeighInHistory, connect};

/// Run the discipline's daily loop: collect, derive, prescribe, deliver.
///
/// The arguments are the four steps' own inputs, resolved by the caller because
/// that is where configuration is resolved for every other command. Nothing here
/// reads the environment.
pub async fn next(
    discipline: &KnownDiscipline,
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
    access: SourceAccess,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let known = discipline.collects();
    let stream = known
        .landing_stream()
        .map_err(|error| Failure::usage(&error))?;

    // 1. What has been done. Printed before the run begins, so a long first
    //    collection says what it is doing.
    output::run_started(&stream);
    let collected = wiring::run(Command::Extract(access), known, database).await?;
    report(&stream, collected);

    // 2. What it means. Contacts no source: everything it needs is now landed.
    output::derivation_started(&stream);
    let derived = wiring::run(Command::Normalise(zone.clone()), known, database).await?;
    report(&stream, derived);
    println!();

    // 3. What to do next, derived against the record the two steps above have
    //    just brought up to date. This is the step the loop exists for, and the
    //    one that was quietly reading stale history before decision 0021.
    //
    //    **The first session at or after the date, found once** (#122) and
    //    handed to both steps below, so they cannot disagree about which
    //    session they mean. Nothing planned has been said by now, and is not a
    //    failure.
    let Some(date) = prescribing::next(database, zone, date).await? else {
        return Ok(());
    };
    let date = date.to_string();
    prescribing::prescribe(database, zone, Some(&date)).await?;
    println!();

    // 4. Where to do it from. Reads what step 3 issued rather than deriving
    //    again, so a destination being unreachable costs a retry rather than a
    //    ladder position (§ 36).
    prescribing::deliver(
        database,
        zone,
        Some(&date),
        false,
        discipline.delivers_to(),
        credentials,
    )
    .await
}

/// Where body weight comes from. The one stream this report reads that the
/// discipline does not collect.
const WEIGH_INS: &str = "withings.measurements";

/// Relative strength, against a record brought up to date first.
///
/// **A source that cannot be reached is reported and stepped past.** The figure
/// reads what is landed, so an unreachable scale costs the newest weigh-ins
/// rather than the whole report (§ 36); the note says so. Deriving is not
/// stepped past: it contacts nothing, so a failure there is a real one.
pub async fn strength(
    discipline: &KnownDiscipline,
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let weigh_ins = catalogue::lookup(WEIGH_INS).ok_or_else(|| {
        Failure::message(
            format!("this build does not collect {WEIGH_INS}"),
            crate::exit::USAGE,
        )
    })?;

    for known in [discipline.collects(), weigh_ins] {
        let stream = known
            .landing_stream()
            .map_err(|error| Failure::usage(&error))?;

        output::run_started(&stream);
        let collected = match crate::source_access(known, None, credentials) {
            Ok(access) => wiring::run(Command::Extract(access), known, database)
                .await
                .map_err(Failure::from),
            Err(failure) => Err(failure),
        };
        match collected {
            Ok(outcome) => report(&stream, outcome),
            Err(failure) => output::not_collected(&stream, failure.message_text()),
        }

        output::derivation_started(&stream);
        let derived = wiring::run(Command::Normalise(zone.clone()), known, database).await?;
        report(&stream, derived);
    }
    println!();

    let pool = connect(database).await?;
    let figures = RelativeStrength::new(
        Rts,
        SqlitePerformedWorkoutReader::new(pool.clone()),
        SqliteWeighInHistory::new(pool),
    )
    .report(
        StartedAt::new(jiff::Timestamp::now(), zone.clone())
            .wall_clock()
            .date(),
    )
    .await?;
    output::strength(&figures);
    Ok(())
}

/// The two outcomes this loop can produce, printed as the plumbing prints them.
///
/// A narrower `report` than the one in `main`, and deliberately: the other three
/// `Outcome` variants belong to commands that answer a question rather than
/// advance the loop, and a match arm for them here would be a claim that `next`
/// might one day reset a watermark.
fn report(stream: &domain::landing::LandingStream, outcome: wiring::Outcome) {
    match outcome {
        wiring::Outcome::Extracted(summary) => output::run_succeeded(&summary),
        wiring::Outcome::Derived(summary) => output::derivation_succeeded(&summary),
        other => crate::report(stream, other),
    }
}
