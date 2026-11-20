//! Resting heart rate, overnight HRV and stress, read together on one page
//! (#787).
//!
//! The operator, 2026-11-12, on #769: *"RHR, OHRV, and Stress also need to be
//! read together, we want to see all of them moving as expected."* The page
//! answers whether each is where the training done expects it to be, and
//! whether it is moving towards that or away from it.
//!
//! **Each week against the expected result**: each reading as the mean of
//! the week to each day since the macrocycle began, its noise as a band, the
//! expected result as a dashed line, and whether it is tracking, diverging
//! from or converging on it (#874), with the mesocycles marked behind.
//!
//! **The charts here, their summaries on Progress** (operator, 2026-11-20):
//! Progress's three cards are simplifications of these charts, read from the
//! same analysis, so they are not repeated above them. This page replaced
//! `/working`, `/resting-heart-rate` and `/hrv`.
//!
//! **The mesocycles are numbered from 0** (operator, 2026-11-11): the entry
//! tests are Mesocycle 0, so *"the first non-test mesocycle would be
//! Mesocycle 1"*, which is what he calls it.

use std::{collections::HashMap, sync::Arc};

use application::working::{Answering, Frame, Framed, Responding};
use configurator::{analyses::Responses, held::Of};
use domain::analytical::{range::Range, response::Response};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    card::Card,
    detail::{self, Axis, Chart, Layer, Traced},
    page::Group,
    progress::NoMacrocycle,
    summary::{Written, answering},
};

pub const PATH: &str = "/response";

/// What the page is called: its group's heading, since it is the whole group.
pub const TITLE: &str = Group::Response.heading();

/// The page: why there is nothing to read, or each week against the
/// expected result.
fn laid_out(cards: [Markup; 2]) -> Markup {
    let [none, each_week] = cards;
    detail::page(PATH, TITLE, None, &html! { (none) (each_week) })
}

/// Each response reading week by week against the expected result.
pub struct EachWeek;

impl Card for EachWeek {
    const NAME: &'static str = "response-each-week";
    type Reads = Of<Responses>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, read: Arc<Option<Framed<Vec<Responding>>>>) -> Markup {
        read.as_ref().as_ref().map_or_else(
            || html! {},
            |read| each_week(&read.read, &read.frame, read.frame.today),
        )
    }
}

/// The second card: each response reading week by week against the expected
/// result.
fn each_week(responses: &[Responding], frame: &Frame, today: Date) -> Markup {
    let range = Range {
        from: frame.began,
        to: today,
    };
    let mesocycles = detail::mesocycles(&frame.mesocycles, range);
    html! {
        section.card.weeks {
            h2 { "Each week against the expected result" }
            @for one in responses {
                @if let Some(response) = &one.response {
                    @let (label, written) = answering(one.reading);
                    div class=(written.metric.class()) {
                        (detail::drawn(&weeks(one.reading, label, written, response, &mesocycles, range, today)))
                    }
                }
            }
            p.marks {
                span { i.read {} "the week to each day, and its noise" }
                span { i.dash {} "expected from the training done" }
            }
        }
    }
}

fn weeks(
    reading: Answering,
    label: &'static str,
    written: Written,
    response: &Response,
    mesocycles: &[(Date, Date, String)],
    range: Range,
    today: Date,
) -> Chart<'static> {
    let runs = &response.weeks;
    let traced = runs
        .iter()
        .map(|run| {
            run.iter()
                .map(|week| Traced {
                    on: week.to,
                    value: week.read,
                    margin: week.margin,
                })
                .collect()
        })
        .collect();
    let expected = runs
        .iter()
        .map(|run| {
            run.iter()
                .filter_map(|week| Some((week.to, week.expected?)))
                .collect()
        })
        .collect();
    let days = runs
        .iter()
        .flatten()
        .map(|week| {
            let read = week.margin.map_or_else(
                || written.figure(week.read),
                |margin| format!("{} ± {margin:.1}", written.figure(week.read)),
            );
            let expected = week
                .expected
                .map(|expected| format!(" · expected {}", written.figure(expected)));
            (
                week.to,
                html! {
                    span.on { "week to " (detail::day(week.to, today)) } " · " (read)
                    @if let Some(expected) = expected { (expected) }
                },
            )
        })
        .collect();
    Chart {
        id: match reading {
            Answering::RestingHeartRate => "response-resting",
            Answering::OvernightHrv => "response-hrv",
            Answering::Stress => "response-stress",
        },
        metric: written.metric,
        title: label,
        sub: if written.unit.is_empty() {
            label.to_owned()
        } else {
            format!("{label} · {}", written.unit)
        },
        range,
        axis: Axis::Figures,
        layers: vec![
            Layer::Spans(mesocycles.to_vec()),
            Layer::Trend(traced),
            Layer::Aim(expected),
            Layer::Moving(response.moving.clone()),
        ],
        days,
    }
}

/// Resting heart rate, overnight HRV and stress, for the macrocycle in force
/// (#787).
#[derive(Debug)]
pub struct Page;

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (NoMacrocycle, EachWeek);

    fn asked(
        _path: &axum::extract::RawPathParams,
        _query: &HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self)
    }

    fn layout(&self, cards: [Markup; 2]) -> Markup {
        laid_out(cards)
    }
}

#[cfg(test)]
mod tests {
    use application::working::{Answering, Frame, Responding};
    use domain::{
        analytical::{
            gap::{Moving, Stretch},
            range::Range,
            response::{Change, Response, Week},
        },
        plan::Span,
    };
    use jiff::civil::{Date, date};

    use super::{each_week, laid_out};
    use crate::progress::no_macrocycle;

    /// What the page's cards read, on a day.
    struct Shown {
        frame: Option<Frame>,
        responses: Vec<Responding>,
    }

    /// The page as its cards draw it from `shown`.
    fn body(shown: &Shown) -> String {
        let frame = shown.frame.as_ref();
        laid_out([
            no_macrocycle(frame),
            frame
                .map(|frame| each_week(&shown.responses, frame, TODAY))
                .unwrap_or_default(),
        ])
        .into_string()
    }

    const BEGAN: Date = date(2026, 9, 14);
    const TODAY: Date = date(2026, 11, 11);

    fn response(before: f64, by: f64, expected: f64) -> Response {
        Response {
            before,
            change: Some(Change {
                over: Range {
                    from: date(2026, 10, 29),
                    to: TODAY,
                },
                now: before + by,
                by,
                margin: Some(1.8),
                expected: Some(expected),
            }),
            weeks: vec![
                BEGAN
                    .series(jiff::Span::new().days(1))
                    .skip(6)
                    .take_while(|to| *to <= TODAY)
                    .map(|to| Week {
                        to,
                        read: before + by,
                        margin: Some(2.1),
                        expected: Some(before + expected),
                    })
                    .collect(),
            ],
            moving: vec![
                Stretch {
                    from: BEGAN,
                    to: date(2026, 11, 5),
                    moving: Moving::Diverging,
                },
                Stretch {
                    from: date(2026, 11, 5),
                    to: TODAY,
                    moving: Moving::Converging,
                },
            ],
        }
    }

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
            }),
            responses: vec![
                Responding {
                    reading: Answering::RestingHeartRate,
                    sent_by: None,
                    response: Some(response(51.0, 1.2, -1.5)),
                },
                Responding {
                    reading: Answering::OvernightHrv,
                    sent_by: None,
                    response: Some(response(57.7, -3.8, 5.2)),
                },
                Responding {
                    reading: Answering::Stress,
                    sent_by: None,
                    response: Some(response(33.4, 0.1, -2.6)),
                },
            ],
        }
    }

    /// **The charts, not their summaries** (operator, 2026-11-20): Progress's
    /// cards simplify these charts, so they are not repeated above them.
    #[test]
    fn it_is_the_charts_and_not_their_summaries() {
        let page = body(&autumn());
        assert!(
            page.contains("<h1>Resting heart rate, HRV and stress</h1>"),
            "{page}"
        );
        assert!(
            !page.contains(r#"<p class="group">"#),
            "the page is the whole group: {page}"
        );
        for gone in [r#"class="reading"#, r#"class="trace""#, "since the start"] {
            assert!(!page.contains(gone), "{gone}: {page}");
        }
    }

    #[test]
    fn each_response_is_drawn_week_by_week_with_the_mesocycles_behind() {
        let page = body(&autumn());

        assert_eq!(
            page.matches("<svg viewBox=\"0 0 412 230\"").count(),
            3,
            "{page}"
        );
        assert_eq!(page.matches("polyline class=\"aim\"").count(), 3, "{page}");
        for (name, drawn) in [
            (">Mesocycle 0<", 3),
            (">Mesocycle 1<", 3),
            (">Mesocycle 2<", 3),
            (">Mesocycle 3<", 0),
        ] {
            assert_eq!(page.matches(name).count(), drawn, "{name}: {page}");
        }
        assert!(page.contains("week to 20 Sep"), "{page}");
        assert!(
            page.contains("52.2 bpm ± 2.1 · expected 49.5 bpm"),
            "{page}"
        );
    }

    /// **Each says whether it is tracking expected** (#874): the latest
    /// stretch above its chart, and each stretch beneath it.
    #[test]
    fn each_response_says_whether_it_is_tracking_expected() {
        let page = body(&autumn());

        assert_eq!(
            page.matches("<b>Converging on expected</b><span>since 5 Nov</span>")
                .count(),
            3,
            "{page}"
        );
        assert_eq!(
            page.matches(r#"class="stretch diverging""#).count(),
            3,
            "{page}"
        );
        assert_eq!(
            page.matches(r#"class="stretch converging""#).count(),
            3,
            "{page}"
        );
    }

    /// **A reading with nothing to show is left out.**
    #[test]
    fn a_reading_with_nothing_to_show_is_left_out() {
        let mut shown = autumn();
        if let Some(stress) = shown.responses.get_mut(2) {
            stress.response = None;
        }
        let page = body(&shown);
        assert_eq!(
            page.matches("<svg viewBox=\"0 0 412 230\"").count(),
            2,
            "{page}"
        );
    }

    #[test]
    fn no_plan_in_force_reads_nothing() {
        let page = body(&Shown {
            frame: None,
            ..autumn()
        });
        assert!(page.contains("No plan is in force today"), "{page}");
        assert!(!page.contains(r#"class="reading"#), "{page}");
    }
}
