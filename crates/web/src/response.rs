//! Resting heart rate, overnight HRV and stress, read together on one page
//! (#787).
//!
//! The operator, 2026-11-12, on #769: *"RHR, OHRV, and Stress also need to be
//! read together, we want to see all of them moving as expected."* The page
//! answers whether each is where the training done expects it to be, and
//! whether it is moving towards that or away from it.
//!
//! **A card for each reading** (operator, 2026-11-20), each week against the
//! expected result: the reading as the mean of the week to each day since the
//! macrocycle began, its noise as a band, the
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
use configurator::{
    analyses::{OvernightHrvResponse, RestingHeartRateResponse, StressResponse},
    held::Of,
};
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

/// The page: why there is nothing to read, or a card for each reading.
fn laid_out(cards: [Markup; 4]) -> Markup {
    let [none, resting, hrv, stress] = cards;
    detail::page(
        PATH,
        TITLE,
        None,
        &html! { (none) (resting) (hrv) (stress) },
    )
}

macro_rules! weekly_card {
    ($(#[$meta:meta])* $card:ident, $name:literal, $node:ty) => {
        $(#[$meta])*
        pub struct $card;

        impl Card for $card {
            const NAME: &'static str = $name;
            type Reads = Of<$node>;

            fn asked(_: &HashMap<String, String>) -> Self {
                Self
            }

            fn render(&self, read: Arc<Option<Framed<Responding>>>) -> Markup {
                read.as_ref().as_ref().map_or_else(
                    || html! {},
                    |read| card(&read.read, &read.frame, read.frame.today),
                )
            }
        }
    };
}

weekly_card!(
    /// Resting heart rate each week against the expected result.
    RestingHeartRate,
    "response-resting-heart-rate",
    RestingHeartRateResponse
);
weekly_card!(
    /// Overnight HRV each week against the expected result.
    OvernightHrv,
    "response-overnight-hrv",
    OvernightHrvResponse
);
weekly_card!(
    /// Stress each week against the expected result.
    Stress,
    "response-stress",
    StressResponse
);

/// One reading's card: its weeks against the expected result, the
/// mesocycles behind; nothing where there is no response to read.
fn card(one: &Responding, frame: &Frame, today: Date) -> Markup {
    let Some(response) = &one.response else {
        return html! {};
    };
    let range = Range {
        from: frame.began,
        to: today,
    };
    let mesocycles = detail::mesocycles(&frame.mesocycles, range);
    let (label, written) = answering(one.reading);
    let chart = weeks(
        one.reading,
        label,
        written,
        response,
        &mesocycles,
        range,
        today,
    );
    html! {
        section class=(format!("card weeks {}", written.metric.class())) {
            h2 { (label) }
            (detail::drawn(&chart))
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
            "Each week against the expected result".to_owned()
        } else {
            format!("Each week against the expected result · {}", written.unit)
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
    type Cards = (NoMacrocycle, RestingHeartRate, OvernightHrv, Stress);

    fn asked(
        _path: &axum::extract::RawPathParams,
        _query: &HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self)
    }

    fn layout(&self, cards: [Markup; 4]) -> Markup {
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

    use super::{card, laid_out};
    use crate::progress::no_macrocycle;

    /// What the page's cards read, on a day.
    struct Shown {
        frame: Option<Frame>,
        responses: Vec<Responding>,
    }

    /// The page as its cards draw it from `shown`.
    fn body(shown: &Shown) -> String {
        let frame = shown.frame.as_ref();
        let card = |at: usize| {
            frame
                .zip(shown.responses.get(at))
                .map(|(frame, one)| card(one, frame, TODAY))
                .unwrap_or_default()
        };
        laid_out([no_macrocycle(frame), card(0), card(1), card(2)]).into_string()
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

    /// **A card for each reading** (operator, 2026-11-20), where Progress
    /// has one tile for the three.
    #[test]
    fn each_reading_is_its_own_card() {
        let page = body(&autumn());
        let order = [
            r#"<section class="card weeks resting-heart-rate"><h2>Resting heart rate</h2>"#,
            "Each week against the expected result · bpm",
            r#"<section class="card weeks hrv"><h2>Overnight HRV</h2>"#,
            "Each week against the expected result · ms",
            r#"<section class="card weeks stress"><h2>Stress</h2>"#,
            "Each week against the expected result",
        ];
        let mut from = 0;
        for said in order {
            let at = page[from..]
                .find(said)
                .unwrap_or_else(|| panic!("{said} after {from}: {page}"));
            from += at + said.len();
        }
        assert_eq!(page.matches(r#"<p class="marks">"#).count(), 3, "{page}");
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
