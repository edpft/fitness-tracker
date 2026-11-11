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
//! **Has this macrocycle improved on the last?** With one macrocycle on
//! record it says this is the first.

use application::working::{Answering, Macrocycle, Performance, Responding, Tested, View};
use domain::analytical::{range::Range, response::Week};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Chart, Layer, Metric, Traced, name},
    run,
};

pub const PATH: &str = "/working";
pub const TITLE: &str = "Is what I'm doing working?";

/// The page: the three cards, or why there is nothing to read.
pub fn page(view: &View, run: &run::View) -> Markup {
    detail::page(PATH, TITLE, None, run, &body(view))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::menu(PATH))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

fn body(view: &View) -> Markup {
    let Some(macrocycle) = &view.macrocycle else {
        return html! { p.sub { "No plan is in force today, so there is no macrocycle to read." } };
    };
    html! {
        (so_far(macrocycle, view.today))
        (each_week(macrocycle, view.today))
        (on_the_last(macrocycle, view.today))
    }
}

/// How a reading's figures are written.
#[derive(Clone, Copy)]
struct Written {
    metric: Metric,
    unit: &'static str,
    /// What one of the figure's own units is in the unit shown: a gram is a
    /// thousandth of a kilogram.
    scale: f64,
    places: usize,
}

impl Written {
    fn figure(self, value: f64) -> String {
        let shown = format!("{:.*}", self.places, value * self.scale);
        if self.unit.is_empty() {
            shown
        } else {
            format!("{shown} {}", self.unit)
        }
    }

    fn signed(self, by: f64) -> String {
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

const fn tested(tested: Tested) -> Written {
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

const fn answering(reading: Answering) -> (&'static str, Written) {
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

/// One row of the first card: the reading's name, its figure now, and what is
/// said beneath.
fn row(metric: Metric, label: &str, figure: Option<String>, beneath: &Markup) -> Markup {
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
fn change(written: Written, by: f64, noise: f64, over: &str, expected: Option<f64>) -> Markup {
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
fn so_far(macrocycle: &Macrocycle, today: Date) -> Markup {
    html! {
        section.card.working {
            h2 { "Has this macrocycle improved my fitness so far?" }
            @for one in &macrocycle.performance { (performance(one)) }
            @for one in &macrocycle.responses { (responding(one)) }
            div.asof { (read_over(macrocycle, today)) }
        }
    }
}

/// When each test was, the days the response readings are over, and the watch
/// that recorded them.
fn read_over(macrocycle: &Macrocycle, today: Date) -> String {
    let mut said: Vec<String> = macrocycle
        .performance
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
    let over = macrocycle
        .responses
        .iter()
        .find_map(|one| one.response.as_ref()?.change.map(|change| change.over));
    if let Some(over) = over {
        said.push(format!(
            "readings over {} to {}, against the two weeks before {}",
            detail::day(over.from, today),
            detail::day(over.to, today),
            detail::day(macrocycle.began, today)
        ));
        said.push("expected from the training done".to_owned());
    }
    let mut watches: Vec<String> = macrocycle
        .responses
        .iter()
        .filter_map(|one| one.sent_by.as_ref().map(name))
        .collect();
    watches.dedup();
    said.extend(watches);
    said.join(" · ")
}

/// The second card: each response reading week by week against the expected
/// result.
fn each_week(macrocycle: &Macrocycle, today: Date) -> Markup {
    let range = Range {
        from: macrocycle.began,
        to: today,
    };
    let mesocycles: Vec<(Date, Date, String)> = macrocycle
        .mesocycles
        .iter()
        .zip(1..)
        .filter(|(span, _)| span.start() <= today)
        .filter_map(|(span, number): (_, u32)| {
            let last = span.end().yesterday().ok()?.min(today);
            Some((span.start(), last, format!("Mesocycle {number}")))
        })
        .collect();
    html! {
        section.card.weeks {
            h2 { "Each week against the expected result" }
            @for one in &macrocycle.responses {
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
fn on_the_last(macrocycle: &Macrocycle, today: Date) -> Markup {
    html! {
        section.card.working {
            h2 { "Has this macrocycle improved on the last?" }
            @match macrocycle.last_began {
                None => p.sub { "This is the first macrocycle. The one after it is compared with it." },
                Some(began) => p.sub {
                    "The macrocycle before this one began on " (detail::day(began, today))
                    ". Comparing the two is not built yet."
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use application::working::{Answering, Macrocycle, Performance, Responding, Tested, View};
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

    use super::body;

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

    fn autumn() -> View {
        View {
            today: TODAY,
            macrocycle: Some(Macrocycle {
                began: BEGAN,
                mesocycles: vec![
                    Span::new(BEGAN, 2),
                    Span::new(date(2026, 9, 28), 4),
                    Span::new(date(2026, 10, 26), 4),
                ],
                last_began: None,
                performance: vec![
                    Performance {
                        tested: Tested::Ftp,
                        since: Some(SinceEntry {
                            entry: (date(2026, 9, 23), 177),
                            latest: Some((date(2026, 10, 25), 182)),
                            noise: 14.2,
                        }),
                    },
                    Performance {
                        tested: Tested::OneRepMax(RepsExercise::FrontSquatBarbell),
                        since: Some(SinceEntry {
                            entry: (date(2026, 9, 25), 92_500),
                            latest: Some((date(2026, 10, 23), 95_000)),
                            noise: 8_974.0,
                        }),
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
            }),
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
        assert_eq!(page.matches(">Mesocycle 3<").count(), 3, "{page}");
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
        if let Some(macrocycle) = &mut second.macrocycle {
            macrocycle.last_began = Some(date(2026, 4, 13));
        }
        let page = body(&second).into_string();
        assert!(page.contains("began on 13 Apr"), "{page}");
        assert!(!page.contains("This is the first macrocycle"), "{page}");
    }

    #[test]
    fn a_reading_with_nothing_to_read_says_why() {
        let mut view = autumn();
        if let Some(macrocycle) = &mut view.macrocycle {
            macrocycle.performance = vec![
                Performance {
                    tested: Tested::Ftp,
                    since: None,
                },
                Performance {
                    tested: Tested::OneRepMax(RepsExercise::FrontSquatBarbell),
                    since: Some(SinceEntry {
                        entry: (date(2026, 9, 25), 92_500),
                        latest: None,
                        noise: 8_974.0,
                    }),
                },
            ];
            macrocycle.responses = vec![Responding {
                reading: Answering::Stress,
                sent_by: None,
                response: None,
            }];
        }
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
        let page = body(&View {
            today: TODAY,
            macrocycle: None,
        })
        .into_string();
        assert!(page.contains("No plan is in force today"), "{page}");
    }
}
