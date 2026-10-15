//! The page: one action, and the run it follows.

use maud::{DOCTYPE, Markup, html};

use crate::run::View;

/// The page, showing the run as it stands when it is opened.
///
/// **Connected to the run whether or not one is going**, so a run started
/// elsewhere — the terminal's is refused, but one started from another tab is
/// not — appears here without asking.
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
                    form hx-post="/run" hx-target="#notice" hx-swap="innerHTML" {
                        button type="submit" { "Run next" }
                    }
                    div #notice {}
                    p { a href="/heart-rate" { "Heart rate per power zone" } }
                    p { a href="/body-weight" { "Body weight" } }
                    p { a href="/estimated-1rm" { "Estimated 1RM" } }
                    p { a href="/equipment" { "Equipment" } }
                    div hx-ext="sse" sse-connect="/run/events" sse-swap="run" {
                        (view.render())
                    }
                }
            }
        }
    }
}

/// What starting a run came to: nothing to say, or why it was not started.
pub fn notice(refused: Option<&str>) -> Markup {
    html! {
        @if let Some(refused) = refused { p.failed { (refused) } }
    }
}

/// Phone width first: one column, nothing wider than the screen.
pub const STYLE: &str = "
:root { color-scheme: light dark; --muted: #6b7280; --failed: #b91c1c; }
@media (prefers-color-scheme: dark) { :root { --muted: #9ca3af; --failed: #f87171; } }
body { margin: 0; font: 16px/1.45 system-ui, sans-serif; }
main { max-width: 40rem; margin: 0 auto; padding: 16px; }
button { width: 100%; padding: 14px; font: inherit; font-weight: 600; }
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
";
