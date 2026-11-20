//! Progress: the macrocycle in force, as tiles (#769).
//!
//! **A tile is the headline; its page holds the detail** (operator,
//! 2026-11-12): *"Don't try and force it, the extra detail can live on the
//! page."* Each row opens its own page until a set read together has a page of
//! its own (#786, #787).
//!
//! **Progress is measured against the plan or what is expected**, the plan
//! being the training in the macrocycle. Drawn as mock 3, which he approved
//! the same day:
//!
//! - **Performance**, a tile per discipline, the test first and the estimate
//!   second, each a bar centred on no change with its noise.
//! - **Body weight and composition**, one tile: body weight's trend since the
//!   start against where the plan's aims would have taken it, fat and
//!   fat-free mass as bars.
//! - **Resting heart rate, HRV and stress**, one tile: each week since the
//!   start against the expected result of the training done, as `/working`
//!   draws them, so the movement shows as well as the gap.
//! - **Health** at the bottom, as links: sleep and pulse wave velocity are
//!   held against guidelines or cohorts, not the training.
//!
//! **A row is its reading's summary card** (#830), the one its own page opens
//! on; Progress groups them into tiles. A reading with nothing to show yet is
//! left out, not drawn empty, and a tile with no row is hidden by the
//! stylesheet.

use std::{collections::HashMap, sync::Arc};

use application::working::{Frame, Performance, Tested};
use configurator::{
    analyses::{GymPerformance, Macrocycle},
    held::Of,
};
use maud::{DOCTYPE, Markup, html};

use crate::{
    card::Card,
    page::{self, Group, PAGES, Place},
    summary::{self, ONE_REP_MAX},
};

pub const PATH: &str = "/progress";

/// The page: its cards under their groups' headings, then Health.
///
/// **A heading over nothing is not shown**: a group none of whose cards drew
/// anything is hidden by the stylesheet, as a reading with nothing to show is
/// left out.
fn laid_out(cards: [Markup; 10]) -> Markup {
    let [
        none,
        ftp,
        at_power,
        gym,
        weight,
        fat,
        fat_free,
        resting,
        hrv,
        stress,
    ] = cards;
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::Progress.name(), &[])) }
            body hx-boost="true" {
                main.progress {
                    h1 { (Place::Progress.name()) }
                    (none)
                    div.cards {
                        h2.group { (Group::Performance.heading()) }
                        section.card.tile {
                            h3 { "Cycling" }
                            (linked(crate::ftp::PATH, &ftp))
                            (linked(crate::at_power::PATH, &at_power))
                        }
                        (gym)
                    }
                    div.cards {
                        h2.group { (Group::Body.heading()) }
                        section.card.tile {
                            (linked(crate::body::PATH, &weight))
                            (linked(crate::body::PATH, &fat))
                            (linked(crate::body::PATH, &fat_free))
                        }
                    }
                    div.cards {
                        h2.group { (Group::Response.heading()) }
                        section.card.tile {
                            (linked(crate::resting::PATH, &resting))
                            (linked(crate::hrv::PATH, &hrv))
                            (linked(crate::working::PATH, &stress))
                        }
                    }
                    (health())
                    (page::bar(PATH))
                }
            }
        }
    }
}

/// That no plan is in force today, so there is no macrocycle to read; nothing
/// on a day one is.
pub struct NoMacrocycle;

impl Card for NoMacrocycle {
    const NAME: &'static str = "no-macrocycle";
    type Reads = Of<Macrocycle>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, frame: Arc<Option<Frame>>) -> Markup {
        no_macrocycle(frame.as_ref().as_ref())
    }
}

pub fn no_macrocycle(frame: Option<&Frame>) -> Markup {
    if frame.is_some() {
        return html! {};
    }
    html! { p.sub { "No plan is in force today, so there is no macrocycle to read." } }
}

/// Each lift's 1RM test since the entry test, and its estimate since the
/// start.
pub struct Gym;

impl Card for Gym {
    const NAME: &'static str = "progress-gym";
    type Reads = Of<GymPerformance>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, performance: Arc<Vec<Performance>>) -> Markup {
        gym(&performance)
    }
}

/// The pages held against guidelines or cohorts, as links.
fn health() -> Markup {
    html! {
        h2.group { (Group::Health.heading()) }
        ul.pages {
            @for page in PAGES.iter().filter(|page| page.group == Some(Group::Health)) {
                li { a href=(page.path) { (page.name) } }
            }
        }
    }
}

/// A reading that opens its page.
fn linked(path: &str, reading: &Markup) -> Markup {
    html! { a.row href=(path) { (reading) } }
}

/// The gym's tile: each lift's test, then its estimate. Nothing where nothing
/// in it has a change to show.
fn gym(performance: &[Performance]) -> Markup {
    let rows: Vec<Markup> = performance
        .iter()
        .flat_map(|one| {
            let Tested::OneRepMax(lift) = one.tested else {
                return [None, None];
            };
            [
                summary::test(ONE_REP_MAX, &format!("{} 1RM test", lift.name()), one.since),
                one.estimated.map(|since| {
                    summary::since_start(
                        ONE_REP_MAX,
                        &format!("{} estimated 1RM", lift.name()),
                        since,
                    )
                }),
            ]
        })
        .flatten()
        .map(|row| linked(crate::one_rep_max::PATH, &row))
        .collect();
    html! {
        @if !rows.is_empty() {
            section.card.tile {
                h3 { "Gym" }
                @for row in &rows { (row) }
            }
        }
    }
}

/// Progress, for the macrocycle in force (#769).
#[derive(Debug)]
pub struct Page;

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (
        NoMacrocycle,
        summary::FtpTest,
        summary::AtPower,
        Gym,
        summary::BodyWeightSince,
        summary::Fat,
        summary::FatFree,
        summary::RestingHeartRate,
        summary::OvernightHrv,
        summary::Stress,
    );

    fn asked(
        _path: &axum::extract::RawPathParams,
        _query: &HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self)
    }

    fn layout(&self, cards: [Markup; 10]) -> Markup {
        laid_out(cards)
    }
}

#[cfg(test)]
mod tests {
    use application::working::{Answering, Frame, Performance, Responding, Tested};
    use domain::planner::Discipline;
    use domain::{
        analytical::{
            body_weight::{Aimed, Trend, Weight},
            range::Range,
            response::{Change, Response, Week},
            since_entry::SinceEntry,
            since_start::SinceStart,
        },
        gym::exercise::RepsExercise,
        measure::Kg,
        plan::Span,
    };
    use jiff::civil::{Date, date};

    use super::{gym, laid_out, no_macrocycle};
    use crate::{
        detail::Metric,
        page::{PAGES, Place},
        summary::{self, AT_POWER, FTP, KG, Written},
    };

    /// What Progress's cards read, on a day.
    struct Shown {
        frame: Option<Frame>,
        performance: Vec<Performance>,
        weight: Option<Weight>,
        fat: Option<SinceStart>,
        fat_free: Option<SinceStart>,
        responses: Vec<Responding>,
    }

    /// The page as its cards draw it from `shown`.
    fn body(shown: &Shown) -> String {
        let Some(frame) = shown.frame.as_ref() else {
            return laid_out(std::array::from_fn(|card| {
                if card == 0 {
                    no_macrocycle(None)
                } else {
                    maud::html! {}
                }
            }))
            .into_string();
        };
        let cycling = shown
            .performance
            .iter()
            .find(|one| one.tested == Tested::Ftp);
        let lifts: Vec<Performance> = shown
            .performance
            .iter()
            .filter(|one| one.tested.discipline() == Discipline::Gym)
            .copied()
            .collect();
        let component = |metric, label, since: Option<SinceStart>| {
            since
                .map(|since| summary::since_start(Written { metric, ..KG }, label, since))
                .unwrap_or_default()
        };
        let response = |at: usize| {
            shown
                .responses
                .get(at)
                .and_then(|one| summary::responding(one, frame.began, TODAY))
                .unwrap_or_default()
        };
        laid_out([
            no_macrocycle(Some(frame)),
            cycling
                .and_then(|one| summary::test(FTP, "FTP test", one.since))
                .unwrap_or_default(),
            cycling
                .and_then(|one| one.estimated)
                .map(|since| summary::since_start(AT_POWER, "Heart rate at 140 W", since))
                .unwrap_or_default(),
            gym(&lifts),
            shown
                .weight
                .as_ref()
                .map(|weight| summary::weight(weight, frame.began, TODAY))
                .unwrap_or_default(),
            component(Metric::Fat, "Fat", shown.fat),
            component(Metric::FatFree, "Fat-free", shown.fat_free),
            response(0),
            response(1),
            response(2),
        ])
        .into_string()
    }

    const BEGAN: Date = date(2026, 9, 14);
    const TODAY: Date = date(2026, 11, 12);

    fn days() -> impl Iterator<Item = Date> {
        BEGAN
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on <= TODAY)
    }

    fn response(before: f64, by: f64, expected: f64) -> Response {
        Response {
            before,
            change: Some(Change {
                over: Range {
                    from: date(2026, 10, 30),
                    to: TODAY,
                },
                now: before + by,
                by,
                margin: Some(1.8),
                expected: Some(expected),
            }),
            weeks: vec![
                days()
                    .skip(6)
                    .map(|to| Week {
                        to,
                        read: before + by,
                        margin: Some(2.1),
                        expected: Some(before + expected),
                    })
                    .collect(),
            ],
            moving: Vec::new(),
        }
    }

    /// **Autumn 2026 on 12 November**, as mock 3 drew it.
    fn autumn() -> Shown {
        Shown {
            frame: Some(Frame {
                began: BEGAN,
                today: TODAY,
                mesocycles: vec![
                    Span::new(BEGAN, 2),
                    Span::new(date(2026, 9, 28), 4),
                    Span::new(date(2026, 10, 26), 4),
                ],
                last_began: None,
            }),
            performance: vec![
                Performance {
                    tested: Tested::Ftp,
                    since: Some(SinceEntry {
                        entry: (date(2026, 9, 23), 177),
                        latest: Some((date(2026, 10, 25), 182)),
                        noise: 14.2,
                    }),
                    estimated: Some(SinceStart {
                        from: 139.0,
                        now: 137.6,
                        margin: Some(5.7),
                    }),
                },
                Performance {
                    tested: Tested::OneRepMax(RepsExercise::FrontSquatBarbell),
                    since: Some(SinceEntry {
                        entry: (date(2026, 9, 25), 92_500),
                        latest: Some((date(2026, 10, 23), 95_000)),
                        noise: 8_974.0,
                    }),
                    estimated: Some(SinceStart {
                        from: 94_000.0,
                        now: 94_800.0,
                        margin: Some(8_300.0),
                    }),
                },
            ],
            weight: Some(Weight {
                days: vec![
                    days()
                        .map(|on| Aimed {
                            trend: Trend {
                                on,
                                mass: Kg::from_grams(84_600),
                                margin: Some(0.4),
                            },
                            expected: Some(84.6),
                        })
                        .collect(),
                ],
                aimed: vec![
                    days()
                        .map(|on| domain::analytical::expected::Expected {
                            on,
                            value: 84.6,
                            since: BEGAN,
                        })
                        .collect(),
                ],
                change: SinceStart {
                    from: 84.6,
                    now: 84.6,
                    margin: Some(0.57),
                },
                expected: Some(-1.8),
            }),
            fat: Some(SinceStart {
                from: 19.0,
                now: 18.2,
                margin: Some(0.5),
            }),
            fat_free: Some(SinceStart {
                from: 65.7,
                now: 66.2,
                margin: Some(0.5),
            }),
            responses: vec![
                Responding {
                    reading: Answering::RestingHeartRate,
                    sent_by: None,
                    response: Some(response(51.0, 1.4, -1.4)),
                },
                Responding {
                    reading: Answering::OvernightHrv,
                    sent_by: None,
                    response: Some(response(57.7, -4.2, 4.5)),
                },
                Responding {
                    reading: Answering::Stress,
                    sent_by: None,
                    response: Some(response(33.4, 0.4, -2.2)),
                },
            ],
        }
    }

    /// **The groups in Progress's order**, performance by discipline with the
    /// test first and the estimate second, and Health last.
    #[test]
    fn the_tiles_are_in_progress_order() {
        let page = body(&autumn());
        let order = [
            "Performance",
            "<h3>Cycling</h3>",
            "FTP test",
            "Heart rate at 140 W",
            "<h3>Gym</h3>",
            "Front Squat (Barbell) 1RM test",
            "Front Squat (Barbell) estimated 1RM",
            "Body weight and composition",
            ">Body weight<",
            ">Fat<",
            ">Fat-free<",
            "Resting heart rate, HRV and stress",
            ">Resting heart rate<",
            ">Overnight HRV<",
            ">Stress<",
            "Health",
        ];
        let mut from = 0;
        for said in order {
            let at = page[from..]
                .find(said)
                .unwrap_or_else(|| panic!("{said} after {from}: {page}"));
            from += at + said.len();
        }
    }

    #[test]
    fn each_row_says_its_change_and_opens_its_page() {
        let page = body(&autumn());
        for said in [
            r#"<a class="row" href="/ftp">"#,
            "<em>182 W</em>",
            "<b>+5 W</b><span>since the entry test</span>",
            r#"<a class="row" href="/heart-rate">"#,
            "<b>−1.4 bpm</b><span>since the start</span>",
            r#"<a class="row" href="/estimated-1rm">"#,
            "<b>+0.8 kg</b><span>since the start</span>",
            r#"<a class="row" href="/body">"#,
            "<em>84.6 kg</em>",
            "expected −1.8 kg",
            r#"<a class="row" href="/body">"#,
            "<b>−0.8 kg</b>",
            r#"<a class="row" href="/resting-heart-rate">"#,
            "<b>+1.4 bpm</b><span>since the start</span>",
            "expected −1.4 bpm",
            r#"<a class="row" href="/hrv">"#,
            r#"<a class="row" href="/working">"#,
            r#"<a href="/sleep">Sleep</a>"#,
            r#"<a href="/pulse-wave-velocity">Pulse wave velocity</a>"#,
        ] {
            assert!(page.contains(said), "{said}: {page}");
        }
        assert_eq!(page.matches(r#"class="gauge""#).count(), 6, "{page}");
        assert_eq!(page.matches(r#"class="trace""#).count(), 4, "{page}");
    }

    /// **A reading with nothing to show is left out**, and a tile with no
    /// reading left with it.
    #[test]
    fn a_reading_with_nothing_to_show_is_left_out() {
        let mut shown = autumn();
        shown.performance = vec![Performance {
            tested: Tested::Ftp,
            since: Some(SinceEntry {
                entry: (date(2026, 9, 23), 177),
                latest: None,
                noise: 14.2,
            }),
            estimated: None,
        }];
        shown.weight = None;
        shown.fat = None;
        shown.fat_free = None;
        shown.responses.truncate(1);
        shown.responses[0].response = None;
        let page = body(&shown);
        for gone in [">Body weight<", ">Resting heart rate<", "gauge", "<h3>Gym"] {
            assert!(!page.contains(gone), "{gone}: {page}");
        }
        // **No group holds a reading**, so the stylesheet hides each tile and
        // its group's heading.
        assert!(!page.contains(r#"class="reading"#), "{page}");
        assert!(page.contains("Health"), "{page}");
    }

    /// **Every page under Progress is opened from it**, now its list of links
    /// is gone (#768).
    #[test]
    fn every_page_under_progress_is_opened_from_it() {
        let page = body(&autumn());
        for one in PAGES.iter().filter(|one| one.place == Place::Progress) {
            assert!(
                page.contains(&format!(r#"href="{}""#, one.path)),
                "{} is not opened from Progress: {page}",
                one.path
            );
        }
    }

    #[test]
    fn no_plan_in_force_reads_nothing_but_health() {
        let page = body(&Shown {
            frame: None,
            ..autumn()
        });
        assert!(page.contains("No plan is in force today"), "{page}");
        assert!(page.contains(r#"<a href="/sleep">Sleep</a>"#), "{page}");
    }
}
