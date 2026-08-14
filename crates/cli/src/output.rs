//! What the operator sees.
//!
//! Every run reports both what it was served and what it wrote, because the
//! difference between them is the difference between finding nothing new and
//! finding nothing at all — and neither is a failure.

use application::{
    DerivationStatus, NormalisationSummary, RefusalReport, RunSummary, StreamStatus,
};
use domain::{
    gym::{Refusal, RefusalKind},
    landing::{LandingStream, RunOutcome, Watermark},
};

pub fn run_started(stream: &LandingStream) {
    // No run number here: the store assigns it, and this line is printed
    // before the run begins. The completion line carries it.
    println!("extracting {stream} …");
}

/// Timestamps to the second for display.
///
/// The stored value keeps its full precision — the resumption point depends on
/// sub-second times, and the source serves them — but nine decimal places in a
/// status table is noise, and it breaks the column alignment.
fn to_the_second(value: &str) -> String {
    match value.split_once('.') {
        Some((whole, _)) => format!("{whole}Z"),
        None => value.to_owned(),
    }
}

pub fn run_succeeded(summary: &RunSummary) {
    println!(
        "run {} succeeded: {} events seen, {} records landed",
        summary.run_id, summary.events_seen, summary.records_landed
    );

    match (summary.resumption_point, summary.resumption_point_moved) {
        (Some(mark), true) => println!("resumption point advanced to {mark}"),
        (Some(mark), false) => println!("resumption point unchanged at {mark}"),
        (None, _) => println!("resumption point unset: the source served nothing to advance to"),
    }
}

/// Never having run is a fact to report, not an error to raise.
pub fn status(standing: &StreamStatus, derivation: &DerivationStatus) {
    println!(
        "{:<16} {:<22} {:>11} {:>15} {:>13}",
        "stream", "last succeeded", "events seen", "records landed", "records held"
    );

    let (when, seen, landed) = match standing.last_success.as_ref().map(ExtractionRunView::of) {
        Some(view) => (view.finished_at, view.events_seen, view.records_landed),
        None => ("never".to_owned(), "-".to_owned(), "-".to_owned()),
    };

    println!(
        "{:<16} {:<22} {:>11} {:>15} {:>13}",
        standing.stream.to_string(),
        when,
        seen,
        landed,
        // `.to_string()` first: a Display impl that forwards through
        // `write!(f, "{}", ..)` discards the width and fill flags, so passing
        // the value directly would ignore the column width.
        standing.records_held.to_string()
    );

    match standing.resumption_point {
        Some(mark) => println!("\nresumption point: {mark}"),
        None => println!("\nresumption point: unset — the next run collects the full history"),
    }

    derivation_status(derivation);
}

/// The derivation's half of § 38.
///
/// An extraction that is up to date and a derivation eight records behind is a
/// system with a silent problem, and `records behind` is the one number that
/// makes it visible.
fn derivation_status(standing: &DerivationStatus) {
    let derived = standing
        .last_success
        .as_ref()
        .and_then(|run| run.outcome().finished_at())
        .map_or_else(|| "never".to_owned(), |at| to_the_second(&at.to_string()));

    println!("\nnormalisation");
    println!("  last succeeded     {derived}");
    println!("  workouts           {}", standing.workouts_held);
    println!("  refusals           {}", standing.refusals_held);

    let behind = standing.records_behind.as_usize();
    if behind == 0 {
        println!("  records behind     0");
    } else {
        println!("  records behind     {behind} — raw has moved since; run `fitness normalise`");
    }
}

struct ExtractionRunView {
    finished_at: String,
    events_seen: String,
    records_landed: String,
}

impl ExtractionRunView {
    fn of(run: &domain::landing::ExtractionRun) -> Self {
        match run.outcome() {
            RunOutcome::Succeeded {
                finished_at,
                events_seen,
                records_landed,
            } => Self {
                finished_at: to_the_second(&finished_at.to_string()),
                events_seen: events_seen.to_string(),
                records_landed: records_landed.to_string(),
            },
            // `latest_success` only ever returns a success; the other arms
            // exist because the type says they could and guessing would be
            // worse than saying so.
            RunOutcome::InFlight | RunOutcome::Failed { .. } => Self {
                finished_at: "unknown".to_owned(),
                events_seen: "-".to_owned(),
                records_landed: "-".to_owned(),
            },
        }
    }
}

pub fn reset(stream: &LandingStream, previous: Option<Watermark>) {
    match previous {
        Some(mark) => println!(
            "resumption point for {stream} cleared (was {mark}); \
             the next run collects the full history"
        ),
        None => println!("resumption point for {stream} was already unset"),
    }
    println!("nothing was landed and nothing was removed");
}

pub fn derivation_started(stream: &LandingStream) {
    println!("deriving {stream} …");
}

/// The four numbers that must add up.
///
/// `records read` equals workouts plus withdrawals plus retractions plus
/// records refused. A record that went missing shows up as arithmetic that does
/// not reconcile, without anyone having to query a table.
///
/// Refusals are reported and do not affect the exit code. A run that recorded
/// 26 of them succeeded — it found 26 things wrong with the data and said so,
/// which is the feature working.
pub fn derivation_succeeded(summary: &NormalisationSummary) {
    println!("derivation {} succeeded", summary.run_id);
    println!("  records read       {:>5}", summary.records_read);
    println!("  workouts written   {:>5}", summary.workouts_written);
    if summary.workouts_retracted.as_usize() > 0 {
        println!("  workouts withdrawn {:>5}", summary.workouts_retracted);
    }
    println!("  retractions        {:>5}", summary.retractions_read);
    if summary.records_refused.as_usize() > 0 {
        println!("  records refused    {:>5}", summary.records_refused);
    }
    println!("  refusals           {:>5}", summary.refusals_recorded);

    if !summary.reconciles() {
        // Not a failure to exit on — the derivation happened and its output is
        // real. It is a loud note that this program's accounting is wrong,
        // which is a defect worth seeing rather than swallowing.
        println!("  ! these do not add up: a record is unaccounted for");
    }
}

/// Grouped by what an operator should *do*, not by record.
///
/// Three different actions — fix it at source, live with it, or note it as
/// evidence for a later feature — and the model of record says telling them
/// apart is the point of recording them at all.
pub fn refusals(stream: &LandingStream, report: &RefusalReport) {
    let when = report
        .derived_at
        .map_or_else(|| "never".to_owned(), |at| to_the_second(&at.to_string()));

    if report.refusals.is_empty() {
        println!("{stream} — nothing refused in the derivation of {when}");
        return;
    }

    println!(
        "{} — {} refusals from the derivation of {when}",
        stream,
        report.refusals.len()
    );

    for kind in [
        RefusalKind::WrongData,
        RefusalKind::DeclaredLimitation,
        RefusalKind::Unmodelled,
    ] {
        let of_kind: Vec<&Refusal> = report
            .refusals
            .iter()
            .filter(|refusal| refusal.kind() == kind)
            .collect();
        if of_kind.is_empty() {
            continue;
        }

        println!("\n{kind} ({})", of_kind.len());
        for refusal in of_kind {
            let exercise = refusal
                .exercise
                .map_or_else(String::new, |exercise| format!("  {exercise}"));
            println!(
                "  {}  {:<22}{exercise}",
                short(refusal.source_record_id.as_str()),
                refusal.locus.to_string(),
            );
            println!("      {}", refusal.reason);
        }
    }
}

/// Enough of an identifier to find the record, without a full UUID per line.
fn short(id: &str) -> String {
    id.chars().take(8).collect()
}
