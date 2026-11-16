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
//! **A reading with nothing to show yet is left out**, not drawn empty.

use std::{collections::HashMap, sync::Arc};

use application::working::{self, Answering, Body, Frame, Performance, Responding, Tested};
use configurator::{
    analyses::{BodyWeight, Composition, FtpTests, HeartRateAtPower, Plans, Responses, Strength},
    clock::Today,
    held::Of,
    rescheduling,
};
use domain::{
    analytical::{
        body_weight, composition::ComponentSeries, ftp_test, heart_rate_at_power, range::Range,
        since_start::SinceStart,
    },
    planner::Discipline,
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    card::Card,
    detail::{self, Metric},
    page::{self, Group, PAGES, Place},
};

pub const PATH: &str = "/progress";

/// The page: its cards under their groups' headings, then Health.
///
/// **A heading over nothing is not shown**: a group none of whose cards drew
/// anything is hidden by the stylesheet, as a reading with nothing to show is
/// left out.
fn laid_out(cards: [Markup; 5]) -> Markup {
    let [none, cycling, gym, body, responses] = cards;
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (page::head(Place::Progress.name(), &[])) }
            body {
                main.progress {
                    h1 { (Place::Progress.name()) }
                    (none)
                    div.cards {
                        h2.group { (Group::Performance.heading()) }
                        (cycling)
                        (gym)
                    }
                    div.cards {
                        h2.group { (Group::Body.heading()) }
                        (body)
                    }
                    div.cards {
                        h2.group { (Group::Response.heading()) }
                        (responses)
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
    type Reads = (Of<Plans>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, (plans, today): (Arc<rescheduling::Plans>, Date)) -> Markup {
        no_macrocycle(Frame::on(today, plans.now()).as_ref())
    }
}

pub fn no_macrocycle(frame: Option<&Frame>) -> Markup {
    if frame.is_some() {
        return html! {};
    }
    html! { p.sub { "No plan is in force today, so there is no macrocycle to read." } }
}

/// The FTP test since the entry test, and heart rate at 140 W since the start.
pub struct Cycling;

impl Card for Cycling {
    const NAME: &'static str = "progress-cycling";
    type Reads = (Of<Plans>, Of<FtpTests>, Of<HeartRateAtPower>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, tests, rides, today): (
            Arc<rescheduling::Plans>,
            Arc<Vec<ftp_test::Test>>,
            Arc<Vec<heart_rate_at_power::Trended>>,
            Date,
        ),
    ) -> Markup {
        Frame::on(today, plans.now()).map_or_else(
            || html! {},
            |frame| {
                tile(
                    Discipline::Cycling,
                    &[cycling(&tests, &rides, &frame, today)],
                )
            },
        )
    }
}

/// The FTP test and heart rate at 140 W over the macrocycle `frame`.
pub fn cycling(
    tests: &[ftp_test::Test],
    at_power: &[heart_rate_at_power::Trended],
    frame: &Frame,
    today: Date,
) -> Performance {
    working::cycling(tests, at_power, frame.began, today)
}

/// Each lift's 1RM test since the entry test, and its estimate since the
/// start.
pub struct Gym;

impl Card for Gym {
    const NAME: &'static str = "progress-gym";
    type Reads = (Of<Plans>, Of<Strength>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, strength, today): (
            Arc<rescheduling::Plans>,
            Arc<(
                Vec<application::one_rep_max::Tests>,
                application::one_rep_max::View,
            )>,
            Date,
        ),
    ) -> Markup {
        Frame::on(today, plans.now()).map_or_else(
            || html! {},
            |frame| {
                tile(
                    Discipline::Gym,
                    &working::gym(&strength.0, &strength.1.lifts, frame.began, today),
                )
            },
        )
    }
}

/// Body weight against the plan's aims, then fat and fat-free mass.
pub struct BodyTile;

impl Card for BodyTile {
    const NAME: &'static str = "progress-body";
    type Reads = (Of<Plans>, Of<BodyWeight>, Of<Composition>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, weight, composition, today): (
            Arc<rescheduling::Plans>,
            Arc<body_weight::Series>,
            Arc<Vec<ComponentSeries>>,
            Date,
        ),
    ) -> Markup {
        let Some(frame) = Frame::on(today, plans.now()) else {
            return html! {};
        };
        let aims = application::body_weight::aims_on(today, plans.now(), plans.holidays());
        let body = working::body(&weight, &aims, &composition, frame.began, today);
        body_tile(&body, frame.began, today).unwrap_or_default()
    }
}

/// Resting heart rate, HRV and stress, each week since the start against the
/// expected result of the training done.
pub struct ResponseTile;

impl Card for ResponseTile {
    const NAME: &'static str = "progress-responses";
    type Reads = (Of<Plans>, Of<Responses>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, responses, today): (Arc<rescheduling::Plans>, Arc<Option<Vec<Responding>>>, Date),
    ) -> Markup {
        let (Some(frame), Some(responses)) = (Frame::on(today, plans.now()), responses.as_ref())
        else {
            return html! {};
        };
        responding_tile(responses, frame.began, today)
    }
}

/// Every response reading with a change to show, on one tile; nothing where
/// none has.
fn responding_tile(responses: &[Responding], began: Date, today: Date) -> Markup {
    let rows: Vec<Markup> = responses
        .iter()
        .filter_map(|one| responding(one, began, today))
        .collect();
    html! {
        @if !rows.is_empty() {
            section.card.tile { @for row in &rows { (row) } }
        }
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

/// How a reading's figures are written.
#[derive(Clone, Copy)]
pub struct Written {
    pub metric: Metric,
    pub unit: &'static str,
    /// What one of the figure's own units is in the unit shown: a gram is a
    /// thousandth of a kilogram.
    pub scale: f64,
    pub places: usize,
}

impl Written {
    pub fn figure(self, value: f64) -> String {
        let shown = format!("{:.*}", self.places, value * self.scale);
        if self.unit.is_empty() {
            shown
        } else {
            format!("{shown} {}", self.unit)
        }
    }

    pub fn signed(self, by: f64) -> String {
        let shown = self.figure(by.abs());
        if shown == self.figure(0.0) {
            shown
        } else if by < 0.0 {
            format!("−{shown}")
        } else {
            format!("+{shown}")
        }
    }
}

pub const fn tested(tested: Tested) -> Written {
    match tested {
        Tested::Ftp => Written {
            metric: Metric::Ftp,
            unit: "W",
            scale: 1.0,
            places: 0,
        },
        Tested::OneRepMax(_) => Written {
            metric: Metric::OneRepMax,
            unit: "kg",
            scale: 0.001,
            places: 1,
        },
    }
}

const KG: Written = Written {
    metric: Metric::BodyWeight,
    unit: "kg",
    scale: 1.0,
    places: 1,
};

const AT_POWER: Written = Written {
    metric: Metric::HeartRateAtPower,
    unit: "bpm",
    scale: 1.0,
    places: 1,
};

pub const fn answering(reading: Answering) -> (&'static str, Written) {
    match reading {
        Answering::RestingHeartRate => (
            "Resting heart rate",
            Written {
                metric: Metric::RestingHeartRate,
                unit: "bpm",
                scale: 1.0,
                places: 1,
            },
        ),
        Answering::OvernightHrv => (
            "Overnight HRV",
            Written {
                metric: Metric::Hrv,
                unit: "ms",
                scale: 1.0,
                places: 1,
            },
        ),
        Answering::Stress => (
            "Stress",
            Written {
                metric: Metric::Stress,
                unit: "",
                scale: 1.0,
                places: 1,
            },
        ),
    }
}

/// One reading: its name, its figure now, and what is said beneath.
pub fn row(metric: Metric, label: &str, figure: Option<String>, beneath: &Markup) -> Markup {
    html! {
        div class=(format!("reading {}", metric.class())) {
            div.name {
                i.swatch {}
                (label)
                @if let Some(figure) = figure { em { (figure) } }
            }
            (beneath)
        }
    }
}

/// A change on the bar: how far, over what, and the expected result where
/// there is one.
pub fn change(written: Written, by: f64, noise: f64, over: &str, expected: Option<f64>) -> Markup {
    html! {
        div.change.plain {
            div {
                b { (written.signed(by)) }
                span { (over) }
                @if let Some(expected) = expected {
                    span.aimed { i {} "expected " (written.signed(expected)) }
                }
            }
            (detail::gauge(by, noise, expected))
        }
    }
}

/// A reading that opens its page.
fn linked(path: &str, reading: &Markup) -> Markup {
    html! { a.row href=(path) { (reading) } }
}

/// A discipline's tile: each test, then the estimate read beside it. Nothing
/// where nothing in it has a change to show.
fn tile(discipline: Discipline, performance: &[Performance]) -> Markup {
    let rows: Vec<Markup> = performance
        .iter()
        .flat_map(|one| [test(one), estimate(one)])
        .flatten()
        .collect();
    let heading = match discipline {
        Discipline::Cycling => "Cycling",
        Discipline::Gym => "Gym",
    };
    html! {
        @if !rows.is_empty() {
            section.card.tile {
                h3 { (heading) }
                @for row in &rows { (row) }
            }
        }
    }
}

fn test(one: &Performance) -> Option<Markup> {
    let since = one.since?;
    let (_, latest) = since.latest?;
    let written = tested(one.tested);
    let (label, path) = match one.tested {
        Tested::Ftp => ("FTP test".to_owned(), crate::ftp::PATH),
        Tested::OneRepMax(lift) => (
            format!("{} 1RM test", lift.name()),
            crate::one_rep_max::PATH,
        ),
    };
    let by = f64::from(latest) - f64::from(since.entry.1);
    Some(linked(
        path,
        &row(
            written.metric,
            &label,
            Some(written.figure(f64::from(latest))),
            &change(written, by, since.noise, "since the entry test", None),
        ),
    ))
}

fn estimate(one: &Performance) -> Option<Markup> {
    let estimated = one.estimated?;
    let (label, path, written) = match one.tested {
        Tested::Ftp => (
            "Heart rate at 140 W".to_owned(),
            crate::at_power::PATH,
            AT_POWER,
        ),
        Tested::OneRepMax(lift) => (
            format!("{} estimated 1RM", lift.name()),
            crate::one_rep_max::PATH,
            tested(one.tested),
        ),
    };
    Some(linked(path, &since_start(written, &label, estimated)))
}

/// An estimate since the start, on its bar.
fn since_start(written: Written, label: &str, since: SinceStart) -> Markup {
    let beneath = since.margin.map_or_else(
        || {
            html! {
                div.change.plain {
                    div { b { (written.signed(since.by())) } span { "since the start" } }
                }
            }
        },
        |noise| change(written, since.by(), noise, "since the start", None),
    );
    row(
        written.metric,
        label,
        Some(written.figure(since.now)),
        &beneath,
    )
}

/// Body weight against the plan's aims, then fat and fat-free mass. `None`
/// where none of them has a change to show.
fn body_tile(body: &Body, began: Date, today: Date) -> Option<Markup> {
    let Body {
        weight,
        fat,
        fat_free,
    } = body;
    let weight = weight.as_ref().map(|weight| {
        let runs: Vec<Vec<Traced>> = weight
            .days
            .iter()
            .map(|run| {
                run.iter()
                    .filter_map(|day| {
                        Some(Traced {
                            on: day.trend.on,
                            read: kilograms(day.trend.mass.as_grams())?,
                            margin: day.trend.margin,
                        })
                    })
                    .collect()
            })
            .collect();
        linked(
            crate::body::PATH,
            &row(
                Metric::BodyWeight,
                "Body weight",
                Some(KG.figure(weight.change.now)),
                &html! {
                    (moved(KG, weight.change.by(), weight.expected))
                    (trace(&runs, &detail::expected(&weight.aimed, Range { from: began, to: today }), began, today))
                },
            ),
        )
    });
    let component = |metric: Metric, label: &str, since: Option<SinceStart>| {
        since.map(|since| {
            linked(
                crate::body::PATH,
                &since_start(Written { metric, ..KG }, label, since),
            )
        })
    };
    let rows: Vec<Markup> = [
        weight,
        component(Metric::Fat, "Fat", *fat),
        component(Metric::FatFree, "Fat-free", *fat_free),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!rows.is_empty()).then(|| html! { section.card.tile { @for row in &rows { (row) } } })
}

/// Grams as kilograms, where they fit.
fn kilograms(grams: u64) -> Option<f64> {
    u32::try_from(grams)
        .ok()
        .map(|grams| f64::from(grams) / 1_000.0)
}

/// The change since the start, and the expected result where there is one,
/// above a line rather than a bar.
fn moved(written: Written, by: f64, expected: Option<f64>) -> Markup {
    html! {
        div.change.plain {
            div {
                b { (written.signed(by)) }
                span { "since the start" }
                @if let Some(expected) = expected {
                    span.aimed { i {} "expected " (written.signed(expected)) }
                }
            }
        }
    }
}

/// A response reading: its change since the start, and each week since it
/// against the expected result. `None` where it has no change to show.
fn responding(one: &Responding, began: Date, today: Date) -> Option<Markup> {
    let (label, written) = answering(one.reading);
    let response = one.response.as_ref()?;
    let read = response.change?;
    let runs: Vec<Vec<Traced>> = response
        .weeks
        .iter()
        .map(|run| {
            run.iter()
                .map(|week| Traced {
                    on: week.to,
                    read: week.read,
                    margin: week.margin,
                })
                .collect()
        })
        .collect();
    let expected: Vec<Vec<(Date, f64)>> = response
        .weeks
        .iter()
        .map(|run| {
            run.iter()
                .filter_map(|week| Some((week.to, week.expected?)))
                .collect()
        })
        .collect();
    let path = match one.reading {
        Answering::RestingHeartRate => crate::resting::PATH,
        Answering::OvernightHrv => crate::hrv::PATH,
        Answering::Stress => crate::working::PATH,
    };
    Some(linked(
        path,
        &row(
            written.metric,
            label,
            Some(written.figure(read.now)),
            &html! {
                (moved(written, read.by, read.expected))
                (trace(&runs, &expected, began, today))
            },
        ),
    ))
}

/// One day of a line on a tile.
struct Traced {
    on: Date,
    read: f64,
    margin: Option<f64>,
}

const TRACE_WIDTH: f64 = 300.0;
const TRACE_HEIGHT: f64 = 64.0;

/// Each run of a reading from the start to today: its band, the expected
/// result dashed, the reading in its colour, and a dot on the latest day.
fn trace(runs: &[Vec<Traced>], expected: &[Vec<(Date, f64)>], began: Date, today: Date) -> Markup {
    let days = |on: Date| f64::from(began.until(on).map_or(0, |gap| gap.get_days()));
    let width = days(today).max(1.0);
    let x = |on: Date| (days(on) / width).mul_add(TRACE_WIDTH - 8.0, 4.0);
    let all = runs.iter().flatten();
    let (low, high) = all.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), day| {
        let margin = day.margin.unwrap_or(0.0);
        (low.min(day.read - margin), high.max(day.read + margin))
    });
    let (low, high) = expected
        .iter()
        .flatten()
        .fold((low, high), |(low, high), (_, value)| {
            (low.min(*value), high.max(*value))
        });
    if !low.is_finite() || !high.is_finite() {
        return html! {};
    }
    let spread = (high - low).max(f64::EPSILON);
    let y = |value: f64| ((high - value) / spread).mul_add(TRACE_HEIGHT - 6.0, 3.0);
    let points = |run: &[Traced], value: &dyn Fn(&Traced) -> Option<f64>| -> String {
        run.iter()
            .filter_map(|day| Some(format!("{:.1},{:.1}", x(day.on), y(value(day)?))))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let band = |run: &[Traced]| -> String {
        let above = run
            .iter()
            .filter_map(|day| Some((day.on, day.read + day.margin?)));
        let below = run
            .iter()
            .rev()
            .filter_map(|day| Some((day.on, day.read - day.margin?)));
        above
            .chain(below)
            .map(|(on, value)| format!("{:.1},{:.1}", x(on), y(value)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let last = runs.iter().flatten().next_back();
    html! {
        svg.trace viewBox=(format!("0 0 {TRACE_WIDTH} {TRACE_HEIGHT}")) aria-hidden="true" {
            @for run in runs { polygon.band points=(band(run)) {} }
            @for run in expected {
                polyline.expected points=(run.iter().map(|(on, value)| format!("{:.1},{:.1}", x(*on), y(*value))).collect::<Vec<_>>().join(" ")) {}
            }
            @for run in runs { polyline.read points=(points(run, &|day| Some(day.read))) {} }
            @if let Some(last) = last {
                circle.fell cx=(format!("{:.1}", x(last.on))) cy=(format!("{:.1}", y(last.read))) r="4" {}
            }
        }
    }
}

/// Progress, for the macrocycle in force (#769).
#[derive(Debug)]
pub struct Page;

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (NoMacrocycle, Cycling, Gym, BodyTile, ResponseTile);

    fn asked(
        _path: &axum::extract::RawPathParams,
        _query: &HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self)
    }

    fn layout(&self, cards: [Markup; 5]) -> Markup {
        laid_out(cards)
    }
}

#[cfg(test)]
mod tests {
    use application::working::{Answering, Body, Frame, Performance, Responding, Tested};
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

    use super::{body_tile, laid_out, no_macrocycle, responding_tile, tile};
    use crate::page::{PAGES, Place};

    /// What Progress's cards read, on a day.
    struct Shown {
        frame: Option<Frame>,
        performance: Vec<Performance>,
        body: Body,
        responses: Vec<Responding>,
    }

    /// The page as its cards draw it from `shown`.
    fn body(shown: &Shown) -> String {
        let frame = shown.frame.as_ref();
        let discipline = |discipline: Discipline| {
            let performance: Vec<Performance> = shown
                .performance
                .iter()
                .filter(|one| one.tested.discipline() == discipline)
                .copied()
                .collect();
            frame
                .map(|_| tile(discipline, &performance))
                .unwrap_or_default()
        };
        laid_out([
            no_macrocycle(frame),
            discipline(Discipline::Cycling),
            discipline(Discipline::Gym),
            frame
                .and_then(|frame| body_tile(&shown.body, frame.began, TODAY))
                .unwrap_or_default(),
            frame
                .map(|frame| responding_tile(&shown.responses, frame.began, TODAY))
                .unwrap_or_default(),
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
        }
    }

    /// **Autumn 2026 on 12 November**, as mock 3 drew it.
    fn autumn() -> Shown {
        Shown {
            frame: Some(Frame {
                began: BEGAN,
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
            body: Body {
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
            },
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
        shown.body = Body {
            weight: None,
            fat: None,
            fat_free: None,
        };
        shown.responses.truncate(1);
        shown.responses[0].response = None;
        let page = body(&shown);
        for gone in [
            "<h3>Cycling",
            ">Body weight<",
            ">Resting heart rate<",
            "gauge",
        ] {
            assert!(!page.contains(gone), "{gone}: {page}");
        }
        // **Each group's heading is over no card**, so the stylesheet hides it.
        for group in page.split(r#"<div class="cards">"#).skip(1) {
            let group = group.split("</div>").next().unwrap_or_default();
            assert!(!group.contains(r#"class="card"#), "{group}");
        }
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
