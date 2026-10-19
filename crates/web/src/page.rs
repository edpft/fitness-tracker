//! The page: the run its opening started, and what follows a run elsewhere.

use maud::{DOCTYPE, Markup, html};

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
                title { "fitness next" }
                style { (STYLE) }
                script src="/htmx.min.js" {}
                script src="/sse.min.js" {}
            }
            body {
                main {
                    p { a href="/heart-rate" { "Heart rate per power zone" } }
                    p { a href="/body-weight" { "Body weight" } " · " a href="/body-composition" { "Body composition" } }
                    p { a href="/resting-heart-rate" { "Resting heart rate" } " · " a href="/hrv" { "Overnight HRV" } }
                    p { a href="/estimated-1rm" { "Estimated 1RM" } }
                    p { a href="/programme" { "Programme" } " · " a href="/equipment" { "Equipment" } }
                    div hx-ext="sse" sse-connect="/run/events" sse-swap="run" {
                        (view.render())
                    }
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
.outcome, .estimate { color: var(--muted); }
.failed { color: var(--failed); }
section { margin-top: 1.5rem; }
table { border-collapse: collapse; }
td { padding: 0.15rem 0.75rem 0.15rem 0; vertical-align: top; }
li.paired { border-left: 2px solid var(--muted); padding-left: 0.5rem; list-style: none; }
a { overflow-wrap: anywhere; }
p.choices a, p.choices strong { margin-right: 0.75rem; }
";
