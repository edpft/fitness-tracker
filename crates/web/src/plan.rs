//! Plan: the macrocycle as planned to the end of term (#789), then the inputs
//! to planning (#770).
//!
//! The operator, 2026-11-10 (#749), on the pages under Plan: *"these are all
//! inputs to planning but not planning themselves."* Asked whether Plan should
//! open with the plan: *"Yes."* Drawn as the mock he approved then.

use application::macrocycle::{Macrocycle, Mesocycle};
use domain::{
    cycling::CyclingProvenance,
    macrocycle::MesocycleState,
    prescription::{GymMesocycle, Progression},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::page::{self, PAGES, Place};

/// The page, with the macrocycle as planned today or the reason it could not
/// be read.
pub fn page(macrocycle: &Result<Option<Macrocycle>, String>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::Plan.name(), &[])) }
            body {
                main.plan {
                    h1 { (Place::Plan.name()) }
                    @match macrocycle {
                        Ok(Some(macrocycle)) => (card(macrocycle)),
                        Ok(None) => p.sub { "No plan is in force today." },
                        Err(error) => p.failed { "The plan could not be read: " (error) },
                    }
                    h2.group { "Inputs" }
                    ul.pages {
                        @for page in PAGES.iter().filter(|page| page.place == Place::Plan) {
                            li { a href=(page.path) { (page.name) } }
                        }
                    }
                    (page::bar(Place::Plan.path()))
                }
            }
        }
    }
}

fn card(macrocycle: &Macrocycle) -> Markup {
    html! {
        section.card.macrocycle {
            p.label { "Macrocycle" }
            p.named { (macrocycle.name) }
            @for mesocycle in &macrocycle.mesocycles { (row(mesocycle)) }
        }
    }
}

fn row(mesocycle: &Mesocycle) -> Markup {
    let start = mesocycle.span.start();
    let last = mesocycle.span.end().yesterday().unwrap_or(start);
    html! {
        div.mesocycle.now[mesocycle.state == MesocycleState::InProgress]
            .planned[mesocycle.state == MesocycleState::Planned] {
            p.when {
                "Mesocycle " (mesocycle.ordinal) " · " (day(start)) " to " (day(last))
                " · " (state(mesocycle.state))
            }
            @if let Some(gym) = &mesocycle.gym {
                p { i.dot.gym {} (gym_programme(gym)) }
            }
            @if let Some(cycling) = &mesocycle.cycling {
                p { i.dot.cycling {} (cycling_programme(cycling, recovering(mesocycle))) }
            }
        }
    }
}

const fn state(state: MesocycleState) -> &'static str {
    match state {
        MesocycleState::Planned => "planned",
        MesocycleState::InProgress => "in progress",
        MesocycleState::Completed => "completed",
    }
}

fn day(on: Date) -> String {
    on.strftime("%-d %b").to_string()
}

/// What the gym runs: the programme it is provided from, or the kind of
/// mesocycle where nobody published it.
fn gym_programme(mesocycle: &GymMesocycle) -> String {
    match mesocycle {
        GymMesocycle::Test(_) => "Test week".to_owned(),
        GymMesocycle::Recovery(recovery) => format!(
            "Recovery from {}, {}",
            recovery.programme().provider(),
            recovery.programme().name()
        ),
        GymMesocycle::Progression(Progression::Provided { from, .. }) => format!(
            "{}, {}",
            from.programme().provider(),
            from.programme().name()
        ),
        GymMesocycle::Progression(Progression::Linear(_)) => "Linear progression".to_owned(),
        GymMesocycle::Progression(Progression::BlockPeriodisation(_)) => {
            "Block periodisation".to_owned()
        }
    }
}

/// Whether it is the transition's recovery mesocycle (#336), which the gym's
/// half says: the bike's is a published programme's deload, and its
/// provenance alone reads as the programme.
const fn recovering(mesocycle: &Mesocycle) -> bool {
    matches!(mesocycle.gym, Some(GymMesocycle::Recovery(_)))
}

fn cycling_programme(provenance: &CyclingProvenance, recovering: bool) -> String {
    match provenance {
        CyclingProvenance::Provided(programme) if recovering => {
            format!("Recovery from {}", programme.name())
        }
        CyclingProvenance::Provided(programme) => programme.name().to_string(),
        CyclingProvenance::Assembled => "Holding microcycle".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use application::macrocycle::{Macrocycle, Mesocycle};
    use domain::{
        cycling::CyclingProvenance,
        macrocycle::MesocycleState,
        plan::{MesocycleOrdinal, PlanName, Span},
        provider::{ExternalProgramme, ProgrammeName, Provider},
    };
    use jiff::civil::date;

    use super::page;

    fn ridden(
        number: u32,
        start: jiff::civil::Date,
        weeks: u32,
        programme: &str,
        state: MesocycleState,
    ) -> Mesocycle {
        Mesocycle {
            ordinal: MesocycleOrdinal::new(number).expect("numbered from one"),
            span: Span::new(start, weeks),
            gym: None,
            cycling: Some(CyclingProvenance::Provided(ExternalProgramme::new(
                Provider::try_from("Peloton").expect("a provider"),
                ProgrammeName::try_from(programme).expect("a programme"),
            ))),
            state,
        }
    }

    fn autumn() -> Macrocycle {
        Macrocycle {
            name: PlanName::try_from("2026-autumn").expect("a name"),
            mesocycles: vec![
                ridden(
                    1,
                    date(2026, 9, 14),
                    1,
                    "Power Zone test",
                    MesocycleState::Completed,
                ),
                ridden(
                    2,
                    date(2026, 9, 28),
                    4,
                    "Build Your Power Zones",
                    MesocycleState::Completed,
                ),
                ridden(
                    3,
                    date(2026, 10, 26),
                    4,
                    "Peak Your Power Zones",
                    MesocycleState::InProgress,
                ),
                ridden(
                    4,
                    date(2026, 11, 23),
                    4,
                    "Peak Your Power Zones",
                    MesocycleState::Planned,
                ),
            ],
        }
    }

    /// **The card comes first, each mesocycle of the term with its days,
    /// programme and state, the running one marked and the planned ones drawn
    /// as planned**, then the inputs (#770, #789).
    #[test]
    fn plan_opens_with_the_macrocycle() {
        let shown = page(&Ok(Some(autumn()))).into_string();

        assert!(shown.contains("2026-autumn"), "{shown}");
        assert!(
            shown.contains("Mesocycle 1 · 14 Sep to 20 Sep · completed"),
            "{shown}"
        );
        assert!(
            shown.contains(
                r#"<div class="mesocycle now"><p class="when">Mesocycle 3 · 26 Oct to 22 Nov · in progress"#
            ),
            "{shown}"
        );
        assert!(
            shown.contains(
                r#"<div class="mesocycle planned"><p class="when">Mesocycle 4 · 23 Nov to 20 Dec · planned"#
            ),
            "{shown}"
        );
        assert_eq!(
            shown.matches(r#"class="mesocycle now""#).count(),
            1,
            "{shown}"
        );
        assert!(shown.contains("Build Your Power Zones"), "{shown}");

        let card = shown.find("2026-autumn").expect("the card");
        let inputs = shown.find("Inputs").expect("the inputs");
        assert!(card < inputs, "{shown}");
        for path in ["/workouts", "/fills", "/schedule", "/equipment"] {
            assert!(shown.contains(&format!(r#"<a href="{path}">"#)), "{path}");
        }
    }

    /// **No plan in force, and the inputs are still listed.**
    #[test]
    fn no_plan_in_force_still_lists_the_inputs() {
        let shown = page(&Ok(None)).into_string();

        assert!(shown.contains("No plan is in force today."), "{shown}");
        assert!(shown.contains(r#"<a href="/schedule">"#), "{shown}");
    }
}
