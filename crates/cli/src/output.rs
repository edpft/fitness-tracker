//! What the operator sees.
//!
//! Every run reports both what it was served and what it wrote, because the
//! difference between them is the difference between finding nothing new and
//! finding nothing at all — and neither is a failure.

use application::{
    DerivationStatus, NormalisationSummary, RefusalReport, RunSummary, StreamStatus,
};
use domain::{
    landing::{LandingStream, RunOutcome, Watermark},
    normalised::{Refusal, RefusalKind},
    prescription::{
        BlockPeriodisation, GenerationParameters, GymMesocycle, Linear, Progression, WeekIndex,
        WeekPlan,
    },
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
pub fn status(standing: &StreamStatus, derivation: Option<&DerivationStatus>) {
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

    match standing.resumption_point {
        Some(mark) => println!("\nresumption point: {mark}"),
        None => println!("\nresumption point: unset — the next run collects the full history"),
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
        println!("\n{lift} — {with} of {} sessions have a figure", rows.len());
        for row in rows {
            let (maximum, from) = row.estimate.map_or_else(
                || (dash(), "no set gives an estimate".to_owned()),
                |estimate| (format!("{}kg", estimate.one_rep_max), estimate.to_string()),
            );
            let body = row
                .body_mass
                .map_or_else(|| "no weigh-in".to_owned(), |mass| format!("{mass}kg"));
            let ratio = row.relative.map_or_else(dash, |ratio| ratio.to_string());
            println!(
                "  {}  {ratio:>6}  {maximum:>10}  {body:>11}  {from}",
                row.on
            );
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
pub fn programme_authored(
    id: domain::plan::PlanId,
    authored: application::Authored,
    plan: &domain::plan::Plan,
    programme: &GymMesocycle,
    parameters: &domain::prescription::GenerationParameters,
) {
    let calendar = programme.calendar();
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
        plan.gym().map_or(0, domain::plan::Programme::count),
    );
    println!(
        "  this one ({}) — {}, {} primary, {} {} from {}",
        programme.template(),
        programme.primary_exercise(),
        programme.primary(),
        calendar.duration_weeks(),
        if calendar.duration_weeks() == 1 {
            "week"
        } else {
            "weeks"
        },
        calendar.start(),
    );
    if let Some(gating) = programme.gating_role() {
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
    authored_plan(programme, parameters);
    authored_parameters(parameters);
}

/// What this programme's template makes of its anchor.
///
/// Split out because it is three unrelated reports sharing a `match`, and
/// because what the parameters say is a separate question from what the
/// programme does with them.
fn authored_plan(
    programme: &GymMesocycle,
    parameters: &domain::prescription::GenerationParameters,
) {
    let calendar = programme.calendar();
    match programme {
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
            println!(
                "  four weeks, stated by the chart: 5×5 @ 80%, 4×3 @ 85%, \
                 3×1 @ 90%, 3×3 @ 75%"
            );
            println!(
                "  the second session of each week finds a rep max, and that \
                 result is what the next week is a share of"
            );
            println!("  week 4 ends on a single, which opens the next cycle");
        }
        GymMesocycle::Progression(Progression::Linear(_)) => {
            println!(
                "  it opens {} off the failed load of whatever measured the \
                 lift before it",
                parameters.entry_drop
            );
            println!(
                "  ladder climbs {}kg a week over {} climbing weeks, and none of \
                 them is a test",
                parameters.ladder_climb_per_week,
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

/// The parameters the programme was authored against.
///
/// **Split out because they are not about the programme.** They are the values
/// in force when it was authored, recorded with it (§ 14), and they read the
/// same whichever template was being authored.
fn authored_parameters(parameters: &domain::prescription::GenerationParameters) {
    println!(
        "  the light session's top set runs at {} of the heavy load",
        parameters.light_of_heavy
    );
    println!(
        "  opening drops {} off a failed entry test",
        parameters.entry_drop
    );
    for (implement, steps) in parameters.scales.iter() {
        println!("  {implement} loads in {steps}");
    }
    println!("  holds {}", parameters.static_hold);
}

/// Every parameter in force, and when it was settled.
///
/// **Complete rather than summarised**, which is the whole reason it exists.
/// Decision 0015's guard against a wrong shipped default was that a default is
/// never invisible — it used to be written into the programme document, and
/// with the document gone this is what keeps that promise. A report that showed
/// the interesting half would leave the warm-up ramp and the reset protocols
/// exactly as unexaminable as they were.
///
/// [`authored_parameters`] stays the short form: it runs after authoring, where
/// the question is what this programme was built against rather than what every
/// number is.
pub fn parameters_in_force(
    authored_at: jiff::Timestamp,
    parameters: &domain::prescription::GenerationParameters,
    programming: &domain::prescription::Programming,
) {
    // Seconds, not nanoseconds. What the operator wants from this line is
    // whether it predates the block he is about to author.
    println!(
        "generation parameters, in force since {}",
        authored_at.strftime("%Y-%m-%d %H:%M:%S UTC")
    );

    println!();
    println!("the ladder");
    println!("  climbs {}kg a week", parameters.ladder_climb_per_week);
    println!(
        "  a derived opening drops {} off a failed entry test",
        parameters.entry_drop
    );
    println!(
        "  the light session's top set runs at {} of the heavy load",
        parameters.light_of_heavy
    );

    println!();
    println!("everything that is not the primary");
    println!("  holds {}", parameters.static_hold);

    println!();
    println!("rest");
    for (block, rest) in [
        ("plyometric", parameters.rest.plyometric),
        ("power", parameters.rest.power),
        ("strength", parameters.rest.strength),
        ("hypertrophy", parameters.rest.hypertrophy),
        ("mobility", parameters.rest.mobility),
    ] {
        // **Absent and equal are different**, so a block that says nothing about
        // supersets says nothing here either rather than repeating its
        // between-sets rest as though it had been stated.
        match rest.after_superset {
            Some(after) => println!(
                "  {block:<12} {} between sets, {after} after a superset",
                rest.between_sets
            ),
            None => println!("  {block:<12} {} between sets", rest.between_sets),
        }
    }

    println!();
    println!("what each implement can hold");
    for (implement, steps) in parameters.scales.iter() {
        // Rendered before it is padded: a `Display` that writes straight to the
        // formatter ignores a width, and `Implement`'s does.
        println!("  {:<12} {steps}", implement.to_string());
    }

    // **Printed even though it is not authored.** These stopped being
    // parameters, and none of them can be set — but the guard against a wrong
    // shipped value was that it is never invisible, and dropping them from this
    // report would leave the warm-up ramp exactly as unexaminable as it was
    // before the report existed. The heading says which half is which.
    println!();
    println!("how this build programmes, which is fixed and not authored");
    println!(
        "  heavy top set × {}; light top set × {}",
        programming.top_set_reps.higher, programming.top_set_reps.lower,
    );
    println!(
        "  heavy back-off {} × {} at {} of top set; light {} × {} at {}",
        programming.back_off.higher.sets,
        programming.back_off.higher.reps,
        programming.back_off.higher.of_top_set,
        programming.back_off.lower.sets,
        programming.back_off.lower.reps,
        programming.back_off.lower.of_top_set,
    );
    println!(
        "  strength slots {} × {}; hypertrophy slots {} × {}",
        programming.strength.sets,
        programming.strength.reps,
        programming.hypertrophy.sets,
        programming.hypertrophy.reps,
    );
    println!(
        "  first stall: drop {}, re-climb {}kg a week",
        programming.first_reset.drop, programming.first_reset.reclimb_per_week,
    );
    println!(
        "  second stall: drop {}, re-climb {}kg a week",
        programming.second_reset.drop, programming.second_reset.reclimb_per_week,
    );

    println!();
    println!("the warm-up ramp, of the session's own top set");
    println!("  a floor: a work-up toward a higher repetition count takes more");
    for step in programming.warmup.iter() {
        println!("  {} × {}", step.of_top_set, step.reps);
    }
}

/// The programme in force, with its ladder week by week and where it stands.
///
/// **The table is the point.** A rate and a duration are two numbers; what an
/// operator needs to see is the load each week asks for, and which of those weeks
/// they are actually on — which after a miss is not the week the calendar is in.
///
/// The `of anchor` column is read back out of the load rather than driving it,
/// which is what lets the climb be seen passing 100% of the max it started from.
pub fn programme_standing(standing: &application::LadderStanding) {
    let programme = &standing.programme;
    let parameters = &standing.parameters;
    let calendar = programme.calendar();

    println!(
        "plan \"{}\" — mesocycle {} ({}) — {}, {} primary, {} {} from {}",
        standing.plan,
        standing.programme_id,
        programme.template(),
        programme.primary_exercise(),
        programme.primary(),
        calendar.duration_weeks(),
        if calendar.duration_weeks() == 1 {
            "week"
        } else {
            "training weeks"
        },
        calendar.start(),
    );
    if let Some(gating) = programme.gating_role() {
        println!("gating on the {gating} session");
    }
    match standing.history_through {
        Some(through) => println!("history through {through}"),
        // § 38: an empty record and a stale one are different, and a report that
        // printed nothing here would look the same for both.
        None => println!("history through — nothing performed yet"),
    }

    match programme {
        GymMesocycle::Test(test) => test_standing(test, standing),
        GymMesocycle::Progression(Progression::Linear(linear)) => {
            linear_standing(linear, standing, parameters);
        }
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            block_standing(block, standing.anchor, parameters);
        }
        GymMesocycle::Progression(Progression::Provided { .. }) => sbs_standing(standing.anchor),
    }
}

/// An SBS cycle: where the chart stands, and the one caveat that matters.
fn sbs_standing(anchor: Option<domain::prescription::Anchor>) {
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
            "no target: this test takes one from the programme before it, and \
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
        None => println!("  the {role} session is the test; the other is the previous programme's"),
    }
}

/// A linear programme: its ladder, week by week, and where the record puts it.
///
/// The `of anchor` column is read back out of the load rather than driving it,
/// which is what lets the climb be seen passing 100% of the max it started from.
fn linear_standing(
    linear: &Linear,
    standing: &application::LadderStanding,
    parameters: &GenerationParameters,
) {
    let Some(resolved) = standing.anchor else {
        println!("no anchor: nothing has measured this lift and no test asserts one");
        return;
    };
    println!("anchor {resolved}, as the record stands on the date asked about");
    println!(
        "opening derived: {} off the anchor's failed load",
        parameters.entry_drop
    );

    let Ok(ladder) = linear.ladder(resolved, parameters) else {
        println!("  no ladder: the block is too short to climb");
        return;
    };
    let anchor = resolved.load();
    let Ok(steps) = linear.steps(parameters) else {
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
        let light = ladder.light_top_set(index, steps, parameters.light_of_heavy);
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
    parameters: &GenerationParameters,
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
    let Some(steps) = parameters.scales.for_exercise(block.primary_exercise()) else {
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

/// The macrocycle a date is in, and its phase (#224).
///
/// **Said even when it cannot be known**: past the school calendar's reach, or
/// with the calendar unreachable, there is no data, which is not the same as
/// no term.
pub fn macrocycle(
    date: jiff::civil::Date,
    holidays: Result<&domain::schedule::Holidays, &str>,
    mesocycle: Option<domain::plan::Span>,
) {
    use domain::macrocycle::Macrocycle;

    let holidays = match holidays {
        Ok(holidays) => holidays,
        Err(reason) => {
            println!("macrocycle unknown: the school calendar could not be read ({reason})\n");
            return;
        }
    };
    let Some(macrocycle) = Macrocycle::on(date, holidays) else {
        let reach = holidays
            .school_published_to()
            .map_or_else(|| "nowhere".to_owned(), |to| to.to_string());
        println!("macrocycle unknown: the school calendar is published to {reach}\n");
        return;
    };
    let kind = holidays
        .kind(&macrocycle.holiday())
        .map_or_else(|| "holiday".to_owned(), |kind| kind.to_string());
    println!(
        "macrocycle {macrocycle}, {} phase: term {} to {}, transition {} to {} ({kind})\n",
        macrocycle.phase(date, mesocycle),
        macrocycle.start(),
        macrocycle.last_day_of_term(),
        macrocycle.transition(),
        macrocycle.last(),
    );
}

/// The concurrent mesocycle just committed, and the days it runs (#222).
pub fn committed(due: &crate::committing::Due) {
    println!("committed {}: {}\n", due.concurrent, due.span);
}

/// A concurrent mesocycle the macrocycle chose and that could not be built.
///
/// **Said out loud**, as a delivery that did not happen is: the week still
/// reports, and the next run tries again.
pub fn not_committed(due: &crate::committing::Due, why: &str) {
    println!("not committed {}, {}: {why}\n", due.concurrent, due.span);
}

/// Every week since the plan began that did not complete, and what that means
/// for this one (#177).
///
/// **Only the weeks that moved something**: a completed week is the plan
/// running as written and says nothing worth a line. Nothing is printed when
/// every week completed.
pub fn rescheduled(
    weeks: &[(jiff::civil::Date, domain::planner::MicrocycleState)],
    reruns: &[domain::planner::Rerun],
) {
    use domain::planner::MicrocycleState;

    let mut said = false;
    for (monday, state) in weeks {
        let holding = reruns
            .iter()
            .find(|rerun| rerun.monday == *monday)
            .and_then(|rerun| rerun.holding);
        match (state, holding) {
            (MicrocycleState::Incomplete, _) => {
                println!(
                    "the microcycle of Monday {monday} was incomplete: no essential session \
                     was performed, so it runs again and everything after it moves back a week"
                );
            }
            (MicrocycleState::PartiallyCompleted { completed, lost }, Some(_)) => {
                println!(
                    "the microcycle of Monday {monday} was partially completed: {completed} \
                     did its test and {lost} did not, so {lost} runs its test again while \
                     {completed} holds for a week, and everything after it moves back a week"
                );
            }
            (MicrocycleState::PartiallyCompleted { completed, lost }, None) => {
                println!(
                    "the microcycle of Monday {monday} was partially completed: {completed} \
                     performed its essential session and {lost} did not, so it runs again \
                     for both and everything after it moves back a week"
                );
            }
            (MicrocycleState::Running | MicrocycleState::Completed, _) => continue,
        }
        said = true;
    }
    if said {
        println!();
    }
}

/// Where every session of the microcycle stands, in the order the week runs.
///
/// **From the first session, not from the last slot** (#185). The operator, on
/// his first run of the installed build: *"Maybe we should always start from
/// the beginning of the microcycle."* A week reported from its opening says
/// where the week is up to; one reported from last night says only what
/// happened last night.
pub fn microcycle(sessions: &[application::microcycle::Session]) {
    let Some(first) = sessions.first() else {
        return;
    };
    println!(
        "this microcycle, from {:?} {}",
        first.slot.date.weekday(),
        first.slot.date,
    );
    println!();

    let named: Vec<(String, String, String)> = sessions
        .iter()
        .map(|session| {
            (
                format!("{} {}", session.slot.discipline, session.number),
                format!(
                    "{:?} {} {}",
                    session.slot.date.weekday(),
                    session.slot.date,
                    session.slot.slot.part,
                ),
                session.state.to_string(),
            )
        })
        .collect();
    let widest = |at: fn(&(String, String, String)) -> &String| {
        named
            .iter()
            .map(|row| at(row).chars().count())
            .max()
            .unwrap_or(0)
    };
    let name = widest(|row| &row.0);
    let when = widest(|row| &row.1);

    for (session, slot, state) in &named {
        println!("  {session:name$}   {slot:when$}   {state}");
    }
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

/// Which session is next, and when — what the schedule says before the
/// discipline's own `next` says the rest.
///
/// Named as the table above names it, so the line and the row are visibly the
/// same session rather than two descriptions of one.
pub fn next_slot(session: &application::microcycle::Session) {
    println!(
        "next: {} {}, {:?} {} {}",
        session.slot.discipline,
        session.number,
        session.slot.date.weekday(),
        session.slot.date,
        session.slot.slot.part,
    );
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
        derived_phrase(workout.derived_from(), workout.week()),
        workout.week(),
        issued
            .history_through
            .map(|through| format!(", history through {through}"))
            .unwrap_or_default(),
    );
    println!();

    let mut block = None;
    for item in workout.shape().items().iter() {
        let Some(slot) = item.slots().next() else {
            continue;
        };
        if block != Some(slot.block()) {
            block = Some(slot.block());
            println!("  {}", slot.block());
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
                slot.slot, slot.exercise, slot.reason
            );
        }
    }

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

    format!("{:<32} {sets}", exercise.exercise_key())
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
        // A test: the load is what the day allows, so there is none to print.
        Prescribed::Autoregulated { measure, effort } => {
            format!("{measure} — work up, {effort} in reserve")
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
    let lead = match (delivered.freshly_delivered, delivered.replaced) {
        (true, Some(_)) => "replaced at",
        (true, None) => "delivered to",
        (false, _) => "already delivered to",
    };
    println!(
        "{lead} {} as session {} ({})",
        delivered.destination, delivered.ordinal, delivered.reference
    );
    // **Said out loud, because the operator's phone changed under them.** The
    // reference is unchanged, so the routine they may already have open now
    // instructs something else — and a line that reports only the identity
    // would read as though nothing had happened.
    if let Some(previous) = delivered.replaced {
        println!("  in place of prescription {previous}, which it supersedes");
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
        println!("  {} — {}", item.exercise, item.reason);
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
    println!("next: add a programme — fitness programme add");
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
    use super::derived_phrase;
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
            "against 90kg (asserted, from 2026-07-03)"
        );
        assert_eq!(
            derived_phrase(
                DerivedFrom::Anchor(ninety()),
                WeekKind::Climbing(WeekIndex::FIRST)
            ),
            "anchor 90kg (asserted, from 2026-07-03)"
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
}
