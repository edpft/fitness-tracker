//! Resting heart rate, on the detail template (#623, #417).
//!
//! **It opens on the summary Progress shows** ([`crate::summary`], #830):
//! resting heart rate since the macrocycle's start
//! against the expected result of the training done, one card on both pages. On a
//! day no plan is in force there is no summary, and the page opens on its
//! chart.
//!
//! **One chart**: each day a dot, each watch's line over its band, and the
//! nights flagged as possible illness ringed. Each watch's line is its own
//! (§ 6); the watches never overlapped, so a tap on a day names the one that
//! recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    per_device::SentBy,
    range::{Over, Range, Ranges},
    resting::{Day, FLAG, METHOD, Trended},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Chart, Layer, Metric, Traced, name};

pub const PATH: &str = "/resting-heart-rate";
pub const TITLE: &str = "Resting heart rate";

/// The days the cards are drawn over, or what to say instead.
fn range(trended: &[Trended], ranges: &Ranges, over: Over) -> Result<Range, Markup> {
    let recorded = Range::spanning(
        trended
            .iter()
            .flat_map(|one| one.series.readings.iter().map(|reading| reading.on)),
    );
    crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.")
}

/// Days, over the range chosen.
fn days_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let range = match range(trended, ranges, over) {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let within: Vec<Trended> = trended.iter().map(|one| one.within(range)).collect();
    detail::chart(&chart(&within, range, today))
}

/// The latest readings, and what the dots, lines and bands are.
fn latest_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let Ok(range) = range(trended, ranges, over) else {
        return html! {};
    };
    let today = range.to;
    html! {
        (latest(trended, today))
        (detail::foot(&format!(
            "Dots: Garmin's resting heart rate for each day, its lowest 30-minute average. Line: each watch's {METHOD}. Band: 95%. Ringed: {FLAG}."
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
pub fn page(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    laid_out(
        over,
        &[
            days_card(trended, ranges, over),
            latest_card(trended, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// Days.
    DaysChart,
    "resting-heart-rate-days",
    configurator::analyses::RestingHeartRate,
    days_card
);
crate::card::ranged_card!(
    /// The latest readings.
    Latest,
    "resting-heart-rate-latest",
    configurator::analyses::RestingHeartRate,
    latest_card
);

/// Each day, each watch's line over its band, and the flagged nights ringed.
fn chart(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut dots: Vec<(Date, f64)> = Vec::new();
    let mut rings: Vec<(Date, f64)> = Vec::new();
    for one in within {
        let sent_by = name(&one.series.sent_by);
        for reading in &one.series.readings {
            let rate = reading.value.as_u32();
            dots.push((reading.on, f64::from(rate)));
            said.entry(reading.on)
                .or_default()
                .push(format!("{rate} bpm · {sent_by}"));
        }
        for day in one.line.iter().flatten() {
            let line = day.margin.map_or_else(
                || format!("line {:.1}", day.bpm),
                |margin| format!("line {:.1} ± {margin:.1}", day.bpm),
            );
            said.entry(day.on).or_default().push(line);
        }
        for flag in &one.flags {
            rings.push((flag.on, f64::from(flag.rate.as_u32())));
            said.entry(flag.on)
                .or_default()
                .push(format!("flagged, against {:.0} normally", flag.normal));
        }
    }
    let sub = format!("{} · bpm", counted(dots.len(), "day"));
    Chart {
        id: "resting",
        metric: Metric::RestingHeartRate,
        title: "Resting heart rate",
        sub,
        range,
        axis: Axis::Figures,
        layers: vec![
            Layer::Trend(within.iter().flat_map(|one| traced(&one.line)).collect()),
            Layer::Dots(dots),
            Layer::Rings(rings),
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
                    value: day.bpm,
                    margin: day.margin,
                })
                .collect()
        })
        .collect()
}

/// The latest seven days, newest first, the flagged ones said so.
fn latest(trended: &[Trended], today: Date) -> Markup {
    let mut every: Vec<(Date, u32, bool, &SentBy)> = trended
        .iter()
        .flat_map(|one| {
            one.series.readings.iter().map(|reading| {
                (
                    reading.on,
                    reading.value.as_u32(),
                    one.flags.iter().any(|flag| flag.on == reading.on),
                    &one.series.sent_by,
                )
            })
        })
        .collect();
    every.sort_by_key(|(on, ..)| *on);
    let rows: Vec<(String, Markup)> = every
        .iter()
        .rev()
        .take(7)
        .map(|(on, rate, flagged, _)| {
            (
                detail::day(*on, today),
                html! { (rate) " bpm" @if *flagged { " · flagged" } },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Days", "bpm", &rows)
}

fn counted(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

/// Resting heart rate over a range, one series per watch with its slow line
/// and the nights it flags.
#[derive(Debug)]
pub struct Page {
    over: Over,
}

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (crate::summary::RestingHeartRate, DaysChart, Latest);

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
        let [summary, days, latest] = cards;
        laid_out(self.over, &[crate::summary::atop(&summary), days, latest])
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            per_device::{DeviceSeries, Reading, SentBy},
            range::{Over, Range, Ranges},
            resting::Trended,
        },
        measure::BeatsPerMinute,
    };
    use jiff::civil::{Date, date};

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn trended(rates: impl IntoIterator<Item = (Date, u32)>) -> [Trended; 1] {
        [Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: rates
                .into_iter()
                .map(|(on, beats)| Reading {
                    on,
                    value: BeatsPerMinute::new(beats).expect("a rate"),
                })
                .collect(),
        })]
    }

    /// `count` days from 1 January 2026, each `rate` of its index.
    fn days(count: i64, rate: impl Fn(i64) -> u32) -> [Trended; 1] {
        trended((0..count).map(|index| {
            (
                date(2026, 1, 1)
                    .checked_add(jiff::Span::new().days(index))
                    .expect("a day"),
                rate(index),
            )
        }))
    }

    fn shown(trended: &[Trended], ranges: &Ranges, over: Over) -> String {
        super::page(trended, ranges, over).into_string()
    }

    #[test]
    fn a_day_is_a_dot_the_line_has_its_band_and_a_flagged_night_is_ringed() {
        let page = shown(
            &days(42, |index| match index {
                40 => 57,
                41 => 58,
                _ if index % 2 == 0 => 50,
                _ => 52,
            }),
            &NO_PLAN,
            Over::All,
        );

        assert_eq!(page.matches(r#"class="dot""#).count(), 42, "{page}");
        assert_eq!(page.matches(r#"class="ring""#).count(), 1, "{page}");
        assert_eq!(page.matches(r#"class="line""#).count(), 1, "{page}");
        assert!(
            page.contains("11 Feb</span> · 58 bpm · device unknown"),
            "{page}"
        );
        assert!(page.contains("flagged, against 51 normally"), "{page}");
        assert!(page.contains("58 bpm · flagged"), "the latest days say so");
    }

    #[test]
    fn a_range_shows_only_its_days_and_nothing_recorded_is_said() {
        let one = trended([
            (date(2024, 12, 2), 50),
            (date(2024, 12, 21), 63),
            (date(2024, 12, 22), 53),
        ]);
        let ranges = Ranges {
            mesocycle: Some(Range {
                from: date(2024, 12, 16),
                to: date(2024, 12, 22),
            }),
            macrocycle: None,
        };
        let page = shown(&one, &ranges, Over::Mesocycle);
        assert!(page.contains("21 Dec</span> · 63 bpm"), "{page}");
        assert!(
            !page.contains("2 Dec</span> · 50 bpm"),
            "before this mesocycle"
        );
        assert!(page.contains(r#"href="/resting-heart-rate?over=all""#));

        let page = shown(&one, &ranges, Over::Macrocycle);
        assert!(page.contains("No macrocycle is planned for today."));

        let page = shown(&[], &NO_PLAN, Over::All);
        assert!(page.contains("Nothing has been recorded."));
    }
}
