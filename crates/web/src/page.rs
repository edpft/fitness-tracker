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

/// A page, and the place it is opened from.
pub struct Page {
    pub path: &'static str,
    pub name: &'static str,
    pub place: Place,
}

const fn page(path: &'static str, name: &'static str, place: Place) -> Page {
    Page { path, name, place }
}

/// Every page, in the order its place lists them.
pub const PAGES: [Page; 15] = [
    page("/", "Today", Place::Today),
    page("/working", "Is what I'm doing working?", Place::Progress),
    page("/heart-rate", "Heart rate per power zone", Place::Progress),
    page("/ftp", "FTP tests", Place::Progress),
    page("/body-weight", "Body weight", Place::Progress),
    page("/body-composition", "Body composition", Place::Progress),
    page("/resting-heart-rate", "Resting heart rate", Place::Progress),
    page("/hrv", "Overnight HRV", Place::Progress),
    page(
        "/pulse-wave-velocity",
        "Pulse wave velocity",
        Place::Progress,
    ),
    page("/sleep", "Sleep", Place::Progress),
    page("/estimated-1rm", "Estimated 1RM", Place::Progress),
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

/// A page's name, after a ‹ back to the place it is opened from.
pub fn header(here: &str, title: &str) -> Markup {
    let place = Place::of(here);
    html! {
        header.detail {
            a href=(place.path()) aria-label=(place.name()) { "‹" }
            h1 { (title) }
        }
    }
}

/// A place other than Today: the pages under it, as plain links until its
/// own issue gives it more (#769, #770).
pub fn place(place: Place) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (head(place.name(), &[])) }
            body {
                main {
                    h1 { (place.name()) }
                    ul.pages {
                        @for page in PAGES.iter().filter(|page| page.place == place) {
                            li { a href=(page.path) { (page.name) } }
                        }
                    }
                    (bar(place.path()))
                }
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
        .map_or(here, |page| page.name);
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
    use super::{PAGES, Place, bar, header, place};

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
    /// page, and every other page is listed by its place.
    #[test]
    fn every_page_is_reachable_from_a_place() {
        let listed: String = [Place::Progress, Place::Plan]
            .into_iter()
            .map(|one| place(one).into_string())
            .collect();

        for page in &PAGES {
            if page.place == Place::Today {
                assert_eq!(page.path, Place::Today.path());
            } else {
                assert!(
                    listed.contains(&format!(r#"<a href="{}">"#, page.path)),
                    "{} is on no place",
                    page.path
                );
            }
        }
    }
}
