//! What the operator sees.
//!
//! Every run reports both what it was served and what it wrote, because the
//! difference between them is the difference between finding nothing new and
//! finding nothing at all — and neither is a failure.

use application::{
    ByReason, DerivationStatus, NormalisationSummary, RefusalReport, RunSummary, StreamStatus,
};
use domain::{
    canonical::SessionCount,
    landing::{LandingStream, RunOutcome, Watermark},
    normalised::{Refusal, RefusalKind, RefusalLocus},
    prescription::{BlockPeriodisation, GymMesocycle, Linear, Progression, WeekIndex, WeekPlan},
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
    folder_landed(summary);
    resumption_point(summary);
}

/// Where the run left the resumption point, which is the fact a reader needs to
/// know whether the next run starts where this one stopped.
fn resumption_point(summary: &RunSummary) {
    match (summary.resumption_point, summary.resumption_point_moved) {
        (Some(mark), true) => println!("resumption point advanced to {mark}"),
        (Some(mark), false) => println!("resumption point unchanged at {mark}"),
        (None, _) => println!("resumption point unset: the source served nothing to advance to"),
    }
}

/// One walk of several behind a single catalogue entry.
///
/// **Named, where a single-walk run is not.** `extracting peloton.rides …`
/// opens the command and names the entry; behind it `peloton.rides` and
/// `peloton.ride_samples` are two runs against two endpoints, and before #367
/// they printed as two anonymous `run N succeeded` blocks — so the output said
/// nothing a reader could line up against the run log. One walk per line,
/// carrying the stream it ran against.
pub fn walk_succeeded(stream: &LandingStream, summary: &RunSummary) {
    println!(
        "{stream} — run {} succeeded: {} events seen, {} records landed",
        summary.run_id, summary.events_seen, summary.records_landed
    );
    resumption_point(summary);
}

/// A run over a folder, which is read whole every time and so has no
/// resumption point to report.
pub fn folder_landed(summary: &RunSummary) {
    println!(
        "run {} succeeded: {} events seen, {} records landed",
        summary.run_id, summary.events_seen, summary.records_landed
    );
}

/// Never having run is a fact to report, not an error to raise.
///
/// `resumes` is false for a stream read from a folder, which has no
/// resumption point to report.
pub fn status(standing: &StreamStatus, derivation: Option<&DerivationStatus>, resumes: bool) {
    // As wide as the longest name this build can collect, so every stream's
    // columns line up with the header's.
    let width = crate::catalogue::KNOWN
        .iter()
        .map(|known| known.name().len())
        .max()
        .unwrap_or_default();
    println!(
        "{:<width$} {:<22} {:>11} {:>15} {:>13}",
        "stream", "last succeeded", "events seen", "records landed", "records held"
    );

    let (when, seen, landed) = match standing.last_success.as_ref().map(ExtractionRunView::of) {
        Some(view) => (view.finished_at, view.events_seen, view.records_landed),
        None => ("never".to_owned(), "-".to_owned(), "-".to_owned()),
    };

    println!(
        "{:<width$} {:<22} {:>11} {:>15} {:>13}",
        standing.stream.to_string(),
        when,
        seen,
        landed,
        // `.to_string()` first: a Display impl that forwards through
        // `write!(f, "{}", ..)` discards the width and fill flags, so passing
        // the value directly would ignore the column width.
        standing.records_held.to_string()
    );

    match (resumes, standing.resumption_point) {
        (false, _) => {}
        (true, Some(mark)) => println!("\nresumption point: {mark}"),
        (true, None) => {
            println!("\nresumption point: unset — the next run collects the full history");
        }
    }

    match derivation {
        Some(derivation) => derivation_status(derivation),
        // Not a zeroed report. A stream that lands and does not yet derive has
        // no derivation to be behind on, and printing "0 of 168" would name a
        // problem that does not exist.
        None => println!("\nnothing derives from this stream yet"),
    }
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

/// A collection that did not happen, in a command that carries on without it.
///
/// **One phrasing for one concept**, whether what would not collect is the whole
/// entry or one walk behind it (#367): the stream names which, and what is
/// already landed still answers either way.
pub fn not_collected(stream: &LandingStream, why: &str) {
    println!("{stream} — not collected: {why}. Carrying on with what is already landed");
}

/// A delivery that did not happen, in a command that has already printed the
/// session.
///
/// **Said out loud, which is the whole of #184.** A credential that is absent
/// costs the delivery and not the answer (§ 36) — but a `next` that silently
/// sends nothing looks exactly like one that sent something, and the operator
/// finds out at the bike.
pub fn not_delivered(why: &str) {
    println!("not delivered: {why}");
}

/// Relative strength, one line per session per lift.
pub fn strength(report: &application::strength::StrengthReport) {
    if report.rows.is_empty() {
        println!("relative strength — no session holds a headline lift");
        return;
    }
    println!(
        "relative strength — estimated one-rep maximum over body weight ({})",
        report.estimator
    );
    let dash = || "—".to_owned();
    for lift in domain::analytical::HEADLINE_LIFTS {
        let rows: Vec<_> = report.rows.iter().filter(|row| row.lift == lift).collect();
        if rows.is_empty() {
            continue;
        }
        let with = rows.iter().filter(|row| row.relative.is_some()).count();
        println!(
            "\n{} — {with} of {} sessions have a figure",
            lift.name(),
            rows.len()
        );
        for row in rows {
            let (maximum, from) = row.estimate.map_or_else(
                || (dash(), "no set gives an estimate".to_owned()),
                |estimate| (to_a_tenth(estimate.one_rep_max), estimate.to_string()),
            );
            let body = row
                .body_mass
                .map_or_else(|| "no weigh-in".to_owned(), to_a_tenth);
            let ratio = row.relative.map_or_else(dash, |ratio| ratio.to_string());
            println!(
                "  {}  {ratio:>6}  {maximum:>10}  {body:>11}  {from}",
                row.on
            );
        }
    }
}

/// A mass to the nearest 0.1 kg, for display (#159).
///
/// An estimate is not accurate to the gram and a weigh-in is not read to one,
/// so printing grams states a confidence neither figure has. The operator,
/// 2026-10-16: estimated 1RM and body weight print to the nearest 0.1 kg. The
/// tenth is always printed, so every figure in a column states the same
/// resolution. What is stored and compared stays in grams.
fn to_a_tenth(mass: domain::measure::Kg) -> String {
    let tenths = mass.as_grams().saturating_add(50) / 100;
    format!("{}.{}kg", tenths / 10, tenths % 10)
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

/// How long a read has to be before it is worth warning about.
///
/// The first refresh against an empty catalogue is ten minutes and has to say
/// so. The weekly one is three classes and two seconds, and announcing that
/// would be commentary.
const WORTH_ANNOUNCING: std::time::Duration = std::time::Duration::from_secs(30);

/// A duration as the operator would say it.
pub fn roughly(takes: std::time::Duration) -> String {
    let seconds = takes.as_secs();
    if seconds < 90 {
        return format!("{seconds} seconds");
    }
    let minutes = (seconds + 30) / 60;
    format!("{minutes} minutes")
}

pub fn canonicalising_started(discipline: domain::schedule::Discipline) {
    println!("canonicalising {} sessions …", discipline.as_str());
}

/// What the canonicalising read and what it wrote.
///
/// **The two numbers are deliberately different things, and the gap is not a
/// loss.** Several accounts of one visit merge into one session, and a visit
/// no account holds an exercise for is no canonical session at all — the
/// operator, 2026-10-01: *"Heart rate only isn't a meaningful gym session."*
/// So `written` is below `read` twice over, and a reader who expects them to
/// reconcile would read a working run as a broken one.
pub fn canonicalised(read: SessionCount, written: SessionCount) {
    println!("canonicalising succeeded");
    println!("  normalised sessions read {read:>5}");
    println!("  canonical sessions       {written:>5}");
}

/// The canonical weigh-ins are being rebuilt.
pub fn canonicalising_weigh_ins_started() {
    println!("canonicalising weigh-ins …");
}

/// What canonicalising the weigh-ins read, wrote and left out.
///
/// **Every weighing left out is named**, because a gap is only visible if it is
/// said (§ 37): the count alone would not say which days.
pub fn weigh_ins_canonicalised(done: &application::canonicalise::WeighInsCanonicalised) {
    println!("canonicalising succeeded");
    println!("  normalised weigh-ins read {:>5}", done.read);
    println!("  canonical weigh-ins       {:>5}", done.written);
    println!("  weighings left out        {:>5}", done.left_out.len());
    for left_out in &done.left_out {
        println!("    {left_out}");
    }
}

/// The numbers that must add up.
///
/// `records read` equals `records composed` plus `records superseded` plus
/// retractions plus `records refused`. A record that went missing shows up as
/// arithmetic that does not reconcile, without anyone having to query a table.
///
/// **`workouts written` is deliberately outside that sum**, and says so by
/// standing apart from the record counts: since a session composes several
/// records, it counts a different thing from everything beside it.
///
/// Refusals are reported and do not affect the exit code. A run that recorded
/// 26 of them succeeded — it found 26 things wrong with the data and said so,
/// which is the feature working.
pub fn derivation_succeeded(summary: &NormalisationSummary) {
    println!("derivation {} succeeded", summary.run_id);
    println!("  records read       {:>5}", summary.records_read);
    println!("  workouts written   {:>5}", summary.workouts_written);
    if summary.records_composed != summary.records_read {
        println!("    from records     {:>5}", summary.records_composed);
    }
    if summary.records_superseded.as_usize() > 0 {
        println!("  records superseded {:>5}", summary.records_superseded);
    }
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

/// Grouped by what an operator should *do*, then by reason, not by record.
///
/// Three different actions — fix it at source, live with it, or note it as
/// evidence for a later feature — and the model of record says telling them
/// apart is the point of recording them at all.
///
/// **The counts come first and the records after.** A stream with thousands of
/// refusals gives a few reasons thousands of times — 1,723 of one Garmin
/// account's 2,433 were the same sentence, once per ride — and listing them one
/// by one buried the reasons worth acting on (#296). So every reason is
/// counted at the top, and below it every record that gave it, so nothing
/// needed to find a record is lost to the count.
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

    let by_reason = report.by_reason();
    let kinds = [
        RefusalKind::WrongData,
        RefusalKind::DeclaredLimitation,
        RefusalKind::Unmodelled,
    ];

    for kind in kinds {
        let of_kind: Vec<&ByReason<'_>> = by_reason
            .iter()
            .filter(|entry| entry.kind() == kind)
            .collect();
        if of_kind.is_empty() {
            continue;
        }
        let total: usize = of_kind.iter().map(|entry| entry.count()).sum();
        println!("\n{kind} ({total})");
        for entry in of_kind {
            println!("  {:>5}  {}", entry.count(), entry.reason());
        }
    }

    // Already in kind order, so the records follow the counts above.
    for entry in &by_reason {
        println!("\n{} ({})", entry.reason(), entry.count());
        for line in packed(entry.refusals().map(refused_at)) {
            println!("  {line}");
        }
    }
}

/// Where one refusal sat: the record, and below it the place and exercise
/// where those say more than the record does.
fn refused_at(refusal: &Refusal) -> String {
    let id = short(refusal.source_record_id.as_str());
    let place = match refusal.locus {
        RefusalLocus::Record => None,
        locus => Some(locus.to_string()),
    };
    let exercise = refusal.exercise.map(domain::gym::Exercise::name);
    match (place, exercise) {
        (None, None) => id,
        (Some(place), None) => format!("{id} ({place})"),
        (None, Some(exercise)) => format!("{id} ({exercise})"),
        (Some(place), Some(exercise)) => format!("{id} ({place}, {exercise})"),
    }
}

/// As many to a line as fit a terminal, so a thousand records are a hundred
/// lines rather than a thousand.
fn packed(items: impl Iterator<Item = String>) -> Vec<String> {
    /// Narrower than a terminal, after the indent.
    const WIDTH: usize = 96;
    const GAP: &str = "   ";

    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for item in items {
        if !line.is_empty() && line.chars().count() + GAP.len() + item.chars().count() > WIDTH {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push_str(GAP);
        }
        line.push_str(&item);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Enough of an identifier to find the record, without a full UUID per line.
///
/// **An identifier that is already short stays whole.** Garmin names a night by
/// its date, and the first eight characters of `2025-03-31` are a month — which
/// named the wrong thing on the first run of `refusals garmin.hrv`.
fn short(id: &str) -> String {
    /// Longer than a date and shorter than a UUID.
    const WHOLE: usize = 12;

    if id.chars().count() <= WHOLE {
        id.to_owned()
    } else {
        id.chars().take(8).collect()
    }
}

/// What was authored.
pub fn mesocycle_authored(
    id: domain::plan::PlanId,
    authored: application::Authored,
    plan: &domain::plan::Plan,
    mesocycle: &GymMesocycle,
    parameters: &crate::wizard::Ready,
) {
    let calendar = mesocycle.calendar();
    // Which of the two it was, first and in plain words. A name the store has
    // not seen starts a plan; one it has corrects that plan — and a typo in the
    // name is a new plan, so the operator has to be able to see that happen
    // rather than infer it later from two plans where one was meant.
    match authored {
        application::Authored::Created => {
            println!("created plan \"{}\"", plan.name());
        }
        application::Authored::Modified => {
            println!(
                "modified plan \"{}\" — its previous version stays as history",
                plan.name()
            );
        }
    }
    println!(
        "  plan {id}, {} gym mesocycle(s)",
        plan.gym().map_or(0, domain::plan::Mesocycles::count),
    );
    println!(
        "  this one ({}) — {}, {} primary, {} {} from {}",
        mesocycle.template(),
        mesocycle.primary_exercise(),
        mesocycle.primary(),
        calendar.duration_weeks(),
        if calendar.duration_weeks() == 1 {
            "week"
        } else {
            "weeks"
        },
        calendar.start(),
    );
    if let Some(gating) = mesocycle.gating_role() {
        println!("  gating on the {gating} session");
    }
    let skipped: Vec<String> = calendar
        .interruptions()
        .iter()
        .map(|week| week.to_string())
        .collect();
    if !skipped.is_empty() {
        // Printed because the operator has to be able to see that the block
        // knows about the holiday. The alternative — silence — looks identical
        // to the bug this replaced, where a week away quietly cost a rung.
        let label = if skipped.len() == 1 {
            "session"
        } else {
            "sessions"
        };
        println!(
            "  not running {label} {} — {} training weeks over {} calendar weeks",
            skipped.join(", "),
            calendar.duration_weeks(),
            calendar.calendar_weeks(),
        );
    }
    authored_plan(mesocycle, &parameters.planned_with);
    authored_parameters(&parameters.planned_with, &parameters.scales);
}

/// What this programme's template makes of its anchor.
///
/// Split out because it is three unrelated reports sharing a `match`, and
/// because what the versions say is a separate question from what the
/// programme does with them.
fn authored_plan(mesocycle: &GymMesocycle, with: &application::PlannedWith) {
    let calendar = mesocycle.calendar();
    match mesocycle {
        GymMesocycle::Test(test) => {
            println!(
                "  a test at {} — no anchor, because producing one is what it does",
                test.reps()
            );
            match test.asserted() {
                Some(asserted) => println!(
                    "  asserted at {asserted} — nothing ran before it, so this is \
                     the number everything after it is read from"
                ),
                None => println!(
                    "  its anchor comes from whatever measured the lift before \
                     it, as the record stands"
                ),
            }
        }
        GymMesocycle::Progression(Progression::Provided { .. }) => {
            println!(
                "  opening from whatever the mesocycle before it measures, \
                 and it does not stay fixed"
            );
            let chart = domain::prescription::SbsChart::published();
            let first_days: Vec<String> = chart
                .iter()
                .flat_map(domain::prescription::SbsChart::days)
                .filter_map(|(_, session, day)| match (session, day) {
                    (
                        domain::prescription::SbsSession::First,
                        domain::prescription::SbsDay::Percentage { sets, reps, share },
                    ) => Some(format!("{sets}×{reps} @ {share}")),
                    _ => None,
                })
                .collect();
            println!(
                "  four weeks, stated by the chart: {}",
                first_days.join(", ")
            );
            println!(
                "  the second session of each week finds a rep max, and that \
                 result is what the next week is a share of"
            );
            println!("  week 4 ends on a test, which opens the next cycle");
        }
        GymMesocycle::Progression(Progression::Linear(_)) => {
            println!(
                "  it opens {} off the failed load of whatever measured the \
                 lift before it",
                with.linear.entry_drop
            );
            println!(
                "  ladder climbs {}kg a week over {} climbing weeks, and none of \
                 them is a test",
                with.linear.ladder_climb_per_week,
                calendar.duration_weeks(),
            );
        }
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            match block.entry_test() {
                Some(test) => println!(
                    "  week one measures what the rest of it plans from, at {}",
                    test.reps()
                ),
                None => println!(
                    "  it plans from whatever measured the lift before it, \
                     which is not a week of this block"
                ),
            }
            match block.plan() {
                Ok(plan) => println!(
                    "  {} weeks of accumulation, {} of intensification, {} of \
                     realisation; the last is the exit test",
                    plan.accumulation_weeks(),
                    plan.intensification_weeks(),
                    plan.realisation_weeks(),
                ),
                Err(error) => println!("  no plan: {error}"),
            }
        }
    }
}

/// What the plan was authored against, in short.
///
/// **Split out because it is not about the mesocycle.** These are the versions
/// in force when the plan was made (#192), and they read the same whichever
/// template was being authored.
fn authored_parameters(with: &application::PlannedWith, scales: &domain::prescription::Scales) {
    println!(
        "  the light session's top set runs at {} of the heavy load",
        with.linear.light_of_heavy
    );
    println!(
        "  opening drops {} off a failed entry test",
        with.linear.entry_drop
    );
    for (implement, steps) in scales.iter() {
        println!("  {implement} loads in {steps}");
    }
    let (higher, lower) = (
        with.workouts.higher.static_hold,
        with.workouts.lower.static_hold,
    );
    if higher == lower {
        println!("  holds {higher}");
    } else {
        println!("  holds {higher} in the higher-intensity workout, {lower} in the lower");
    }
}

/// The time a version was authored, to the second. What the operator wants from
/// it is whether it predates the plan he is about to make.
fn since(at: jiff::Timestamp) -> String {
    at.strftime("%Y-%m-%d %H:%M:%S UTC").to_string()
}

fn ramp(warmup: &domain::sequence::NonEmpty<domain::prescription::WarmupStep>) {
    println!("  warm-up ramp, of the session's own top set — a floor: a work-up toward");
    println!("  a higher repetition count takes more");
    for step in warmup.iter() {
        println!("    {} × {}", step.of_top_set, step.reps);
    }
}

/// The latest version of each workout and of each primary progression, and
/// today's load scales.
///
/// **Complete rather than summarised**, which is the whole reason it exists.
/// Decision 0015's guard against a wrong shipped default was that a default is
/// never invisible, and a report that showed the interesting half would leave
/// the warm-up ramp and the reset protocols exactly as unexaminable as they were.
///
/// [`authored_parameters`] stays the short form: it runs after authoring, where
/// the question is what this plan was built against rather than what every
/// number is.
pub fn designs_in_force(
    workouts: [(&str, jiff::Timestamp, &domain::prescription::Workout); 2],
    linear: (jiff::Timestamp, &domain::prescription::LinearProgression),
    sbs: (jiff::Timestamp, &domain::prescription::SbsProgression),
    chart: &domain::prescription::SbsChart,
    scales: &domain::prescription::Scales,
) {
    for (index, (which, at, workout)) in workouts.into_iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("the {which} workout, since {}", since(at));
        workout_in_force(workout);
    }

    let (linear_at, linear) = linear;
    println!();
    println!("linear progression, since {}", since(linear_at));
    println!("  climbs {}kg a week", linear.ladder_climb_per_week);
    println!(
        "  a derived opening drops {} off a failed entry test",
        linear.entry_drop
    );
    println!(
        "  the light session's top set runs at {} of the heavy load",
        linear.light_of_heavy
    );
    println!(
        "  heavy top set × {}; light top set × {}",
        linear.top_set_reps.higher, linear.top_set_reps.lower,
    );
    println!(
        "  heavy back-off {} × {} at {} of top set; light {} × {} at {}",
        linear.back_off.higher.sets,
        linear.back_off.higher.reps,
        linear.back_off.higher.of_top_set,
        linear.back_off.lower.sets,
        linear.back_off.lower.reps,
        linear.back_off.lower.of_top_set,
    );
    println!(
        "  first stall: drop {}, re-climb {}kg a week",
        linear.first_reset.drop, linear.first_reset.reclimb_per_week,
    );
    println!(
        "  second stall: drop {}, re-climb {}kg a week",
        linear.second_reset.drop, linear.second_reset.reclimb_per_week,
    );
    println!(
        "  rest {} after a top set, {} after a back-off, {} between attempts",
        linear.rest.top_set, linear.rest.back_off, linear.rest.attempt,
    );
    ramp(&linear.warmup);

    let (sbs_at, sbs) = sbs;
    println!();
    println!("SBS, since {}", since(sbs_at));
    println!(
        "  rest {} between sets across, {} after a repetition maximum, {} after a back-off, \
         {} between attempts",
        sbs.rest.across, sbs.rest.rep_max, sbs.rest.back_off, sbs.rest.attempt,
    );
    ramp(&sbs.warmup);
    println!("  SBS's chart");
    for (week, session, day) in chart.days() {
        use domain::prescription::{SbsDay, SbsSession};
        let label = match session {
            SbsSession::First => format!("  week {week}"),
            SbsSession::Second => "        ".to_owned(),
        };
        match day {
            SbsDay::Percentage { sets, reps, share } => {
                println!("{label}  {sets}×{reps} @ {share}");
            }
            SbsDay::RepMax {
                reps,
                back_off_sets,
                back_off_reps,
            } => println!("{label}  {reps}RM, then {back_off_sets}×{back_off_reps}"),
            SbsDay::Test { reps } => println!("{label}  a test at {reps}"),
        }
    }

    println!();
    println!("what each implement can hold");
    for (implement, steps) in scales.iter() {
        // Rendered before it is padded: a `Display` that writes straight to the
        // formatter ignores a width, and `Implement`'s does.
        println!("  {:<12} {steps}", implement.to_string());
    }
}

/// One workout: its reps and rest at every level it states them, and its hold.
fn workout_in_force(workout: &domain::prescription::Workout) {
    let scheme = |scheme: &domain::prescription::AccessoryScheme| {
        format!("{} × {}", scheme.sets, scheme.reps)
    };
    // **Absent and equal are different**, so a level that says nothing about
    // supersets says nothing here either rather than repeating its between-sets
    // rest as though it had been stated.
    let rest = |rest: domain::prescription::Rest| {
        rest.after_superset.map_or_else(
            || format!("{} between sets", rest.between_sets),
            |after| {
                format!(
                    "{} between sets, {after} after a superset",
                    rest.between_sets
                )
            },
        )
    };
    println!(
        "  reps    {} unless stated below",
        scheme(&workout.reps.workout)
    );
    for (section, stated) in &workout.reps.sections {
        println!("    {:<28} {}", section.to_string(), scheme(stated));
    }
    for (superset, stated) in &workout.reps.supersets {
        println!("    {:<28} {}", superset.to_string(), scheme(stated));
    }
    for (slot, stated) in &workout.reps.exercises {
        println!("    {:<28} {}", slot.to_string(), scheme(stated));
    }
    println!(
        "  rest    {} unless stated below",
        rest(workout.rest.workout)
    );
    for (section, stated) in &workout.rest.sections {
        println!("    {:<28} {}", section.to_string(), rest(*stated));
    }
    for (superset, stated) in &workout.rest.supersets {
        println!("    {:<28} {stated} after a round", superset.to_string());
    }
    for (slot, stated) in &workout.rest.exercises {
        println!("    {:<28} {stated} between sets", slot.to_string());
    }
    println!("  holds   {}", workout.static_hold);
}

/// The mesocycle in force, with its ladder week by week and where it stands.
///
/// **The table is the point.** A rate and a duration are two numbers; what an
/// operator needs to see is the load each week asks for, and which of those weeks
/// they are actually on — which after a miss is not the week the calendar is in.
///
/// The `of anchor` column is read back out of the load rather than driving it,
/// which is what lets the climb be seen passing 100% of the max it started from.
pub fn mesocycle_standing(standing: &application::LadderStanding) {
    let mesocycle = &standing.mesocycle;
    let calendar = mesocycle.calendar();

    println!(
        "plan \"{}\" — mesocycle {} ({}) — {}, {} primary, {} {} from {}",
        standing.plan,
        standing.ordinal,
        mesocycle.template(),
        mesocycle.primary_exercise(),
        mesocycle.primary(),
        calendar.duration_weeks(),
        if calendar.duration_weeks() == 1 {
            "week"
        } else {
            "training weeks"
        },
        calendar.start(),
    );
    if let Some(gating) = mesocycle.gating_role() {
        println!("gating on the {gating} session");
    }
    match standing.history_through {
        Some(through) => println!("history through {through}"),
        // § 38: an empty record and a stale one are different, and a report that
        // printed nothing here would look the same for both.
        None => println!("history through — nothing performed yet"),
    }

    match mesocycle {
        GymMesocycle::Test(test) => test_standing(test, standing),
        GymMesocycle::Progression(Progression::Linear(linear)) => {
            linear_standing(linear, standing);
        }
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            block_standing(block, standing.anchor, &standing.scales);
        }
        GymMesocycle::Progression(Progression::Provided { .. }) => {
            sbs_standing(standing.anchor, standing.maximum);
        }
    }
}

/// An SBS cycle: where the chart stands, and the one caveat that matters.
fn sbs_standing(
    anchor: Option<domain::prescription::Anchor>,
    maximum: Option<domain::measure::Kg>,
) {
    match anchor {
        Some(anchor) => println!(
            "the chart opens from {} ({})",
            anchor.load(),
            anchor.provenance(),
        ),
        // **Nothing to print, and that is the whole of what it says.** A cycle
        // authored months ahead states nothing about what it opens from: the
        // test it opens from has not happened. It is resolved when a session is
        // asked for, and refused if that test was never recorded.
        None => println!("the chart opens from the mesocycle before it, once that has measured"),
    }
    println!(
        "  and the maximum moves inside the cycle: each rep-max day resets it, \
         so week 3 is a share of what week 2 produced"
    );
    println!(
        "  read from the record: each performed rep-max day advances it, so a \
         week nobody trained leaves it where it was"
    );
    // **And where it has got to** (#362). Saying that a number moves without
    // saying where it has moved to leaves the reader to work out the session's
    // loads for themselves, which is the question this report exists to answer.
    match maximum {
        Some(maximum) if Some(maximum) != anchor.map(domain::prescription::Anchor::load) => {
            println!("  and it stands at {maximum}kg");
        }
        Some(_) | None => {}
    }
}

/// A standalone test: what it is an attempt at, and which session takes it.
fn test_standing(test: &domain::prescription::Test, standing: &application::LadderStanding) {
    let reps = test.reps().as_u32();
    match standing.target {
        Some(target) => println!(
            "the test is for {target} at {reps}, the last of three attempts a \
             step apart"
        ),
        None => println!(
            "no target: this test takes one from the mesocycle before it, and \
             there is none in the same lift"
        ),
    }
    match test.asserted() {
        Some(_) => println!(
            "  asserted, not read — nothing ran before it for this lift to take \
             an anchor from"
        ),
        None => {
            println!("  read from whatever measured the lift before it, as the record stands");
        }
    }
    let role = domain::prescription::Test::ROLE;
    match test.provided() {
        // A published week states both its days, so the other one is the
        // taper the programme puts before its own test rather than anything
        // inherited. Naming the programme is the point: it is what says the
        // light session exists at all.
        Some(provided) => println!(
            "  the {role} session is the test; the other is {} {}, off the same target",
            provided.programme(),
            provided,
        ),
        None => println!("  the {role} session is the test; the other is the previous mesocycle's"),
    }
}

/// A linear programme: its ladder, week by week, and where the record puts it.
///
/// The `of anchor` column is read back out of the load rather than driving it,
/// which is what lets the climb be seen passing 100% of the max it started from.
fn linear_standing(linear: &Linear, standing: &application::LadderStanding) {
    let (linear_progression, scales) = (&standing.planned_with.linear, &standing.scales);
    let Some(resolved) = standing.anchor else {
        println!("no anchor: nothing has measured this lift and no test asserts one");
        return;
    };
    println!("anchor {resolved}, as the record stands on the date asked about");
    println!(
        "opening derived: {} off the anchor's failed load",
        linear_progression.entry_drop
    );

    let Ok(ladder) = linear.ladder(resolved, linear_progression, scales) else {
        println!("  no ladder: the block is too short to climb");
        return;
    };
    let anchor = resolved.load();
    let Ok(steps) = linear.steps(scales) else {
        println!("  no ladder: no load scale is authored for the primary's implement");
        return;
    };
    let Some(progress) = standing.progress else {
        println!("  no position: nothing in the record places this ladder");
        return;
    };
    let standing_week = progress.week();

    println!("  week  of anchor    heavy    light");
    for week in 1..=linear.calendar().duration_weeks() {
        let Ok(index) = WeekIndex::new(week) else {
            continue;
        };
        // Every week of a linear block is a rung (decision 0013), so this
        // only skips a week the ladder cannot price at all.
        let Some(percentage) = ladder.implied_percentage(anchor, index, steps) else {
            continue;
        };
        let heavy = ladder.heavy_top_set(index, steps);
        let light = ladder.light_top_set(index, steps, linear_progression.light_of_heavy);
        // The rung the record puts the operator on, which after a miss is behind
        // the calendar.
        let here = if index == standing_week { " ←" } else { "" };
        // Formatted into strings first: a width in a format spec only applies if
        // the `Display` impl routes through `Formatter::pad`, and these do not.
        println!(
            "  {week:>4}  {:>9}  {:>7}  {:>7}{here}",
            format!("{percentage}"),
            heavy.map_or_else(|| "—".to_owned(), |load| format!("{load}")),
            light.map_or_else(|| "—".to_owned(), |load| format!("{load}")),
        );
    }

    match progress.reset() {
        None => println!(
            "  on the plan at week {standing_week} of {}",
            ladder.climbing_weeks()
        ),
        Some(reset) => {
            let load = progress
                .heavy_top_set(ladder, steps)
                .map_or_else(|| "—".to_owned(), |load| format!("{load}"));
            println!(
                "  the {reset} reset is in play: re-climbing at {load}, and the \
                 ladder resumes at week {standing_week}"
            );
        }
    }
}

/// A periodised block: its phases, week by week.
///
/// **No position marker and no reset line.** A block reads nothing from the
/// record — every load is a share of the anchor decided by the duration and
/// three literature constants — so there is no rung a miss could hold and
/// nothing for the record to place. That is the difference between the two
/// models, and printing an arrow here would hide it.
fn block_standing(
    block: &BlockPeriodisation,
    resolved: Option<domain::prescription::Anchor>,
    scales: &domain::prescription::Scales,
) {
    let Some(resolved) = resolved else {
        println!("no anchor: nothing has measured this lift and no test asserts one");
        return;
    };
    match block.entry_test() {
        Some(test) => println!(
            "anchor {resolved}, and week one measures it again at {}",
            test.reps()
        ),
        None => println!("anchor {resolved}, measured before this block rather than in it"),
    }
    let Ok(plan) = block.plan() else {
        println!("  no plan: this duration does not hold three phases");
        return;
    };
    let Some(steps) = scales.for_exercise(block.primary_exercise()) else {
        println!("  no plan: no load scale is authored for the primary's implement");
        return;
    };
    let anchor = resolved.load();

    println!("  week  phase              sets × reps   of anchor      load");
    // The entry test is week one where there is one, so the phases below start
    // at two. It carries no share of the anchor: it is what establishes it.
    let mut number = 0;
    if let Some(test) = block.entry_test() {
        number += 1;
        println!(
            "  {number:>4}  {:<17}  {:>11}   {:>9}  {:>8}",
            "entry test",
            format!("1 × {}", test.reps().as_u32()),
            "—",
            test.light()
                .map_or_else(|| "—".to_owned(), |load| format!("{load} light")),
        );
    }
    for week in plan.weeks() {
        number += 1;
        match week {
            WeekPlan::Working {
                phase,
                sets,
                reps,
                load,
            } => println!(
                "  {number:>4}  {:<17}  {:>11}   {:>9}  {:>8}",
                format!("{phase}"),
                format!("{} × {}", sets.as_u32(), reps.as_u32()),
                format!("{load}"),
                format!("{}", steps.quantise_loaded(load.of(anchor))),
            ),
            WeekPlan::ExitTest { reps, expected } => println!(
                "  {number:>4}  {:<17}  {:>11}   {:>9}  {:>8}  ←  the exit test",
                "exit test",
                format!("1 × {}", reps.as_u32()),
                format!("{expected}"),
                format!("{}", steps.quantise_loaded(expected.of(anchor))),
            ),
        }
    }
    println!("  the block plans to finish at 105% of the maximum it entered on");
    if block
        .entry_test()
        .is_some_and(|test| test.light().is_none())
    {
        println!("  its entry-test week runs the test only");
    }
}

/// What the session's loads are shares of, in words.
///
/// **The anchor is not always it, and saying so was #362.** An SBS cycle's
/// percentages are shares of a maximum the chart advances on every
/// repetition-maximum day; the header printed the cycle's opening anchor and
/// the session below it was derived from something else. The operator, reading
/// 92.5kg over a set of 82.5kg top sets: *"is the anchor now calculated from
/// Friday night 8RM?"*
///
/// So the maximum leads where it has moved, and the anchor stays beside it
/// because where the number came from is the rest of the answer. Where nothing
/// has moved it, the anchor is the whole answer and is printed alone.
fn shares_of(
    derived_from: domain::prescription::DerivedFrom,
    week: domain::prescription::WeekKind,
    maximum: Option<domain::measure::Kg>,
) -> String {
    let derived = derived_phrase(derived_from, week);
    let anchored = derived_from
        .anchor()
        .map(domain::prescription::Anchor::load);
    match maximum {
        Some(maximum) if Some(maximum) != anchored => {
            format!("maximum {maximum}kg, advanced from {derived}")
        }
        Some(_) | None => derived,
    }
}

/// Where a session's loads came from, in words.
///
/// **A test week's anchor is a reference, not a result.** Calling it the anchor
/// reads as an established maximum, and in a test week it is not one: an entry
/// test ramps *toward* the number and an exit test is measured *against* it.
/// The two are indistinguishable here — `WeekKind::Test` covers both — so the
/// word has to be true of either, and "against" is.
///
/// Dropping the number instead would hide what the ramp is aiming at, which is
/// the thing worth knowing before the session rather than after it.
fn derived_phrase(
    derived_from: domain::prescription::DerivedFrom,
    week: domain::prescription::WeekKind,
) -> String {
    use domain::prescription::{DerivedFrom, WeekKind};

    match (derived_from, week) {
        (DerivedFrom::Anchor(anchor), WeekKind::Test) => format!("against {anchor}"),
        (DerivedFrom::Anchor(anchor), WeekKind::Climbing(_) | WeekKind::Holding) => {
            format!("anchor {anchor}")
        }
        // A standalone test has no anchor at all: what it derived from is what
        // the record put it at, and that is the number worth naming.
        //
        // `Kg` displays bare, so the unit is appended here as it is everywhere
        // else — this line read "for 95, test" until someone looked.
        (DerivedFrom::Target(target), _) => format!("for {target}kg"),
    }
}

// --- `fitness next` ---------------------------------------------------------

/// How wide a step's name is, so the outcomes line up.
const STEP_WIDTH: usize = 37;

/// A step of `next`'s run, named and padded for its outcome.
pub fn step(step: &application::next::Step) -> String {
    use application::next::Step;
    let name = match step {
        Step::Joining(parts) => joining(parts),
        Step::Fetching(stream) => format!("Fetching {stream}"),
        Step::Normalising(stream) => format!("Normalising {stream}"),
        Step::Canonicalising(discipline) => {
            format!("Canonicalising {} sessions", discipline.as_str())
        }
        Step::CanonicalisingWeighIns => "Canonicalising weigh-ins".to_owned(),
        // **The driving adapter knows which catalogue this build keeps**, as it
        // knows where rides are done.
        Step::RefreshingClasses => "Refreshing Peloton's classes".to_owned(),
        Step::Committing => "Committing the next mesocycle".to_owned(),
        Step::Skipping => "Skipping".to_owned(),
    };
    format!("{name:<STEP_WIDTH$}")
}

/// The runs a run joined, by what they collect: the whole of `next`, or the
/// streams and canonical layers (#559).
fn joining(parts: &std::collections::BTreeSet<application::Part>) -> String {
    use application::Part;
    if parts.contains(&Part::Delivering) {
        return "Joining the run of next going".to_owned();
    }
    let mut named = std::collections::BTreeSet::new();
    for part in parts {
        named.insert(match part {
            Part::Fetching(stream) | Part::Normalising(stream) => stream.to_string(),
            Part::Canonicalising(layer) => format!("canonical {}", layer.as_str()),
            Part::Delivering => continue,
        });
    }
    format!(
        "Joining the run collecting {}",
        named.into_iter().collect::<Vec<_>>().join(", ")
    )
}

/// How a step ended, and how long it took (#560).
pub fn outcome(done: &application::next::Done) -> String {
    format!("{} ({:.1} s)", ended(done), done.took.as_secs_f64())
}

/// Done with a count of what it found, or failed and why.
fn ended(done: &application::next::Done) -> String {
    use std::fmt::Write as _;

    use application::next::Found;
    let found = match &done.outcome {
        Err(failure) => return format!("failed: {failure}"),
        Ok(Found::NotDue(why)) => return why.to_string(),
        Ok(Found::Unchanged) => return "unchanged".to_owned(),
        Ok(Found::Joined) => return "done".to_owned(),
        Ok(found) => found,
    };
    let what = match found {
        Found::NotDue(_) | Found::Unchanged | Found::Joined => String::new(),
        Found::Landed(records) if records.as_usize() == 0 => "nothing new".to_owned(),
        Found::Landed(records) => format!("{records} new"),
        Found::Written(written) => format!("{written} written"),
        Found::Canonical(sessions) => format!("{sessions} sessions"),
        Found::CanonicalWeighIns { written, left_out } => {
            let mut said = format!("{written} weigh-ins");
            for left_out in left_out {
                let _ = write!(said, "; left out {left_out}");
            }
            said
        }
        Found::Classes {
            added,
            outstanding,
            stopped,
            ..
        } => {
            let mut said = match added {
                0 => "nothing new".to_owned(),
                added => format!("{added} new classes"),
            };
            if *outstanding > 0 {
                let _ = write!(said, ", {outstanding} still to read");
            }
            // **A read that stopped part-way says so, and what it read still
            // counts**: the next refresh does not pay for it again.
            if let Some(error) = stopped {
                let _ = write!(said, "; reading stopped: {error}");
            }
            said
        }
        Found::Committed { concurrent, span } => format!("{concurrent}, {span}"),
        Found::Skipped { session, skip } => {
            let mut last = skip.from();
            while let Some(next) = last.next().filter(|next| *next < skip.until()) {
                last = next;
            }
            format!(
                "{} ({}): no training from {} to {}",
                named(session),
                skip.reason(),
                when(skip.from()),
                when(last),
            )
        }
    };
    format!("done, {what}")
}

/// Whether a read is long enough to say how long before it starts (#369).
pub fn worth_announcing(takes: std::time::Duration) -> bool {
    takes >= WORTH_ANNOUNCING
}

/// A word with its first letter capitalised, to open a line.
fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

/// A session as the microcycle names it: `Gym 1`, `Cycling 2`.
fn named(session: &application::microcycle::Session) -> String {
    format!(
        "{} {}",
        capitalised(session.slot.discipline.as_str()),
        session.number
    )
}

/// A part of a day, by its weekday: `Wednesday evening`.
fn when(part: domain::schedule::DayPart) -> String {
    format!("{} {}", part.date.strftime("%A"), part.part)
}

/// Where the plan stands: the macrocycle's week, each discipline's mesocycle,
/// and this microcycle's sessions.
pub fn plan_standing(plan: &application::next::PlanStanding) {
    use application::next::{MacrocycleStanding, WeekPosition};

    match &plan.macrocycle {
        MacrocycleStanding::Week {
            macrocycle,
            week,
            of,
        } => println!(
            "{} macrocycle, week {week} of {of}",
            capitalised(&macrocycle.to_string())
        ),
        // **Said even when it cannot be known**: past the school calendar's
        // reach there is no data, which is not the same as no term.
        MacrocycleStanding::Unknown { published_to } => println!(
            "Macrocycle unknown: the school calendar is published to {}",
            published_to.map_or_else(|| "nowhere".to_owned(), |to| to.to_string())
        ),
        MacrocycleStanding::Unreadable(error) => {
            println!("Macrocycle unknown: the school calendar could not be read ({error})");
        }
    }

    let width = plan
        .disciplines
        .iter()
        .map(|(discipline, _)| discipline.as_str().chars().count())
        .max()
        .unwrap_or(0);
    for (discipline, position) in &plan.disciplines {
        let name = capitalised(discipline.as_str());
        let Some(position) = position else {
            println!("  {name:<width$}  no mesocycle this microcycle");
            continue;
        };
        let week = match position.week {
            WeekPosition::Microcycle { number, of, rerun } => format!(
                "microcycle {number} of {of}{}",
                rerun.map_or_else(String::new, |rerun| format!(", rerun {rerun}"))
            ),
            WeekPosition::Holding => "holding".to_owned(),
        };
        println!("  {name:<width$}  mesocycle {}, {week}", position.mesocycle);
    }

    let Some((monday, sessions)) = &plan.microcycle else {
        return;
    };
    println!("This microcycle, from {}", monday.strftime("%A %-d %B"));
    let rows: Vec<(String, String, String)> = sessions
        .iter()
        .map(|session| {
            (
                named(session),
                format!(
                    "{} {}",
                    session.slot.date.strftime("%a"),
                    session.slot.slot.part
                ),
                session.state.to_string(),
            )
        })
        .collect();
    let widest = |at: fn(&(String, String, String)) -> &String| {
        rows.iter()
            .map(|row| at(row).chars().count())
            .max()
            .unwrap_or(0)
    };
    let name = widest(|row| &row.0);
    let slot = widest(|row| &row.1);
    for (session, at, state) in &rows {
        println!("  {session:name$}   {at:slot$}   {state}");
    }
}

/// Which session is next, and how long it can still be performed.
pub fn next_session(session: &application::microcycle::Session) {
    let until = match session.closes {
        _ if session.closes_at_midnight() => ", until midnight".to_owned(),
        Some(closes) => format!(", until {}", when(closes)),
        None => String::new(),
    };
    println!("Next: {}, {}{until}", named(session), when(session.at()));
}

/// A session that could not be prescribed, under the line naming it.
pub fn not_prescribed(why: &dyn std::fmt::Display) {
    println!("  not prescribed: {why}");
}

/// The microcycle holds nothing left to prescribe.
///
/// Not a fault, and not silence either: a week that is done is a real answer,
/// and the slot after it says when the next one opens without prescribing it.
/// Deriving next week's session against a record that has a week left to
/// change would be issuing something nobody can train from yet.
pub fn microcycle_complete(after: Option<domain::schedule::ScheduledSlot>) {
    match after {
        Some(slot) => println!(
            "nothing in this microcycle is still to be prescribed. The next slot is {} on {} ({:?} {})",
            slot.discipline, slot.date, slot.slot.weekday, slot.slot.part,
        ),
        None => println!(
            "nothing in this microcycle is still to be prescribed, and the schedule holds no slot after it"
        ),
    }
}

pub fn no_next_slot(from: jiff::civil::Date) {
    println!("the schedule holds no slot on or after {from}");
}

/// The week has slots and no plan covers it.
///
/// **Told apart from a week with no slots**, because the two are answered by
/// different commands and a wrong one sends the operator to the wrong wizard.
pub fn no_plan_covers(week_of: jiff::civil::Date) {
    println!(
        "the week of {week_of} has training slots and no plan covers it, \
         so there is no microcycle to report. Author one: fitness plan"
    );
}

/// Nothing is programmed at or after the date asked from.
///
/// **Said, not refused.** Running out of plan is a fact about the plan rather
/// than something the operator got wrong, and the answer to it is a new plan.
pub fn nothing_planned(
    from: jiff::civil::Date,
    last: Option<&(domain::plan::PlanName, jiff::civil::Date)>,
) {
    match last {
        Some((plan, day)) => println!(
            "nothing is planned after {day} ({:?}), when {plan}'s gym mesocycles end",
            day.weekday(),
        ),
        None => {
            println!("nothing is planned on or after {from}: no gym mesocycle has been authored");
        }
    }
}

/// The prescription, as a session to train from.
pub fn prescription(issued: &application::Prescription) {
    use application::Issuance;

    let workout = &issued.workout;
    // **What the derivation found, in the first three words.** The session below
    // is the same either way; what the operator cannot see by reading it is
    // whether it has just changed under them.
    let lead = match issued.issuance {
        Issuance::Issued => "prescribing",
        Issuance::Superseded { .. } => "re-prescribing",
        Issuance::Unchanged => "unchanged for",
        Issuance::Performed { .. } => "already performed",
    };
    let weekday = workout.issued_for().weekday();
    println!(
        "{lead} {} ({weekday:?}, {})",
        workout.issued_for(),
        workout.session_role(),
    );
    println!(
        "{}, {}{}",
        shares_of(
            workout.derived_from(),
            workout.week(),
            issued.training_maximum
        ),
        workout.week(),
        issued
            .history_through
            .map(|through| format!(", history through {through}"))
            .unwrap_or_default(),
    );
    println!();
    prescribed_session(issued);

    println!();
    match &issued.issuance {
        Issuance::Issued => println!("issued as prescription {}", issued.id),
        Issuance::Superseded { previous, stranded } => {
            println!(
                "issued as prescription {}, superseding {previous}",
                issued.id
            );
            // **Since decision 0022 this is stale rather than stranded.** The
            // destination still holds the superseded session, and `deliver`
            // will replace it in place rather than landing a second one beside
            // it — so what the operator needs is the instruction, not a warning
            // about tidying up by hand.
            if let Some(reference) = stranded {
                println!(
                    "  {reference} at the destination is still the superseded session; \
                     deliver to replace it"
                );
            }
        }
        // The derivation ran and produced this. Saying so is the whole value of
        // the line: "unchanged" is a statement about the record having been
        // read, not about it having been skipped.
        Issuance::Unchanged => println!(
            "unchanged since prescription {} — the record has moved on, the session has not",
            issued.id
        ),
        Issuance::Performed { reference } => println!(
            "prescription {} was performed as {reference}, and a performed session is not \
             re-derived",
            issued.id
        ),
    }
}

/// The session itself: its blocks, its exercises and their sets, and any slot
/// that could not be derived.
///
/// **What `next` shows of a gym prescription** (#397), because it is what can
/// diverge from what reaches the destination. The lines about the derivation
/// — what it found, the shares, the prescription's number — are `prescribe`'s.
pub fn prescribed_session(issued: &application::Prescription) {
    let workout = &issued.workout;
    let mut block = None;
    for item in workout.shape().items().iter() {
        let Some(slot) = item.slots().next() else {
            continue;
        };
        if block != Some(slot.section()) {
            block = Some(slot.section());
            println!("  {}", slot.section());
        }

        let members: Vec<_> = item.exercises().collect();
        let paired = members.len() > 1;
        for (at, exercise) in members.iter().enumerate() {
            let marker = if !paired {
                "   "
            } else if at == 0 {
                " ┐ "
            } else if at + 1 == members.len() {
                " ┘ "
            } else {
                " │ "
            };
            println!("   {marker}{}", describe(exercise));
        }
    }

    if !issued.underivable.is_empty() {
        println!();
        for slot in &issued.underivable {
            println!(
                "  {} ({}) — not derivable: {}",
                slot.slot,
                slot.exercise.name(),
                slot.reason
            );
        }
    }
}

/// One exercise's line: its name, then its sets collapsed where they repeat.
///
/// Written the way an operator writes a session down — `3 × 6 @ 30kg`, sets
/// first — rather than the way the type nests.
fn describe(exercise: &domain::prescription::PrescribedExercise) -> String {
    use domain::prescription::PrescribedExercise;
    let lines: Vec<String> = match exercise {
        PrescribedExercise::ForReps { sets, .. } => sets.iter().map(set_line).collect(),
        PrescribedExercise::ForDuration { sets, .. } => sets.iter().map(set_line).collect(),
        PrescribedExercise::ForDistance { sets, .. } => sets.iter().map(set_line).collect(),
    };

    // Consecutive identical sets read as `3 × …`, which is how they are written
    // down and how they are actually performed.
    let mut collapsed: Vec<(String, usize)> = Vec::new();
    for line in lines {
        match collapsed.last_mut() {
            Some((last, count)) if *last == line => *count += 1,
            _ => collapsed.push((line, 1)),
        }
    }

    let sets = collapsed
        .into_iter()
        .map(|(line, count)| {
            if count == 1 {
                line
            } else {
                format!("{count} × {line}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    // The bands a band-assisted fill names, which the load alone does not
    // say (#454).
    let bands = match exercise {
        PrescribedExercise::ForReps {
            assisted: Some(setting),
            ..
        } => format!(" ({setting})"),
        _ => String::new(),
    };
    format!("{:<32} {sets}{bands}", exercise.exercise().name())
}

/// One set: the measure, then the load, then whatever qualifies it.
fn set_line<M: std::fmt::Display + domain::measure::Spans>(
    set: &domain::prescription::PrescribedSet<M>,
) -> String {
    use domain::prescription::Prescribed;
    let mut line = match &set.prescription {
        // An unloaded movement has no load worth printing. `Load` keeps
        // absolute-zero ("no external load" — a pogo, a stretch) apart from
        // relative-zero ("plain bodyweight" — a pull-up, where assistance and
        // added weight are both conventional), and only the second is worth a
        // word on the line.
        Prescribed::Fixed {
            load,
            measure,
            effort,
        } if unloaded(*load) => effort.as_ref().map_or_else(
            || format!("{measure}"),
            |effort| format!("{measure}, {effort} in reserve"),
        ),
        // **Effort is guidance on a fixed set, and it is printed.** Until SBS's
        // repetition-maximum day nothing issued one, so the line dropped it
        // silently; the day's whole instruction is "eight at this weight with
        // nothing left", and half of that was going missing.
        Prescribed::Fixed {
            load,
            measure,
            effort,
        } => effort.as_ref().map_or_else(
            || format!("{measure} @ {}", weight(*load)),
            |effort| format!("{measure} @ {}, {effort} in reserve", weight(*load)),
        ),
        Prescribed::ToEffort { load, effort, .. } => {
            format!("as many as @ {}, {effort} in reserve", weight(*load))
        }
    };
    if set.warmup {
        line.push_str(" (warm-up)");
    }
    line
}

/// External load that is simply absent, as against bodyweight on an axis where
/// assistance is conventional.
const fn unloaded(load: domain::gym::Load) -> bool {
    matches!(load, domain::gym::Load::Absolute(mass) if mass.is_none())
}

/// A load, written the short way.
///
/// `Load`'s own `Display` spells out "no external load" and "bodyweight +10 kg",
/// which is right for a diagnostic and too long for a line an operator reads in
/// the gym.
fn weight(load: domain::gym::Load) -> String {
    use domain::gym::Load;
    match load {
        Load::Absolute(mass) if mass.is_none() => "bodyweight".to_owned(),
        Load::Absolute(mass) => format!("{mass}kg"),
        Load::Relative(delta) if delta.as_grams() == 0 => "bodyweight".to_owned(),
        Load::Relative(delta) if delta.as_grams() < 0 => format!("bodyweight {delta}kg"),
        Load::Relative(delta) => format!("bodyweight +{delta}kg"),
    }
}

/// What a delivery amounted to.
///
/// The reference is printed because it is the join a later correspondence
/// feature reads, and because an operator who cannot find the routine needs
/// something to search for.
pub fn delivery(delivered: &application::Delivery) {
    use application::Placed;

    let lead = match delivered.placed {
        Placed::Created => "delivered to",
        Placed::Replaced { .. } => "replaced at",
        Placed::Unchanged => "already delivered to",
    };
    println!(
        "{lead} {} as session {} ({})",
        delivered.destination, delivered.ordinal, delivered.reference
    );
    // **Said out loud, because the operator's phone changed under them.** The
    // reference is unchanged, so the routine they may already have open now
    // instructs something else — and a line that reports only the identity
    // would read as though nothing had happened.
    match delivered.placed {
        Placed::Replaced {
            superseding: Some(previous),
        } => println!("  in place of prescription {previous}, which it supersedes"),
        // The same session, and so nothing superseded. It does not claim what
        // was there said instead: a delivery recorded before the digest
        // existed has no rendering on record, so "it differed" is more than
        // this knows. What it is sure of is what is there now.
        Placed::Replaced { superseding: None } => {
            println!("  the same session, as this build renders it");
        }
        Placed::Created | Placed::Unchanged => {}
    }
    unexpressed(&delivered.unexpressed);
}

/// The rendering, and nothing sent.
pub fn preview(delivered: &application::Delivery, body: &str) {
    println!(
        "would deliver session {} to {} — nothing was sent and nothing recorded",
        delivered.ordinal, delivered.destination
    );
    unexpressed(&delivered.unexpressed);
    println!();
    println!("{body}");
}

/// What the destination had no way to state.
///
/// Printed rather than swallowed, for the reason an underivable slot is: the
/// rest of the session still arrived, and the operator is the only one who can
/// decide what to do about the part that did not.
fn unexpressed(unexpressed: &[application::Unexpressed]) {
    if unexpressed.is_empty() {
        return;
    }
    println!();
    println!("what would not go:");
    for item in unexpressed {
        println!("  {} — {}", item.exercise.name(), item.reason);
    }
}

/// What one source's credential came to.
fn credential_said(outcome: &crate::setup::CredentialOutcome) -> String {
    use crate::setup::CredentialOutcome;

    match outcome {
        // **Which of the two answered, said plainly.** A stored credential
        // and an exported variable behave identically until one of them is
        // wrong, and then the difference is the whole diagnosis.
        CredentialOutcome::Stored => "connected — checked and stored".to_owned(),
        CredentialOutcome::InEnvironment => "connected — from the environment".to_owned(),
        CredentialOutcome::AlreadyStored => "connected — stored earlier".to_owned(),
        CredentialOutcome::Outstanding => "not connected".to_owned(),
        CredentialOutcome::Refused(detail) => {
            format!("not connected — the source would not have it: {detail}")
        }
    }
}

/// What `fitness credentials <source>` did.
pub fn credential(source: &str, path: &std::path::Path, outcome: &crate::setup::CredentialOutcome) {
    println!("{source:<9} {}", credential_said(outcome));
    if *outcome == crate::setup::CredentialOutcome::Stored {
        println!("          kept in {}", path.display());
    }
    if let Some(known) = crate::catalogue::source(source)
        && matches!(
            outcome,
            crate::setup::CredentialOutcome::Outstanding
                | crate::setup::CredentialOutcome::Refused(_)
        )
    {
        // Every variable this source needs, not just a key: a login has two
        // and an application three, and naming one would read as the whole
        // answer.
        println!(
            "          or set {} — credentials come from {}",
            known.required_variables().join(", "),
            known.credential_url()
        );
    }
}

/// What `init` made, and what is left to do.
///
/// **The remaining steps are the point.** A setup command that says only
/// "done" leaves an operator to discover the credential and the programme by
/// running something else and failing.
pub fn prepared(prepared: &crate::setup::Prepared) {
    use crate::setup::{CredentialOutcome, ParameterOutcome};

    println!("ready to use");
    println!("  store       {}", prepared.database.display());
    println!("  credentials {}", prepared.credentials_path.display());
    println!("  time zone   {}", prepared.zone);
    println!(
        "  numbers     {}",
        match prepared.parameters {
            ParameterOutcome::Seeded => "this build's shipped set, stored",
            ParameterOutcome::AlreadyInForce => "already set, left alone",
        }
    );
    println!();

    // **A source is something the tool can connect to, not something it needs.**
    // A programme can be authored and a session prescribed with no source at
    // all — what a credential buys is reading the performed record and
    // delivering to the phone. Listing them as obligations made one vendor look
    // mandatory.
    println!("sources:");
    for (source, outcome) in &prepared.credentials {
        println!("  {source:<9} {}", credential_said(outcome));
    }

    for (source, outcome) in &prepared.credentials {
        if matches!(
            outcome,
            CredentialOutcome::Outstanding | CredentialOutcome::Refused(_)
        ) && let Some(known) = crate::catalogue::source(source)
        {
            // Every variable this source needs, not just a key: a login has
            // two and an application three, and naming one of them would read
            // as the whole answer.
            println!(
                "            connect it later with `fitness credentials {source}`, or set {} — \
                 credentials come from {}",
                known.required_variables().join(", "),
                known.credential_url()
            );
        }
    }

    println!();
    println!("next: add a mesocycle — fitness mesocycle add");
}

// --- The operator's week ----------------------------------------------------

/// The slots of a week, as a line per weekday, each saying whose it is and
/// what it takes.
///
/// Grouped by day rather than listed flat, because "Monday evening, Wednesday
/// evening" is how the week is said and a flat list of seven is not a week. The
/// slots arrive ordered by weekday then part, so grouping is a fold rather than
/// a sort.
///
/// **The role reads as the operator says it**, not as the type prints it: a
/// slot is the harder and shorter session of its discipline's week, or the
/// easier and longer one. "higher intensity, lower volume" is what the domain
/// means by that and is a mouthful in a list of four lines.
fn week_slots(
    slots: &std::collections::BTreeMap<
        domain::schedule::TrainingSlot,
        domain::schedule::Allocation,
    >,
) {
    if slots.is_empty() {
        println!("    no room to train at all");
        return;
    }

    let mut days: Vec<(jiff::civil::Weekday, Vec<String>)> = Vec::new();
    for (slot, allocation) in slots {
        let entry = format!(
            "{} ({}, {})",
            slot.part,
            allocation.discipline,
            said(allocation.role)
        );
        match days.last_mut() {
            Some((day, parts)) if *day == slot.weekday => parts.push(entry),
            _ => days.push((slot.weekday, vec![entry])),
        }
    }

    for (day, parts) in days {
        let name = format!("{day:?}").to_lowercase();
        println!("    {name:<10}{}", parts.join(", "));
    }
}

/// A role in the words the prompt asks for it in.
const fn said(role: domain::schedule::SessionRole) -> &'static str {
    use domain::schedule::Relative;
    match (role.intensity(), role.volume()) {
        (Relative::Higher, Relative::Lower) => "harder, shorter",
        (Relative::Lower, Relative::Higher) => "easier, longer",
        (Relative::Higher, Relative::Higher) => "harder, longer",
        (Relative::Lower, Relative::Lower) => "easier, shorter",
    }
}

fn alteration_line(alteration: &domain::schedule::Alteration) {
    let last = alteration.last();
    let span = if alteration.days().get() == 1 {
        alteration.start().to_string()
    } else {
        format!("{} to {last}", alteration.start())
    };

    match alteration.reason() {
        Some(reason) => println!("  {span} — {}: {reason}", alteration.absence().kind()),
        // An illness has no reason, so there is nothing for a colon to introduce.
        None => println!("  {span} — {}", alteration.absence().kind()),
    }

    if let Some(zone) = alteration.zone() {
        println!("    in {}", zone.id());
    }
    match alteration.slots() {
        // Absent and empty are different facts, and printing them the same way
        // would undo the distinction the schema goes to trouble to keep.
        None => println!("    when you train is unchanged"),
        Some(slots) if slots.is_empty() => println!("    no room to train at all"),
        Some(slots) => week_slots(slots),
    }
}

pub fn pattern_recorded(pattern: &domain::schedule::TrainingPattern) {
    println!(
        "\nrecorded, from {} ({})",
        pattern.from(),
        pattern.zone().id()
    );
    week_slots(pattern.slots());
}

pub fn alteration_recorded(alteration: &domain::schedule::Alteration) {
    println!("\nrecorded");
    alteration_line(alteration);
}

pub fn schedule(diary: &domain::schedule::Diary) {
    if diary.patterns().is_empty() {
        println!(
            "nothing recorded — `fitness schedule add` asks when you have room \
             to train, and nothing derives it from the record"
        );
        return;
    }

    // Every week, not only the one in force: a schedule is superseded by a later
    // one existing, so the history is what makes that legible.
    for (at, week) in diary.patterns().iter().enumerate() {
        let heading = if at + 1 == diary.patterns().len() {
            "ordinarily"
        } else {
            "until superseded"
        };
        println!("{heading}, from {} ({})", week.from(), week.zone().id());
        week_slots(week.slots());
    }

    // Not "holidays": a run of days that departs from the ordinary week is
    // as often a course, a visitor or a late finish as it is a trip.
    if diary.alterations().is_empty() {
        println!("\nno alterations");
        return;
    }

    println!("\nalterations");
    for alteration in diary.alterations() {
        alteration_line(alteration);
    }
}

/// A performance read against the prescription it answers.
///
/// **The pairing is the first line and not a footnote.** A comparison paired by
/// published id is a fact the record holds; one paired by date is this tool
/// assuming the session trained that day was the one prescribed. The
/// divergences below read identically either way, so the line saying which is
/// what stops an assumption being read as a finding.
pub fn comparison(comparison: &application::compare::Comparison) {
    let weekday = comparison.prescribed_for.weekday();
    println!("prescribed for {} ({weekday:?})", comparison.prescribed_for);

    if comparison.performed_on == comparison.prescribed_for {
        println!("performed on the day");
    } else {
        let performed = comparison.performed_on.weekday();
        println!("performed on {} ({performed:?})", comparison.performed_on);
    }

    match &comparison.pairing {
        application::compare::Pairing::Published(reference) => {
            println!("matched by published id {reference}");
        }
        application::compare::Pairing::Dated => {
            println!(
                "matched by date — the record does not say which session this was, \
                 so it is assumed to be the one prescribed"
            );
        }
    }
    println!();

    if comparison.satisfied() {
        println!("the session did what it was told");
    } else {
        let count = comparison.divergences.len();
        let plural = if count == 1 { "" } else { "s" };
        println!("{count} divergence{plural}");
        for divergence in &comparison.divergences {
            println!("  {divergence}");
        }
    }

    if !comparison.gaps.is_empty() {
        println!();
        println!("what the record could not say");
        for gap in &comparison.gaps {
            println!("  {gap}");
        }
    }
}

/// Every school and public holiday from a date, in the order they fall.
///
/// All of them are passed, past ones included, because the macrocycle a
/// holiday ends starts where the one before it ended.
pub fn holidays(all: &domain::schedule::Holidays, from: jiff::civil::Date) {
    let holidays = &all.clone().since(from);
    let mut lines: Vec<(jiff::civil::Date, String)> = holidays
        .school()
        .iter()
        .map(|holiday| {
            let span = if holiday.days().get() == 1 {
                holiday.start().to_string()
            } else {
                format!("{} to {}", holiday.start(), holiday.last())
            };
            // **Unknown, not a half term**, past gov.uk's reach: a half
            // term there would be a guess.
            let kind = holidays
                .kind(holiday)
                .map_or_else(|| "unknown".to_owned(), |kind| kind.to_string());
            let weeks = holiday.weeks();
            // The transition of the macrocycle it ends, which a half term
            // does not.
            let ends = domain::macrocycle::Macrocycle::on(holiday.start(), all)
                .filter(|macrocycle| macrocycle.holiday() == *holiday)
                .map(|macrocycle| format!("  transition of {macrocycle}"))
                .unwrap_or_default();
            (
                holiday.start(),
                format!(
                    "school holiday  {span}  {kind:<9}  weeks {} to {}{ends}",
                    weeks.start(),
                    weeks.end()
                ),
            )
        })
        .chain(holidays.public().iter().map(|holiday| {
            (
                holiday.date(),
                format!("public holiday  {}  {}", holiday.date(), holiday.name()),
            )
        }))
        .collect();
    lines.sort();

    if lines.is_empty() {
        println!("no school or public holidays published from today");
    }
    for (_, line) in lines {
        println!("{line}");
    }

    // **Where the sources stop, said outright**: past it there is no data,
    // which is not the same as no holiday.
    let reach = |to: Option<jiff::civil::Date>| {
        to.map_or_else(|| "nowhere".to_owned(), |to| to.to_string())
    };
    println!(
        "\nschool holidays published to {}; public holidays to {}",
        reach(holidays.school_published_to()),
        reach(holidays.public_published_to()),
    );
}

#[cfg(test)]
mod tests {
    use super::{derived_phrase, shares_of, to_a_tenth};
    use domain::{
        measure::Kg,
        prescription::{Anchor, AnchorProvenance, DerivedFrom, WeekIndex, WeekKind},
    };
    use jiff::civil::date;

    fn ninety() -> Anchor {
        match Anchor::new(
            Kg::from_grams(90_000),
            None,
            AnchorProvenance::Asserted,
            date(2026, 7, 3),
        ) {
            Ok(anchor) => anchor,
            Err(error) => panic!("ninety kilograms is an anchor: {error}"),
        }
    }

    /// **A test week is measured against the anchor; it does not stand on it.**
    ///
    /// The operator read `anchor 90kg …, test` and asked whether the number
    /// belonged there at all. It does — the entry test's ramp aims at it — but
    /// "anchor" reads as an established maximum, which in a test week is
    /// exactly what has not been established yet.
    #[test]
    fn a_test_week_is_against_its_anchor_and_a_climbing_week_is_on_it() {
        assert_eq!(
            derived_phrase(DerivedFrom::Anchor(ninety()), WeekKind::Test),
            "against 90kg (asserted, in force from 2026-07-03)"
        );
        assert_eq!(
            derived_phrase(
                DerivedFrom::Anchor(ninety()),
                WeekKind::Climbing(WeekIndex::FIRST)
            ),
            "anchor 90kg (asserted, in force from 2026-07-03)"
        );
    }

    /// **An SBS week says what it is a share of, not what the cycle opened
    /// from** (#362). The operator was handed 82.5kg top sets under a header
    /// reading 92.5kg, and asked where the number had come from.
    #[test]
    fn a_moved_maximum_leads_and_the_anchor_stays_beside_it() {
        assert_eq!(
            shares_of(
                DerivedFrom::Anchor(ninety()),
                WeekKind::Climbing(WeekIndex::FIRST),
                Some(Kg::from_grams(95_000)),
            ),
            "maximum 95kg, advanced from anchor 90kg (asserted, in force from 2026-07-03)",
        );
    }

    /// And where nothing has moved it, the anchor is the whole answer.
    ///
    /// A second number equal to the first would be noise, and a reader would
    /// have to compare them to find that out.
    #[test]
    fn a_maximum_still_at_the_anchor_is_not_said_twice() {
        assert_eq!(
            shares_of(
                DerivedFrom::Anchor(ninety()),
                WeekKind::Climbing(WeekIndex::FIRST),
                Some(Kg::from_grams(90_000)),
            ),
            "anchor 90kg (asserted, in force from 2026-07-03)",
        );
        assert_eq!(
            shares_of(
                DerivedFrom::Anchor(ninety()),
                WeekKind::Climbing(WeekIndex::FIRST),
                None,
            ),
            "anchor 90kg (asserted, in force from 2026-07-03)",
            "and a template whose maximum does not move says nothing about one",
        );
    }

    /// A standalone test has no anchor: what it derived from is what the record
    /// put it at, and that number is named rather than dressed up as one.
    #[test]
    fn a_standalone_test_names_what_it_is_an_attempt_at() {
        assert_eq!(
            derived_phrase(DerivedFrom::Target(Kg::from_grams(95_000)), WeekKind::Test),
            "for 95kg"
        );
    }

    /// **Strength figures print to the nearest 0.1 kg** (#159), the tenth
    /// always shown and half a tenth rounding up.
    #[test]
    fn a_strength_figure_prints_to_a_tenth_of_a_kilogramme() {
        let shown = |grams| to_a_tenth(Kg::from_grams(grams));
        assert_eq!(shown(113_333), "113.3kg");
        assert_eq!(shown(87_916), "87.9kg");
        assert_eq!(shown(87_950), "88.0kg");
        assert_eq!(shown(85_000), "85.0kg");
        assert_eq!(shown(49), "0.0kg");
    }
}
