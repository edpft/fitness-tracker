//! Pulse wave velocity, on the detail template (#624, #471).
//!
//! **The summary is the latest 90-day line, its ±, and its change on the
//! line 90 days before**, read against the noise of the two as resting heart
//! rate's is (#688): the arrow only where the change is beyond it, and lower
//! is better. It is the latest whatever the range: the range only chooses
//! which days the chart shows.
//!
//! **One chart**: each reading a dot, and each algorithm's line over its band.
//! An algorithm's readings are a series of their own (§ 6), named only where
//! the page holds more than one.
//!
//! **No vascular age and no bands for age** (operator, 2026-10-28): see
//! [`domain::analytical::pulse_wave_velocity`].

use std::collections::BTreeMap;

use domain::analytical::{
    pulse_wave_velocity::{Day, METHOD, Series},
    range::{Over, Range, Ranges},
    resting::Change,
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced};

pub const PATH: &str = "/pulse-wave-velocity";
const TITLE: &str = "Pulse wave velocity";

/// The summary, the chart over the range chosen and the latest readings, or
/// why there is no range.
/// The days the cards are drawn over, or what to say instead.
fn range(series: &[Series], ranges: &Ranges, over: Over) -> Result<Range, Markup> {
    let recorded = Range::spanning(
        series
            .iter()
            .flat_map(|one| one.readings.iter().map(|reading| reading.on)),
    );
    crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.")
}

/// The summary, which is the latest whatever the range; or why there is no
/// range, which leaves every other card empty.
fn summary_card(series: &[Series], ranges: &Ranges, over: Over) -> Markup {
    match range(series, ranges, over) {
        Ok(range) => summary(series, range.to),
        Err(said) => said,
    }
}

/// Readings, over the range chosen.
fn readings_card(series: &[Series], ranges: &Ranges, over: Over) -> Markup {
    let Ok(range) = range(series, ranges, over) else {
        return html! {};
    };
    let today = range.to;
    let within: Vec<Series> = series.iter().map(|one| one.within(range)).collect();
    detail::chart(&chart(series, &within, range, today))
}

/// The latest readings, and what the dots, lines and bands are.
fn latest_card(series: &[Series], ranges: &Ranges, over: Over) -> Markup {
    let Ok(range) = range(series, ranges, over) else {
        return html! {};
    };
    let today = range.to;
    html! {
        (latest(series, today))
        (detail::foot(&format!(
            "Dots: each Body Scan reading. Line: each algorithm's {METHOD}. Band: 95%. Lower is better. Whether the scale follows one person's change has not been tested."
        )))
    }
}

/// The page: its cards, under the range chosen.
fn laid_out(over: Over, cards: &[Markup]) -> Markup {
    detail::page(
        PATH,
        TITLE,
        Some(over),
        &html! { @for card in cards { (card) } },
    )
}

/// The whole page over `over`, each card drawn from the same reads, as a
/// load draws it.
#[cfg(test)]
pub fn page(series: &[Series], ranges: &Ranges, over: Over) -> Markup {
    laid_out(
        over,
        &[
            summary_card(series, ranges, over),
            readings_card(series, ranges, over),
            latest_card(series, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// The summary.
    Summary,
    "pulse-wave-velocity-summary",
    configurator::analyses::PulseWaveVelocity,
    summary_card
);
crate::card::ranged_card!(
    /// Readings.
    ReadingsChart,
    "pulse-wave-velocity-readings",
    configurator::analyses::PulseWaveVelocity,
    readings_card
);
crate::card::ranged_card!(
    /// The latest readings.
    Latest,
    "pulse-wave-velocity-latest",
    configurator::analyses::PulseWaveVelocity,
    latest_card
);

/// The latest line, its ±, and its change, read against its noise.
fn summary(series: &[Series], today: Date) -> Markup {
    let Some((one, day)) = series
        .iter()
        .filter_map(|one| one.line.iter().flatten().next_back().map(|day| (one, *day)))
        .max_by_key(|(_, day)| day.on)
    else {
        return html! {};
    };
    let reading = match one.change() {
        Some(Change {
            by,
            margin: Some(noise),
            direction,
            ..
        }) => Reading::Change {
            by,
            noise,
            direction,
            said: format!("{} m/s", signed(by)),
            over: "in 90 days".to_owned(),
        },
        Some(_) => Reading::NoRule("Too few readings to measure the noise.".to_owned()),
        None => Reading::NoRule("No line 90 days before to read it against.".to_owned()),
    };
    detail::summary(
        &[Cell {
            metric: Metric::PulseWaveVelocity,
            label: "90-day average",
            figure: format!("{:.2}", day.metres_per_second),
            unit: "m/s",
            margin: day.margin.map(|margin| format!("{margin:.2}")),
            readings: vec![reading],
        }],
        &format!(
            "Line on {}{}",
            detail::day(day.on, today),
            algorithm(series, one)
        ),
    )
}

/// Each reading, and each algorithm's line over its band.
fn chart(series: &[Series], within: &[Series], range: Range, today: Date) -> Chart<'static> {
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut dots: Vec<(Date, f64)> = Vec::new();
    for one in within {
        let named = algorithm(series, one);
        for reading in &one.readings {
            let metres = f64::from(reading.value.as_millimetres_per_second()) / 1_000.0;
            dots.push((reading.on, metres));
            said.entry(reading.on)
                .or_default()
                .push(format!("{metres:.2} m/s{named}"));
        }
        for day in one.line.iter().flatten() {
            let line = day.margin.map_or_else(
                || format!("line {:.2}", day.metres_per_second),
                |margin| format!("line {:.2} ± {margin:.2}", day.metres_per_second),
            );
            said.entry(day.on).or_default().push(line);
        }
    }
    let sub = format!("{} · m/s", counted(dots.len()));
    Chart {
        id: "pulse-wave-velocity",
        metric: Metric::PulseWaveVelocity,
        title: "Pulse wave velocity",
        sub,
        range,
        axis: Axis::Figures,
        layers: vec![
            Layer::Trend(within.iter().flat_map(|one| traced(&one.line)).collect()),
            Layer::Dots(dots),
        ],
        days: said
            .into_iter()
            .map(|(on, said)| {
                (
                    on,
                    html! { span.on { (detail::day(on, today)) } " · " (said.join(" · ")) },
                )
            })
            .collect(),
    }
}

fn traced(runs: &[Vec<Day>]) -> Vec<Vec<Traced>> {
    runs.iter()
        .map(|run| {
            run.iter()
                .map(|day| Traced {
                    on: day.on,
                    value: day.metres_per_second,
                    margin: day.margin,
                })
                .collect()
        })
        .collect()
}

/// The latest seven readings, newest first.
fn latest(series: &[Series], today: Date) -> Markup {
    let mut every: Vec<(Date, u32)> = series
        .iter()
        .flat_map(|one| {
            one.readings
                .iter()
                .map(|reading| (reading.on, reading.value.as_millimetres_per_second()))
        })
        .collect();
    every.sort_by_key(|(on, _)| *on);
    let rows: Vec<(String, Markup)> = every
        .iter()
        .rev()
        .take(7)
        .map(|(on, millimetres)| {
            (
                detail::day(*on, today),
                html! { (format!("{:.2}", f64::from(*millimetres) / 1_000.0)) " m/s" },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Readings", "m/s", &rows)
}

/// The algorithm, named only where the page holds more than one.
fn algorithm(series: &[Series], one: &Series) -> String {
    if series.len() > 1 {
        format!(" · algorithm {}", one.algorithm)
    } else {
        String::new()
    }
}

/// A change with its sign, to two places.
fn signed(by: f64) -> String {
    format!("{by:+.2}").replace('-', "−")
}

fn counted(count: usize) -> String {
    if count == 1 {
        "1 reading".to_owned()
    } else {
        format!("{count} readings")
    }
}

/// Pulse wave velocity over a range, with its slow line (#471).
#[derive(Debug)]
pub struct Page {
    over: Over,
}

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (Summary, ReadingsChart, Latest);

    fn asked(
        _path: &axum::extract::RawPathParams,
        query: &std::collections::HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self {
            over: crate::drawn::over(query),
        })
    }

    fn over(&self) -> Option<Over> {
        Some(self.over)
    }

    fn layout(&self, cards: [Markup; 3]) -> Markup {
        laid_out(self.over, &cards)
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            pulse_wave_velocity::{Measured, Series, by_algorithm},
            range::{Over, Range, Ranges},
        },
        body::PulseWaveVelocity,
    };
    use jiff::civil::{Date, date};

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn measured(on: Date, algorithm: u64, millimetres: u32) -> Measured {
        Measured {
            on,
            algorithm,
            velocity: PulseWaveVelocity::from_millimetres_per_second(millimetres),
        }
    }

    /// `count` daily readings from 1 January 2026, each `millimetres` of its
    /// index.
    fn daily(count: i64, millimetres: impl Fn(i64) -> u32) -> Vec<Series> {
        by_algorithm(
            &(0..count)
                .map(|index| {
                    measured(
                        date(2026, 1, 1)
                            .checked_add(jiff::Span::new().days(index))
                            .expect("a day"),
                        0,
                        millimetres(index),
                    )
                })
                .collect::<Vec<_>>(),
        )
    }

    fn shown(series: &[Series], ranges: &Ranges, over: Over) -> String {
        super::page(series, ranges, over).into_string()
    }

    #[test]
    fn the_summary_is_the_latest_line_its_band_and_its_change() {
        // Alternating 7.0 and 7.4 m/s, a metre a second lower from 21 April:
        // on 19 July the line is 6.2 and 7.2 the 90 days before.
        let page = shown(
            &daily(200, |index| {
                let base = if index % 2 == 0 { 7_000 } else { 7_400 };
                if index >= 110 { base - 1_000 } else { base }
            }),
            &NO_PLAN,
            Over::All,
        );

        assert!(
            page.contains(r#"class="cell pulse-wave-velocity""#),
            "{page}"
        );
        assert!(page.contains("6.20<small>m/s</small>"), "{page}");
        assert!(page.contains(r#"class="margin""#), "{page}");
        assert!(page.contains("−1.00 m/s"), "{page}");
        assert!(page.contains(r#"class="badge""#), "falling, with its arrow");
        assert!(
            page.contains("Line on 19 Jul<"),
            "one algorithm is not named"
        );
        assert!(page.contains("Lower is better."), "{page}");
    }

    #[test]
    fn a_change_within_the_noise_has_no_arrow_and_too_short_a_record_says_so() {
        let page = shown(
            &daily(200, |index| if index % 2 == 0 { 7_000 } else { 7_400 }),
            &NO_PLAN,
            Over::All,
        );
        assert!(page.contains(r#"class="change plain""#), "{page}");
        assert!(!page.contains(r#"class="badge"#), "{page}");

        let page = shown(&daily(100, |_| 7_200), &NO_PLAN, Over::All);
        assert!(
            page.contains("No line 90 days before to read it against."),
            "{page}"
        );
    }

    #[test]
    fn a_reading_is_a_dot_and_the_line_has_its_band() {
        let page = shown(
            &daily(60, |index| if index % 2 == 0 { 7_000 } else { 7_400 }),
            &NO_PLAN,
            Over::All,
        );
        assert_eq!(page.matches(r#"class="dot""#).count(), 60, "{page}");
        assert_eq!(page.matches(r#"class="line""#).count(), 1, "{page}");
        assert!(
            page.contains("1 Mar</span> · 7.40 m/s · line 7.20"),
            "{page}"
        );
    }

    #[test]
    fn two_algorithms_are_named_and_never_joined() {
        let series = by_algorithm(&[
            measured(date(2026, 3, 1), 1, 7_100),
            measured(date(2026, 3, 2), 2, 7_300),
        ]);
        let page = shown(&series, &NO_PLAN, Over::All);
        assert!(page.contains("7.10 m/s · algorithm 1"), "{page}");
        assert!(page.contains("7.30 m/s · algorithm 2"), "{page}");
    }

    #[test]
    fn a_range_shows_only_its_readings_and_nothing_recorded_is_said() {
        let series = by_algorithm(&[
            measured(date(2024, 12, 2), 0, 7_000),
            measured(date(2024, 12, 21), 0, 7_300),
        ]);
        let ranges = Ranges {
            mesocycle: Some(Range {
                from: date(2024, 12, 16),
                to: date(2024, 12, 22),
            }),
            macrocycle: None,
        };
        let page = shown(&series, &ranges, Over::Mesocycle);
        assert!(page.contains("21 Dec</span> · 7.30 m/s"), "{page}");
        assert!(
            !page.contains("2 Dec</span> · 7.00 m/s"),
            "before this mesocycle"
        );
        assert!(page.contains(r#"href="/pulse-wave-velocity?over=all""#));

        let page = shown(&series, &ranges, Over::Macrocycle);
        assert!(page.contains("No macrocycle is planned for today."));

        let page = shown(&[], &NO_PLAN, Over::All);
        assert!(page.contains("Nothing has been recorded."));
    }
}
