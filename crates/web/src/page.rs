//! The page: the run its opening started, and what follows a run elsewhere.

use maud::{DOCTYPE, Markup, PreEscaped, html};

use crate::run::View;

/// The page, showing the run as it stands when it is opened.
///
/// **Connected to the run whether or not one is going**, so a run started
/// elsewhere — the terminal's is refused, but one started by opening another
/// page is not — appears here without asking.
pub fn index(view: &View) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (head(Place::Today.name(), &[]))
                script src="/htmx.min.js" {}
                script src="/sse.min.js" {}
            }
            body {
                main {
                    div.home hx-ext="sse" sse-connect="/run/events" sse-swap="run" {
                        (view.render())
                    }
                    (bar(Place::Today.path()))
                }
            }
        }
    }
}

/// The places the bar takes him between (#768), in the order it shows them.
///
/// The operator, 2026-11-10 (#749): *"I like the idea of a bottom bar and
/// today, progress, plan, and history make sense."* History joins when its
/// page does (#772).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Today,
    Progress,
    Plan,
}

impl Place {
    pub const ALL: [Self; 3] = [Self::Today, Self::Progress, Self::Plan];

    pub const fn path(self) -> &'static str {
        match self {
            Self::Today => "/",
            Self::Progress => "/progress",
            Self::Plan => "/plan",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Progress => "Progress",
            Self::Plan => "Plan",
        }
    }

    /// The place `here` is in: the place itself, or the one its page is under.
    fn of(here: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|place| place.path() == here)
            .or_else(|| {
                PAGES
                    .iter()
                    .find(|page| page.path == here)
                    .map(|page| page.place)
            })
            .unwrap_or(Self::Today)
    }
}

/// The groups Progress shows its readings in (#769), in its order.
///
/// The operator, 2026-11-12: *"Progress must be measured against plan or
/// expectation, and the plan is the training in the macrocycle."* Performance
/// is what the tests and their estimates give; body weight and composition,
/// and resting heart rate, HRV and stress, are each read together; health is
/// held against guidelines or cohorts rather than the training.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Performance,
    Body,
    Response,
    Health,
}

impl Group {
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Performance => "Performance",
            Self::Body => "Body weight and composition",
            Self::Response => "Resting heart rate, HRV and stress",
            Self::Health => "Health",
        }
    }
}

/// A page, the place it is opened from, and the group a metric's page is in.
pub struct Page {
    pub path: &'static str,
    pub name: &'static str,
    pub place: Place,
    pub group: Option<Group>,
}

const fn page(path: &'static str, name: &'static str, place: Place) -> Page {
    Page {
        path,
        name,
        place,
        group: None,
    }
}

const fn metric(path: &'static str, name: &'static str, group: Group) -> Page {
    Page {
        path,
        name,
        place: Place::Progress,
        group: Some(group),
    }
}

/// Every page, in the order its place shows them.
pub const PAGES: [Page; 15] = [
    page("/", "Today", Place::Today),
    metric("/ftp", "FTP tests", Group::Performance),
    metric(
        "/heart-rate",
        "Heart rate per power zone",
        Group::Performance,
    ),
    metric("/estimated-1rm", "Estimated 1RM", Group::Performance),
    metric("/body-weight", "Body weight", Group::Body),
    metric("/body-composition", "Body composition", Group::Body),
    metric("/resting-heart-rate", "Resting heart rate", Group::Response),
    metric("/hrv", "Overnight HRV", Group::Response),
    page("/working", "Is what I'm doing working?", Place::Progress),
    metric("/sleep", "Sleep", Group::Health),
    metric("/pulse-wave-velocity", "Pulse wave velocity", Group::Health),
    page("/workouts", "Workouts", Place::Plan),
    page("/fills", "Fills", Place::Plan),
    page("/schedule", "Schedule", Place::Plan),
    page("/equipment", "Equipment", Place::Plan),
];

/// The bar fixed to the bottom of every page (#768), the place it is in
/// marked: as the page itself on the place, as where it was opened from on a
/// page under it.
pub fn bar(here: &str) -> Markup {
    let current = Place::of(here);
    html! {
        nav.places aria-label="Places" {
            @for place in Place::ALL {
                @if place.path() == here {
                    a href=(place.path()) aria-current="page" { (place.name()) }
                } @else if place == current {
                    a href=(place.path()) aria-current="true" { (place.name()) }
                } @else {
                    a href=(place.path()) { (place.name()) }
                }
            }
        }
    }
}

/// A page's name, after a ‹ back to the place it is opened from, and above
/// it the group a metric's page is in (#769).
pub fn header(here: &str, title: &str) -> Markup {
    let place = Place::of(here);
    let group = PAGES
        .iter()
        .find(|page| page.path == here)
        .and_then(|page| page.group);
    html! {
        header.detail {
            a href=(place.path()) aria-label=(place.name()) { "‹" }
            div {
                @if let Some(group) = group { p.group { (group.heading()) } }
                h1 { (title) }
            }
        }
    }
}

/// The step the run is on, above a page drawn from the store, and the page
/// redrawn from the store when the run ends.
///
/// **Outside `main`**, which is what the redraw replaces, so the connection
/// that asks for it outlives it. The redraw asks for the page's own address
/// as htmx, which is how its handler knows not to start another run.
///
/// The scripts are here rather than in the head so that a page drawn from the
/// store takes all of this in one line.
pub fn following(run: &View) -> Markup {
    html! {
        script src="/htmx.min.js" {}
        script src="/sse.min.js" {}
        div.following hx-ext="sse" sse-connect=(format!("/run/step?since={}", run.ended())) {
            div sse-swap="step,ended" { (run.step()) }
            div hx-get="" hx-trigger="sse:ended" hx-target="main" hx-select="main" hx-swap="outerHTML" {}
        }
    }
}

/// A page whose data a run is collecting, where it has never been drawn on
/// this store (#467, #605): the step the run is on, and the page once no run
/// is collecting what it reads.
///
/// The operator, 2026-10-23: *"if we're in the middle of re fetching,
/// normalising, or canonicalisaing the data for a visualisation, we should just
/// show a loading spinner or some other place holder."*
pub fn collecting(here: &str, run: &View) -> Markup {
    let title = PAGES
        .iter()
        .find(|page| page.path == here)
        .map(|page| page.name)
        .or_else(|| {
            Place::ALL
                .into_iter()
                .find(|place| place.path() == here)
                .map(Place::name)
        })
        .unwrap_or(here);
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (head(title, &[])) }
            body {
                (following(run))
                main {
                    (header(here, title))
                    p.collecting { "Collecting what this page shows…" }
                    (redrawing())
                    (bar(here))
                }
            }
        }
    }
}

/// A page whose data a run is collecting, as it was last drawn before (#467).
/// The operator, 2026-10-27: *"keep the chart"* until the run has finished
/// with it.
///
/// **The drawing as it was, asking to be drawn again**: inside its `main`,
/// which is what a redraw replaces, so the asking ends with the first drawing
/// made with nothing collecting.
pub fn kept(drawn: &Markup) -> Markup {
    PreEscaped(
        drawn
            .0
            .replacen("</main>", &format!("{}</main>", redrawing().0), 1),
    )
}

/// The page asked for again every two seconds, as well as when this server's
/// run ends, because a run the terminal started sends this server nothing.
fn redrawing() -> Markup {
    html! {
        div hx-get="" hx-trigger="every 2s" hx-target="main" hx-select="main" hx-swap="outerHTML" {}
    }
}

/// What every page's head holds: its title, and the one stylesheet (#614)
/// with any rules of the page's own that are not yet in it.
pub fn head(title: &str, own: &[&str]) -> Markup {
    html! {
        meta charset="utf-8";
        meta name="viewport" content="width=device-width, initial-scale=1";
        title { (title) }
        link rel="stylesheet" href=(STYLESHEET);
        @if !own.is_empty() {
            style { @for rules in own { (PreEscaped(rules)) } }
        }
    }
}

/// Where the one stylesheet is served.
pub const STYLESHEET: &str = "/style.css";

#[cfg(test)]
mod tests {
    use super::{PAGES, Place, bar, header};

    /// **The bar is the three places, the one a page is under marked**, and
    /// no pill (#768).
    #[test]
    fn the_bar_marks_the_place_a_page_is_opened_from() {
        let shown = bar("/body-weight").into_string();

        assert_eq!(shown.matches("<a ").count(), 3, "{shown}");
        assert!(
            shown.contains(r#"<a href="/progress" aria-current="true">Progress</a>"#),
            "{shown}"
        );
        assert_eq!(shown.matches("aria-current").count(), 1, "{shown}");
        assert!(!shown.contains("menu"), "{shown}");

        let on_plan = bar("/plan").into_string();
        assert!(
            on_plan.contains(r#"<a href="/plan" aria-current="page">Plan</a>"#),
            "{on_plan}"
        );
    }

    /// **‹ returns to the place a page is opened from**: Body weight to
    /// Progress, Schedule to Plan (#768).
    #[test]
    fn back_returns_to_the_place() {
        let weight = header("/body-weight", "Body weight").into_string();
        assert!(
            weight.contains(r#"<a href="/progress" aria-label="Progress">‹</a>"#),
            "{weight}"
        );

        let schedule = header("/schedule", "Schedule").into_string();
        assert!(
            schedule.contains(r#"<a href="/plan" aria-label="Plan">‹</a>"#),
            "{schedule}"
        );
    }

    /// **Every page the pills reached is still reachable**: Today is the home
    /// page, Plan lists its pages, and Progress opens each of its own from a
    /// tile (`progress::tests`).
    #[test]
    fn every_plan_page_is_listed_by_plan() {
        let listed = crate::plan::page(&Ok(None)).into_string();

        for page in &PAGES {
            if page.place == Place::Today {
                assert_eq!(page.path, Place::Today.path());
            } else if page.place == Place::Plan {
                assert!(
                    listed.contains(&format!(r#"<a href="{}">"#, page.path)),
                    "{} is on no place",
                    page.path
                );
            }
        }
    }
}
