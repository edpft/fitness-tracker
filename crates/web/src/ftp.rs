//! The FTP tests, in watts and over body weight, on the detail template
//! (#620, #469).
//!
//! **The summary is the latest test in watts and per kilo**, each with its
//! change against its noise (#688): against the test before, or since the
//! start of its run where that is what decided the reading. The arrow only
//! where the change is beyond the noise; higher is better. It is the latest
//! whatever the range.
//!
//! **One chart, the unit a choice**: each test a dot, nothing joining them,
//! since a test is a measurement and no line claims anything about the days
//! between. A test with no weigh-in that day has no per-kilo dot.
//!
//! **Beneath it, the heart rate each test was ridden at** (operator,
//! 2026-11-07, #548): its maximum as a ring, the mean of its last 5 minutes
//! solid and its mean faint, nothing joining two tests, with each device's
//! tests grouped under its name, so a test can be read as ridden as deep as
//! the ones before it or not.
//!
//! **Opens on All** (operator, 2026-10-28): *"inherently, there won't be
//! very many per macrocycle"*. The range only chooses which tests are shown;
//! each is read against the whole record.

use std::collections::BTreeMap;

use domain::analytical::{
    ftp_test::{Moved, Test, TestHeartRate, noise, within},
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Cell, Chart, Layer, Metric, Reading},
    run::View,
};

pub const PATH: &str = "/ftp";
const TITLE: &str = "FTP tests";

/// The page: the summary, then the charts over the range chosen.
pub fn page(
    tests: &[Test],
    heart_rates: &[TestHeartRate],
    ranges: &Ranges,
    over: Over,
    run: &View,
) -> Markup {
    detail::page(
        PATH,
        TITLE,
        Some(over),
        run,
        &body(tests, heart_rates, ranges, over),
    )
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

/// The summary, the chart over the range chosen and the latest tests, or why
/// there is no range.
fn body(tests: &[Test], heart_rates: &[TestHeartRate], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(tests.iter().map(Test::on));
    let range = match crate::range::chosen(ranges, over, recorded, "No FTP test has been recorded.")
    {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let shown = within(tests, range);
    html! {
        (summary(tests, today))
        (detail::choice("ftp", Metric::Ftp, "FTP", &[
            ("W", chart(&shown, range, today, Unit::Watts)),
            ("W/kg", chart(&shown, range, today, Unit::PerKilo)),
        ]))
        (heart_rate(heart_rates, range, today))
        (latest(tests, today))
        (detail::foot(
            "Dots: each FTP test; per kilo over that day's weigh-in. A change is clear beyond the noise of two tests, about 8% of the earlier (a 20-minute test repeats to 2.9%, Borszcz et al. 2020), against the test before or else the first test since the last clear change. Higher is better."
        ))
    }
}

/// The latest test, in watts and per kilo, each read against its noise.
fn summary(tests: &[Test], today: Date) -> Markup {
    let Some(last) = tests.last() else {
        return html! {};
    };
    let mut cells = vec![Cell {
        metric: Metric::Ftp,
        label: "FTP",
        figure: last.ftp.watts().as_u32().to_string(),
        unit: "W",
        margin: None,
        readings: vec![reading(
            last.ftp.watts().as_u32(),
            last.watts_moved,
            &|by| format!("{} W", signed(by)),
        )],
    }];
    if let Some(ratio) = last.per_kilo {
        cells.push(Cell {
            metric: Metric::Ftp,
            label: "Per kilo",
            figure: hundredths(i64::from(ratio.as_hundredths()), ""),
            unit: "W/kg",
            margin: None,
            readings: vec![reading(ratio.as_hundredths(), last.per_kilo_moved, &|by| {
                format!("{} W/kg", hundredths(by, "+"))
            })],
        });
    }
    detail::summary(
        &cells,
        &format!("Test on {}", detail::day(last.on(), today)),
    )
}

/// A figure's change against the noise of two tests: since the start of its
/// run where that decided it, else against the test before.
fn reading(now: u32, moved: Option<Moved>, said: &dyn Fn(i64) -> String) -> Reading {
    let Some(moved) = moved else {
        return Reading::NoRule("No test before it to read it against.".to_owned());
    };
    let (by, over) = moved.since.map_or_else(
        || (moved.by, "against the test before".to_owned()),
        |since| {
            (
                since.by,
                format!("since {}", since.on.strftime("%-d %b %Y")),
            )
        },
    );
    let Ok(earlier) = u32::try_from(i64::from(now) - by) else {
        return Reading::NoRule("No test before it to read it against.".to_owned());
    };
    Reading::Change {
        by: f64::from(now) - f64::from(earlier),
        noise: noise(earlier),
        direction: moved.direction,
        said: said(by),
        over,
    }
}

#[derive(Debug, Clone, Copy)]
enum Unit {
    Watts,
    PerKilo,
}

impl Unit {
    /// The test's figure, in watts or watts a kilo.
    fn of(self, test: &Test) -> Option<f64> {
        match self {
            Self::Watts => Some(f64::from(test.ftp.watts().as_u32())),
            Self::PerKilo => test
                .per_kilo
                .map(|ratio| f64::from(ratio.as_hundredths()) / 100.0),
        }
    }
}

/// Each test a dot, in one unit; a tap names the test and how it was read.
fn chart(tests: &[Test], range: Range, today: Date, unit: Unit) -> Chart<'static> {
    let dots: Vec<(Date, f64)> = tests
        .iter()
        .filter_map(|test| Some((test.on(), unit.of(test)?)))
        .collect();
    let said: BTreeMap<Date, String> = tests
        .iter()
        .filter(|test| unit.of(test).is_some())
        .map(|test| (test.on(), described(test, unit)))
        .collect();
    let (id, sub) = match unit {
        Unit::Watts => ("ftp-watts", format!("{} · W", counted(dots.len(), "test"))),
        Unit::PerKilo => (
            "ftp-per-kilo",
            format!(
                "{} of {} weighed that day · W/kg",
                dots.len(),
                counted(tests.len(), "test")
            ),
        ),
    };
    Chart {
        id,
        metric: Metric::Ftp,
        title: "FTP",
        sub,
        range,
        axis: Axis::Figures,
        layers: vec![Layer::Dots(dots)],
        days: said
            .into_iter()
            .map(|(on, said)| {
                (
                    on,
                    html! { span.on { (detail::day(on, today)) } " · " (said) },
                )
            })
            .collect(),
    }
}

/// A test in one unit, and how it was read: `182 W · +5 W, no clear change`.
fn described(test: &Test, unit: Unit) -> String {
    match unit {
        Unit::Watts => format!(
            "{} W{}",
            test.ftp.watts().as_u32(),
            moved(test.watts_moved, |by| format!("{} W", signed(by)))
        ),
        Unit::PerKilo => format!(
            "{} W/kg{}",
            test.per_kilo.map_or_else(String::new, |ratio| hundredths(
                i64::from(ratio.as_hundredths()),
                ""
            )),
            moved(test.per_kilo_moved, |by| format!(
                "{} W/kg",
                hundredths(by, "+")
            ))
        ),
    }
}

/// ` · +5 W, no clear change`, or ` · +5 W, rising, +15 W since 22 Jul 2026`.
fn moved(moved: Option<Moved>, said: impl Fn(i64) -> String) -> String {
    moved.map_or_else(String::new, |moved| {
        let read = match moved.direction {
            Some(domain::analytical::body_weight::Direction::Rising) => "rising",
            Some(domain::analytical::body_weight::Direction::Falling) => "falling",
            Some(domain::analytical::body_weight::Direction::Maintaining) | None => {
                "no clear change"
            }
        };
        let since = moved.since.map_or_else(String::new, |since| {
            format!(
                ", {} since {}",
                said(since.by),
                since.on.strftime("%-d %b %Y")
            )
        });
        format!(" · {}, {read}{since}", said(moved.by))
    })
}

/// The heart rate each test in `range` was ridden at: max, last 5 minutes and
/// mean, each device's tests grouped under its name.
fn heart_rate(rates: &[TestHeartRate], range: Range, today: Date) -> Markup {
    if rates.is_empty() {
        return html! {};
    }
    let shown: Vec<&TestHeartRate> = rates.iter().filter(|rate| range.covers(rate.on)).collect();
    let figure = |of: fn(&TestHeartRate) -> u32| -> Vec<(Date, f64)> {
        shown
            .iter()
            .map(|rate| (rate.on, f64::from(of(rate))))
            .collect()
    };
    let mut spans: Vec<(Date, Date, String)> = Vec::new();
    for rate in &shown {
        let name = detail::name(&rate.sent_by);
        match spans.last_mut() {
            Some((_, to, last)) if *last == name => *to = rate.on,
            _ => spans.push((rate.on, rate.on, name)),
        }
    }
    let chart = Chart {
        id: "ftp-heart-rate",
        metric: Metric::TestHeartRate,
        title: "Heart rate in the test",
        sub: format!(
            "{} · bpm over the twenty minutes",
            counted(shown.len(), "test")
        ),
        range,
        axis: Axis::Figures,
        layers: vec![
            Layer::Spans(spans),
            Layer::Dots(figure(|rate| rate.mean.as_u32())),
            Layer::Solid(figure(|rate| rate.last_five_minutes.as_u32())),
            Layer::Hollow(figure(|rate| rate.max.as_u32())),
        ],
        days: shown
            .iter()
            .map(|rate| {
                (
                    rate.on,
                    html! {
                        span.on { (detail::day(rate.on, today)) }
                        " · max " (rate.max.as_u32())
                        " · last 5 min " (rate.last_five_minutes.as_u32())
                        " · mean " (rate.mean.as_u32()) " bpm · "
                        (detail::name(&rate.sent_by))
                    },
                )
            })
            .collect(),
    };
    html! {
        section class=(format!("card {}", Metric::TestHeartRate.class())) {
            h2 { (chart.title) }
            (detail::drawn(&chart))
            p.marks {
                span { i.hollow {} "Max" }
                span { i.solid {} "Last 5 min" }
                span { i.dot {} "Mean" }
            }
        }
    }
}

/// The latest seven tests, newest first, in watts and per kilo.
fn latest(tests: &[Test], today: Date) -> Markup {
    let rows: Vec<(String, Markup)> = tests
        .iter()
        .rev()
        .take(7)
        .map(|test| {
            (
                detail::day(test.on(), today),
                html! {
                    (test.ftp.watts().as_u32()) " W"
                    @if let Some(ratio) = test.per_kilo {
                        " · " (hundredths(i64::from(ratio.as_hundredths()), "")) " W/kg"
                    }
                },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Tests", "FTP · per kilo", &rows)
}

const fn sign(by: i64, plus: &'static str) -> &'static str {
    match by.signum() {
        1 => plus,
        -1 => "−",
        _ => "",
    }
}

/// Watts with their sign: `+5`, `−25`.
fn signed(by: i64) -> String {
    format!("{}{}", sign(by, "+"), by.unsigned_abs())
}

/// Hundredths as a figure to two places, signed with `plus` where positive.
fn hundredths(by: i64, plus: &'static str) -> String {
    let abs = by.unsigned_abs();
    format!("{}{}.{:02}", sign(by, plus), abs / 100, abs % 100)
}

fn counted(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            Weighed,
            ftp_test::{Test, TestHeartRate, tests},
            heart_rate::SentBy,
            range::{Over, Range, Ranges},
        },
        canonical::Occurred,
        cycling::{Ftp, FtpProvenance, Watts},
        measure::{BeatsPerMinute, Kg},
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::civil::{Date, date};

    use super::page;

    type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

    fn tested(on: Date, watts: u32) -> Fallible<(Ftp, StartedAt)> {
        let zone = OperatorZone::try_from("Europe/London".to_owned())?;
        let instant = on.at(8, 0, 0, 0).to_zoned(zone.as_time_zone())?.timestamp();
        Ok((
            Ftp::new(Watts::from_u32(watts), on, FtpProvenance::Estimated)?,
            StartedAt::new(instant, zone),
        ))
    }

    const fn weighed(on: Date, grams: u64) -> Weighed {
        Weighed {
            at: Occurred::On(on),
            mass: Kg::from_grams(grams),
            fat_free: None,
        }
    }

    fn autumn() -> Fallible<Vec<Test>> {
        Ok(tests(
            &[
                tested(date(2026, 5, 10), 155)?,
                tested(date(2026, 7, 22), 172)?,
                tested(date(2026, 9, 23), 177)?,
                tested(date(2026, 10, 25), 182)?,
            ],
            &[
                weighed(date(2026, 7, 22), 84_362),
                weighed(date(2026, 9, 23), 83_877),
                weighed(date(2026, 10, 25), 84_191),
            ],
        ))
    }

    fn ranges() -> Ranges {
        let today = date(2026, 10, 28);
        Ranges {
            mesocycle: Some(Range {
                from: date(2026, 10, 19),
                to: today,
            }),
            macrocycle: Some(Range {
                from: date(2026, 5, 4),
                to: today,
            }),
        }
    }

    fn shown(tests: &[Test], over: Over) -> String {
        page(tests, &[], &ranges(), over, &crate::run::View::default()).into_string()
    }

    #[test]
    fn each_test_is_a_dot_in_each_unit_it_has() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert_eq!(
            page.matches(r#"class="dot""#).count(),
            7,
            "four in watts, three weighed: {page}"
        );
        assert!(!page.contains(r#"class="line""#), "nothing joins two tests");
        assert!(page.contains("4 tests · W"), "{page}");
        assert!(
            page.contains("3 of 4 tests weighed that day · W/kg"),
            "{page}"
        );
    }

    #[test]
    fn the_summary_is_the_latest_test_against_its_noise() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert!(page.contains(r#"class="cell ftp""#), "{page}");
        assert!(page.contains("182<small>W</small>"), "{page}");
        assert!(page.contains("2.16<small>W/kg</small>"), "{page}");
        assert!(page.contains("+5 W"), "{page}");
        assert!(page.contains("+0.05 W/kg"), "{page}");
        assert_eq!(
            page.matches("<span>against the test before</span>").count(),
            2,
            "{page}"
        );
        assert!(
            !page.contains(r#"class="badge"#),
            "no arrow: +5 W is within 177 W's noise"
        );
        assert!(page.contains("Test on 25 Oct"), "{page}");
    }

    #[test]
    fn per_kilo_has_no_direction_after_an_unweighed_test() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert!(
            page.contains("22 Jul</span> · 2.03 W/kg<"),
            "10 May was not weighed: {page}"
        );
        assert!(
            page.contains("22 Jul</span> · 172 W · +17 W, rising<"),
            "{page}"
        );
    }

    #[test]
    fn a_range_keeps_each_test_s_reading_against_the_one_before() {
        let page = shown(&autumn().expect("the tests"), Over::Mesocycle);
        assert_eq!(page.matches(r#"class="dot""#).count(), 2, "{page}");
        assert!(
            page.contains("25 Oct</span> · 182 W · +5 W, no clear change<"),
            "{page}"
        );
    }

    #[test]
    fn a_rise_read_against_the_start_of_the_run_says_since_when() {
        let tests = tests(
            &[
                tested(date(2026, 5, 10), 155).expect("a test"),
                tested(date(2026, 7, 22), 172).expect("a test"),
                tested(date(2026, 9, 23), 177).expect("a test"),
                tested(date(2026, 10, 25), 182).expect("a test"),
                tested(date(2026, 10, 27), 187).expect("a test"),
            ],
            &[],
        );
        let page = shown(&tests, Over::Macrocycle);
        assert!(page.contains("since 22 Jul 2026"), "{page}");
        assert!(page.contains(r#"class="badge"#), "rising: {page}");
        assert!(
            page.contains("187 W · +5 W, rising, +15 W since 22 Jul 2026"),
            "{page}"
        );
    }

    fn ridden(on: Date, max: u32, last: u32, mean: u32) -> TestHeartRate {
        let bpm = |beats| BeatsPerMinute::new(beats).expect("a heart rate");
        TestHeartRate {
            on,
            max: bpm(max),
            mean: bpm(mean),
            last_five_minutes: bpm(last),
            sent_by: SentBy::Unknown,
        }
    }

    #[test]
    fn each_test_s_heart_rate_is_a_ring_a_solid_dot_and_a_faint_one() {
        let rates = [
            ridden(date(2026, 9, 23), 187, 177, 165),
            ridden(date(2026, 10, 25), 184, 177, 165),
        ];
        let page = page(
            &autumn().expect("the tests"),
            &rates,
            &ranges(),
            Over::Macrocycle,
            &crate::run::View::default(),
        )
        .into_string();
        assert!(page.contains("Heart rate in the test"), "{page}");
        assert!(page.contains(r#"class="card test-heart-rate""#), "{page}");
        assert_eq!(
            page.matches(r#"<circle class="hollow""#).count(),
            2,
            "{page}"
        );
        assert_eq!(
            page.matches(r#"<circle class="solid""#).count(),
            2,
            "{page}"
        );
        assert!(
            page.contains(
                "25 Oct</span> · max 184 · last 5 min 177 · mean 165 bpm · device unknown<"
            ),
            "{page}"
        );
        assert!(
            page.contains("2 tests · bpm over the twenty minutes"),
            "{page}"
        );
        assert!(!page.contains(r#"class="line""#), "nothing joins two tests");
    }

    #[test]
    fn tests_without_heart_rate_draw_no_heart_rate_chart() {
        let page = shown(&autumn().expect("the tests"), Over::Macrocycle);
        assert!(!page.contains("Heart rate in the test"), "{page}");
    }

    #[test]
    fn no_tests_is_said_rather_than_drawn() {
        let page = page(&[], &[], &ranges(), Over::All, &crate::run::View::default()).into_string();
        assert!(page.contains("No FTP test has been recorded."));
        assert!(!page.contains(r#"class="dot""#));
    }
}
