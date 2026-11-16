//! Is what I'm doing working?, for the macrocycle in force (#517).
//!
//! **Three cards, as the operator read them on his own record** (mock 6,
//! 2026-11-11: *"this is really good!"*).
//!
//! **Has this macrocycle improved my fitness so far?** One bar per reading,
//! centred on no change, its noise shaded and a dot where the change fell. A
//! test's is the latest against the entry test. A response reading's is the
//! two weeks to today against the two weeks before the macrocycle began, with
//! a dashed mark for the expected result of the training done.
//!
//! **Nothing marks which way is good**: he could not read a coloured half
//! meant for it, and read every bar correctly without it.
//!
//! **Each week against the expected result.** Each response reading as the
//! mean of the week to each day since the macrocycle began, its noise as a
//! band, and the expected result as a dashed line, with the mesocycles marked
//! behind.
//!
//! **The mesocycles are numbered from 0** (operator, 2026-11-11): the entry
//! tests are Mesocycle 0, so *"the first non-test mesocycle would be
//! Mesocycle 1"*, which is what he calls it.
//!
//! **Has this macrocycle improved on the last?** With one macrocycle on
//! record it says this is the first.

use std::{collections::HashMap, sync::Arc};

use application::working::{self, Answering, Frame, Performance, Responding, Tested};
use configurator::{
    analyses::{
        FtpTests, HeartRateAtPower, OvernightHrvResponse, Plans, RestingHeartRateResponse,
        Strength, StressResponse,
    },
    clock::Today,
    held::Of,
    rescheduling,
};
use domain::analytical::{ftp_test, heart_rate_at_power, range::Range, response::Week};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    card::Card,
    detail::{self, Axis, Chart, Layer, Traced, name},
    progress::NoMacrocycle,
    summary::{FTP, ONE_REP_MAX, Written, answering, change, row},
};

pub const PATH: &str = "/working";
pub const TITLE: &str = "Is what I'm doing working?";

/// The page: the three cards, or why there is nothing to read.
fn laid_out(cards: [Markup; 4]) -> Markup {
    let [none, so_far, each_week, on_the_last] = cards;
    detail::page(
        PATH,
        TITLE,
        None,
        &html! { (none) (so_far) (each_week) (on_the_last) },
    )
}

/// Has this macrocycle improved my fitness so far?
pub struct SoFar;

impl Card for SoFar {
    const NAME: &'static str = "working-so-far";
    type Reads = (
        Of<Plans>,
        Of<FtpTests>,
        Of<HeartRateAtPower>,
        Of<Strength>,
        Of<RestingHeartRateResponse>,
        Of<OvernightHrvResponse>,
        Of<StressResponse>,
        Today,
    );

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, tests, rides, strength, resting, hrv, stress, today): (
            Arc<rescheduling::Plans>,
            Arc<Vec<ftp_test::Test>>,
            Arc<Vec<heart_rate_at_power::Trended>>,
            Arc<(
                Vec<application::one_rep_max::Tests>,
                application::one_rep_max::View,
            )>,
            Arc<Option<Responding>>,
            Arc<Option<Responding>>,
            Arc<Option<Responding>>,
            Date,
        ),
    ) -> Markup {
        let Some(frame) = Frame::on(today, plans.now()) else {
            return html! {};
        };
        let performance: Vec<Performance> =
            std::iter::once(working::cycling(&tests, &rides, frame.began, today))
                .chain(working::gym(
                    &strength.0,
                    &strength.1.lifts,
                    frame.began,
                    today,
                ))
                .collect();
        let responses = responses(&[resting, hrv, stress]);
        so_far(&performance, &responses, frame.began, today)
    }
}

/// Each response reading week by week against the expected result.
pub struct EachWeek;

impl Card for EachWeek {
    const NAME: &'static str = "working-each-week";
    type Reads = (
        Of<Plans>,
        Of<RestingHeartRateResponse>,
        Of<OvernightHrvResponse>,
        Of<StressResponse>,
        Today,
    );

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(
        &self,
        (plans, resting, hrv, stress, today): (
            Arc<rescheduling::Plans>,
            Arc<Option<Responding>>,
            Arc<Option<Responding>>,
            Arc<Option<Responding>>,
            Date,
        ),
    ) -> Markup {
        let Some(frame) = Frame::on(today, plans.now()) else {
            return html! {};
        };
        each_week(&responses(&[resting, hrv, stress]), &frame, today)
    }
}

/// Each response reading there is, in order.
fn responses(read: &[Arc<Option<Responding>>]) -> Vec<Responding> {
    read.iter().filter_map(|one| one.as_ref().clone()).collect()
}

/// How a test's figures are written.
const fn tested(tested: Tested) -> Written {
    match tested {
        Tested::Ftp => FTP,
        Tested::OneRepMax(_) => ONE_REP_MAX,
    }
}

/// Has this macrocycle improved on the last?
pub struct OnTheLast;

impl Card for OnTheLast {
    const NAME: &'static str = "working-on-the-last";
    type Reads = (Of<Plans>, Today);

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, (plans, today): (Arc<rescheduling::Plans>, Date)) -> Markup {
        Frame::on(today, plans.now())
            .map_or_else(|| html! {}, |frame| on_the_last(frame.last_began, today))
    }
}

fn performance(one: &Performance) -> Markup {
    let written = tested(one.tested);
    let label = match one.tested {
        Tested::Ftp => "FTP".to_owned(),
        Tested::OneRepMax(lift) => format!("{} 1RM", lift.name()),
    };
    let Some(since) = one.since else {
        return row(
            written.metric,
            &label,
            None,
            &html! { div.none { "No test in this macrocycle yet." } },
        );
    };
    let Some((_, latest)) = since.latest else {
        return row(
            written.metric,
            &label,
            Some(written.figure(f64::from(since.entry.1))),
            &html! { div.none { "The entry test is the only test so far." } },
        );
    };
    let by = f64::from(latest) - f64::from(since.entry.1);
    row(
        written.metric,
        &label,
        Some(written.figure(f64::from(latest))),
        &change(written, by, since.noise, "since the entry test", None),
    )
}

fn responding(one: &Responding) -> Markup {
    let (label, written) = answering(one.reading);
    let Some(response) = &one.response else {
        return row(
            written.metric,
            label,
            None,
            &html! { div.none { "Too few days recorded in the two weeks before the macrocycle began." } },
        );
    };
    let Some(read) = response.change else {
        return row(
            written.metric,
            label,
            None,
            &html! { div.none { "Too few days recorded in the two weeks to today." } },
        );
    };
    let beneath = read.margin.map_or_else(
        || {
            html! {
                div.change.plain {
                    div { b { (written.signed(read.by)) } span { "since the start" } }
                }
                div.none { "Too few days to measure the noise." }
            }
        },
        |noise| change(written, read.by, noise, "since the start", read.expected),
    );
    row(
        written.metric,
        label,
        Some(written.figure(read.now)),
        &beneath,
    )
}

/// The first card: a bar per reading, and beneath them what each was read
/// over.
fn so_far(
    performance: &[Performance],
    responses: &[Responding],
    began: Date,
    today: Date,
) -> Markup {
    html! {
        section.card.working {
            h2 { "Has this macrocycle improved my fitness so far?" }
            @for one in performance { (self::performance(one)) }
            @for one in responses { (responding(one)) }
            div.asof { (read_over(performance, responses, began, today)) }
        }
    }
}

/// When each test was, the days the response readings are over, and the watch
/// that recorded them.
fn read_over(
    performance: &[Performance],
    responses: &[Responding],
    began: Date,
    today: Date,
) -> String {
    let mut said: Vec<String> = performance
        .iter()
        .filter_map(|one| {
            let since = one.since?;
            let (latest, _) = since.latest?;
            let what = match one.tested {
                Tested::Ftp => "FTP",
                Tested::OneRepMax(_) => "1RM",
            };
            Some(format!(
                "{what} tests on {} and {}",
                detail::day(since.entry.0, today),
                detail::day(latest, today)
            ))
        })
        .collect();
    let over = responses
        .iter()
        .find_map(|one| one.response.as_ref()?.change.map(|change| change.over));
    if let Some(over) = over {
        said.push(format!(
            "readings over {} to {}, against the two weeks before {}",
            detail::day(over.from, today),
            detail::day(over.to, today),
            detail::day(began, today)
        ));
        said.push("expected from the training done".to_owned());
    }
    let mut watches: Vec<String> = responses
        .iter()
        .filter_map(|one| one.sent_by.as_ref().map(name))
        .collect();
    watches.dedup();
    said.extend(watches);
    said.join(" · ")
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
                        (detail::drawn(&weeks(one.reading, label, written, &response.weeks, &mesocycles, range, today)))
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
    runs: &[Vec<Week>],
    mesocycles: &[(Date, Date, String)],
    range: Range,
    today: Date,
) -> Chart<'static> {
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
            Answering::RestingHeartRate => "working-resting",
            Answering::OvernightHrv => "working-hrv",
            Answering::Stress => "working-stress",
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
        ],
        days,
    }
}

/// The third card: this macrocycle against the one before.
fn on_the_last(last_began: Option<Date>, today: Date) -> Markup {
    html! {
        section.card.working {
            h2 { "Has this macrocycle improved on the last?" }
            @match last_began {
                None => p.sub { "This is the first macrocycle. The one after it is compared with it." },
                Some(began) => p.sub {
                    "The macrocycle before this one began on " (detail::day(began, today))
                    ". Comparing the two is not built yet."
                },
            }
        }
    }
}

/// Is what I'm doing working?, for the macrocycle in force (#517).
#[derive(Debug)]
pub struct Page;

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (NoMacrocycle, SoFar, EachWeek, OnTheLast);

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
    use application::working::{Answering, Frame, Performance, Responding, Tested};
    use domain::{
        analytical::{
            range::Range,
            response::{Change, Response, Week},
            since_entry::SinceEntry,
        },
        gym::exercise::RepsExercise,
        plan::Span,
    };
    use jiff::civil::{Date, date};

    use super::{each_week, laid_out, on_the_last, so_far};
    use crate::progress::no_macrocycle;

    /// What the page's cards read, on a day.
    struct Shown {
        frame: Option<Frame>,
        performance: Vec<Performance>,
        responses: Vec<Responding>,
    }

    /// The page as its cards draw it from `shown`.
    fn body(shown: &Shown) -> maud::Markup {
        let frame = shown.frame.as_ref();
        laid_out([
            no_macrocycle(frame),
            frame
                .map(|frame| so_far(&shown.performance, &shown.responses, frame.began, TODAY))
                .unwrap_or_default(),
            frame
                .map(|frame| each_week(&shown.responses, frame, TODAY))
                .unwrap_or_default(),
            frame
                .map(|frame| on_the_last(frame.last_began, TODAY))
                .unwrap_or_default(),
        ])
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
        }
    }

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
                    estimated: None,
                },
                Performance {
                    tested: Tested::OneRepMax(RepsExercise::FrontSquatBarbell),
                    since: Some(SinceEntry {
                        entry: (date(2026, 9, 25), 92_500),
                        latest: Some((date(2026, 10, 23), 95_000)),
                        noise: 8_974.0,
                    }),
                    estimated: None,
                },
            ],
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

    #[test]
    fn each_reading_is_a_bar_and_a_response_carries_its_expected_result() {
        let page = body(&autumn()).into_string();

        assert_eq!(page.matches(r#"class="gauge""#).count(), 5, "{page}");
        assert_eq!(page.matches(r#"<line class="aim""#).count(), 3, "{page}");
        for said in [
            "<em>182 W</em>",
            "<b>+5 W</b><span>since the entry test</span>",
            "Front Squat (Barbell) 1RM<em>95.0 kg</em>",
            "<b>+2.5 kg</b>",
            "<em>52.2 bpm</em>",
            "<b>+1.2 bpm</b><span>since the start</span>",
            "expected −1.5 bpm",
            "<b>−3.8 ms</b>",
            "expected +5.2 ms",
            "<em>33.5</em>",
            "expected −2.6",
            "FTP tests on 23 Sep and 25 Oct · 1RM tests on 25 Sep and 23 Oct · readings over 29 Oct to 11 Nov, against the two weeks before 14 Sep · expected from the training done",
        ] {
            assert!(page.contains(said), "{said}: {page}");
        }
    }

    #[test]
    fn each_response_is_drawn_week_by_week_with_the_mesocycles_behind() {
        let page = body(&autumn()).into_string();

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

    #[test]
    fn one_macrocycle_on_record_is_the_first() {
        let page = body(&autumn()).into_string();
        assert!(
            page.contains("This is the first macrocycle. The one after it is compared with it."),
            "{page}"
        );

        let mut second = autumn();
        if let Some(frame) = &mut second.frame {
            frame.last_began = Some(date(2026, 4, 13));
        }
        let page = body(&second).into_string();
        assert!(page.contains("began on 13 Apr"), "{page}");
        assert!(!page.contains("This is the first macrocycle"), "{page}");
    }

    #[test]
    fn a_reading_with_nothing_to_read_says_why() {
        let view = Shown {
            performance: vec![
                Performance {
                    tested: Tested::Ftp,
                    since: None,
                    estimated: None,
                },
                Performance {
                    tested: Tested::OneRepMax(RepsExercise::FrontSquatBarbell),
                    since: Some(SinceEntry {
                        entry: (date(2026, 9, 25), 92_500),
                        latest: None,
                        noise: 8_974.0,
                    }),
                    estimated: None,
                },
            ],
            responses: vec![Responding {
                reading: Answering::Stress,
                sent_by: None,
                response: None,
            }],
            ..autumn()
        };
        let page = body(&view).into_string();
        for said in [
            "No test in this macrocycle yet.",
            "<em>92.5 kg</em>",
            "The entry test is the only test so far.",
            "Too few days recorded in the two weeks before the macrocycle began.",
        ] {
            assert!(page.contains(said), "{said}: {page}");
        }
        assert!(!page.contains(r#"class="gauge""#), "{page}");
    }

    #[test]
    fn no_plan_in_force_reads_nothing() {
        let page = body(&Shown {
            frame: None,
            ..autumn()
        })
        .into_string();
        assert!(page.contains("No plan is in force today"), "{page}");
    }
}
