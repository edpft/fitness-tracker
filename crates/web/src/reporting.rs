//! Report (#873): what the operator reports himself, a page for each.
//!
//! The operator, 2026-11-19: a place *"for this, illness, and the other things
//! we plan to collect directly."*

use maud::{DOCTYPE, Markup, html};

use crate::page::{self, PAGES, Place};

/// The page: each thing to report, a link apiece.
pub fn page() -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::Report.name(), &[])) }
            body hx-boost="true" {
                main.plan {
                    h1 { (Place::Report.name()) }
                    // **Loaded, not swapped in**, as Plan's inputs are: each is
                    // a form page with scripts of its own.
                    ul.pages {
                        @for page in PAGES.iter().filter(|page| page.place == Place::Report) {
                            li { a href=(page.path) hx-boost="false" { (page.name) } }
                        }
                    }
                    (page::bar(Place::Report.path()))
                }
            }
        }
    }
}
