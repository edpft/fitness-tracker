//! Today, the home page: what is next, this microcycle, the curves, and
//! whether what each page reads has been collected (#666).
//!
//! **A page of cards like any other** (#830): it opens at once on each card as
//! it now stands, and a card is drawn again only when the node it reads moves.
//! Opening it still runs the whole of `next`, which issues and delivers what
//! the Next card shows; the card itself issues nothing.

use std::{collections::HashMap, sync::Arc};

use application::next::Upcoming;
use axum::{extract::RawPathParams, response::Redirect};
use configurator::{
    analyses::{Prescription, Standing},
    held::Of,
    macrocycle::Stood,
};
use maud::{DOCTYPE, Markup, html};

use crate::{
    card::Card,
    page::{self, Place},
    run::Asked,
};

/// What is next, and the session derived for it.
///
/// **It waits on the run's delivering as well as on what it reads**, so a run
/// still to deliver the session is not drawn as one that failed to.
pub struct Next;

impl Card for Next {
    const NAME: &'static str = "home-next";
    const DELIVERED: bool = true;
    type Reads = Of<Prescription>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, upcoming: Arc<Upcoming>) -> Markup {
        crate::report::next(&upcoming)
    }
}

/// This microcycle's sessions, and where each stands.
pub struct Microcycle;

impl Card for Microcycle {
    const NAME: &'static str = "home-microcycle";
    type Reads = Of<Standing>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, stood: Arc<Stood>) -> Markup {
        crate::report::microcycle(&stood.plan)
    }
}

/// The macrocycle's volume and intensity curves (#742).
pub struct Curves;

impl Card for Curves {
    const NAME: &'static str = "home-curves";
    type Reads = Of<Standing>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, stood: Arc<Stood>) -> Markup {
        crate::curves::card(&stood.plan.macrocycle, &stood.plan.curves)
    }
}

/// The page.
pub struct Page;

impl crate::card::Page for Page {
    const ROUTE: &'static str = Place::Today.path();
    /// **The home page shows no step** (#666): whether each page's data has
    /// been collected is its own card.
    const FOLLOWS: bool = false;
    type Cards = (Next, Microcycle, Curves);

    fn asked(_: &RawPathParams, _: &HashMap<String, String>) -> Result<Self, Redirect> {
        Ok(Self)
    }

    /// The operator, 2026-10-23: *"Opening the main page should trigger a
    /// collection run for all metrics."*
    fn run() -> Asked {
        Asked::Everything
    }

    /// **Whether what each page reads has been collected follows the run**,
    /// not the store: its first state is sent as the page connects.
    fn layout(&self, [next, microcycle, curves]: [Markup; 3]) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                head { (page::head(Place::Today.name(), &[])) }
                body hx-boost="true" {
                    main {
                        div.home {
                            (next)
                            (microcycle)
                            (curves)
                            div hx-ext="sse" sse-connect="/run/events" sse-swap="run" {}
                        }
                        (page::bar(Place::Today.path()))
                    }
                }
            }
        }
    }
}
