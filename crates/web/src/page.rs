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
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Home" }
                style { (STYLE) }
                script src="/htmx.min.js" {}
                script src="/sse.min.js" {}
            }
            body {
                main {
                    (menu("/"))
                    div hx-ext="sse" sse-connect="/run/events" sse-swap="run" {
                        (view.render())
                    }
                }
            }
        }
    }
}

/// Every page, in the order the menu lists them: the home page, then the rest
/// as the home page used to list them.
pub const PAGES: [(&str, &str); 13] = [
    ("/", "Home"),
    ("/heart-rate", "Heart rate per power zone"),
    ("/ftp", "FTP tests"),
    ("/body-weight", "Body weight"),
    ("/body-composition", "Body composition"),
    ("/resting-heart-rate", "Resting heart rate"),
    ("/hrv", "Overnight HRV"),
    ("/pulse-wave-velocity", "Pulse wave velocity"),
    ("/sleep", "Sleep"),
    ("/estimated-1rm", "Estimated 1RM"),
    ("/workouts", "Workouts"),
    ("/fills", "Fills"),
    ("/equipment", "Equipment"),
];

/// The menu every page carries (#500), the page it is on marked.
///
/// **One list, so every page links to every other.** The operator,
/// 2026-10-19: *"the menu is only available on the main page, all other pages
/// have to go back to the menu to navigate"*. And on the page the menu used to
/// live on, *"the main page is called run next, which is confusing"*, so here
/// it is the home page.
///
/// **It wraps rather than scrolls**, so it fits a phone held upright.
pub fn menu(here: &str) -> Markup {
    html! {
        nav.menu aria-label="Pages" {
            @for (path, name) in PAGES {
                @if path == here {
                    a href=(path) aria-current="page" { (name) }
                } @else {
                    a href=(path) { (name) }
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
        .find(|(path, _)| *path == here)
        .map_or(here, |(_, name)| name);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                style { (STYLE) }
            }
            body {
                (following(run))
                main {
                    (menu(here))
                    h1 { (title) }
                    p.collecting { "Collecting what this page shows…" }
                    (redrawing())
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

/// Phone width first: one column, nothing wider than the screen.
pub const STYLE: &str = "
:root { color-scheme: light dark; --muted: #6b7280; --failed: #b91c1c; }
@media (prefers-color-scheme: dark) { :root { --muted: #9ca3af; --failed: #f87171; } }
body { margin: 0; font: 16px/1.45 system-ui, sans-serif; }
main { max-width: 40rem; margin: 0 auto; padding: 16px; }
button { width: 100%; padding: 14px; font: inherit; font-weight: 600; }
.following { padding: 0 16px; }
.following p { margin: 16px 0 0; }
ol, ul { padding-left: 1.2rem; }
li { margin: 0.25rem 0; }
.step { font-weight: 600; }
.outcome, .estimate, .collecting { color: var(--muted); }
.failed { color: var(--failed); }
section { margin-top: 1.5rem; }
table { border-collapse: collapse; }
td { padding: 0.15rem 0.75rem 0.15rem 0; vertical-align: top; }
li.paired { border-left: 2px solid var(--muted); padding-left: 0.5rem; list-style: none; }
a { overflow-wrap: anywhere; }
p.choices a, p.choices strong { margin-right: 0.75rem; }
nav.menu { display: flex; flex-wrap: wrap; gap: 0.35rem 1rem; margin: 0 0 1.25rem; }
nav.menu a[aria-current] { color: inherit; font-weight: 600; text-decoration: none; }
";

#[cfg(test)]
mod tests {
    use super::{PAGES, menu};

    /// **Every page is in the menu, the one it is on marked, and the main
    /// page is the home page** (#500).
    #[test]
    fn the_menu_lists_every_page_and_marks_the_one_it_is_on() {
        let shown = menu("/hrv").into_string();

        assert_eq!(shown.matches("<a ").count(), PAGES.len(), "{shown}");
        assert!(
            shown.contains(r#"<a href="/hrv" aria-current="page">Overnight HRV</a>"#),
            "{shown}"
        );
        assert_eq!(shown.matches("aria-current").count(), 1, "{shown}");
        assert!(shown.contains(r#"<a href="/">Home</a>"#), "{shown}");
        assert!(!shown.contains("Run next"), "{shown}");
    }
}
