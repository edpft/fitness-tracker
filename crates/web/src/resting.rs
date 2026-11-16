//! Resting heart rate, on the detail template (#623, #417).
//!
//! **The summary is the latest 90-day line, its ±, and its change on the
//! line 90 days before**, read against the noise of the two (#615): the arrow
//! only where the change is beyond it. It is the latest whatever the range:
//! the range only chooses which days the chart shows.
//!
//! **One chart**: each day a dot, each watch's line over its band, and the
//! nights flagged as possible illness ringed. Each watch's line is its own
//! (§ 6); the watches never overlapped, so a tap on a day names the one that
//! recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    per_device::SentBy,
    range::{Over, Range, Ranges},
    resting::{Change, Day, FLAG, METHOD, Trended},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced, name};

pub const PATH: &str = "/resting-heart-rate";
pub const TITLE: &str = "Resting heart rate";

/// The summary, the chart over the range chosen and the latest nights, or why
/// there is no range.
/// The days the cards are drawn over, or what to say instead.
fn range(trended: &[Trended], ranges: &Ranges, over: Over) -> Result<Range, Markup> {
    let recorded = Range::spanning(
        trended
            .iter()
            .flat_map(|one| one.series.readings.iter().map(|reading| reading.on)),
    );
    crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.")
}

/// The summary, which is the latest whatever the range; or why there is no
/// range, which leaves every other card empty.
fn summary_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    match range(trended, ranges, over) {
        Ok(range) => summary(trended, range.to),
        Err(said) => said,
    }
}

/// Days, over the range chosen.
fn days_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let Ok(range) = range(trended, ranges, over) else {
        return html! {};
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
            summary_card(trended, ranges, over),
            days_card(trended, ranges, over),
            latest_card(trended, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// The summary.
    Summary,
    "resting-heart-rate-summary",
    configurator::analyses::RestingHeartRate,
    summary_card
);
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

/// The watch whose line is the latest, its last day, and that day against the
/// line 90 days before.
fn latest_line(trended: &[Trended]) -> Option<(&Trended, Day, Option<Change>)> {
    trended
        .iter()
        .filter_map(|one| {
            one.line
                .iter()
                .flatten()
                .next_back()
                .map(|day| (one, *day, one.change()))
        })
        .max_by_key(|(_, day, _)| day.on)
}

/// The latest line, its ±, and its change, read against its noise.
fn summary(trended: &[Trended], today: Date) -> Markup {
    let Some((one, day, change)) = latest_line(trended) else {
        return html! {};
    };
    let reading = match change {
        Some(Change {
            by,
            margin: Some(noise),
            direction,
            ..
        }) => Reading::Change {
            by,
            noise,
            direction,
            said: format!("{} bpm", signed(by)),
            over: "in 90 days".to_owned(),
        },
        Some(_) => Reading::NoRule("Too few days to measure the noise.".to_owned()),
        None => Reading::NoRule("No line 90 days before to read it against.".to_owned()),
    };
    let flagged = one.flags.last().map_or_else(String::new, |flag| {
        format!(" · last flagged {}", detail::day(flag.on, today))
    });
    let asof = format!(
        "Line on {} · {}{flagged}",
        detail::day(day.on, today),
        name(&one.series.sent_by)
    );
    detail::summary(
        &[Cell {
            metric: Metric::RestingHeartRate,
            label: "90-day average",
            figure: format!("{:.1}", day.bpm),
            unit: "bpm",
            margin: day.margin.map(|margin| format!("{margin:.1}")),
            readings: vec![reading],
        }],
        &asof,
    )
}

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

/// A change with its sign, to one place.
fn signed(by: f64) -> String {
    format!("{by:+.1}").replace('-', "−")
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
    type Cards = (Summary, DaysChart, Latest);

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
    fn the_summary_is_the_latest_line_its_band_and_its_change() {
        // 180 days scattering around 57, then 120 around 51: on 27 Oct the
        // line is 51.0, and 55.0 on 29 Jul, a fall far beyond the noise.
        let page = shown(
            &days(300, |index| {
                let base = if index < 180 { 56 } else { 50 };
                base + u32::try_from(index % 3).expect("small")
            }),
            &NO_PLAN,
            Over::All,
        );

        assert!(
            page.contains(r#"class="cell resting-heart-rate""#),
            "{page}"
        );
        assert!(page.contains("51.0<small>bpm</small>"), "{page}");
        assert!(page.contains("± "), "{page}");
        assert!(page.contains("−4.0 bpm"), "{page}");
        assert!(page.contains("in 90 days"), "{page}");
        assert!(page.contains(r#"class="badge""#), "falling, with its arrow");
        assert!(page.contains(r#"class="noise""#), "{page}");
        assert!(page.contains("Line on 27 Oct · device unknown"), "{page}");
    }

    #[test]
    fn a_change_within_the_noise_has_no_arrow() {
        let page = shown(
            &days(200, |index| 50 + u32::try_from(index % 5).expect("small")),
            &NO_PLAN,
            Over::All,
        );
        assert!(page.contains(r#"class="change plain""#), "{page}");
        assert!(!page.contains(r#"class="badge"#), "{page}");
    }

    #[test]
    fn no_line_ninety_days_before_is_said() {
        let page = shown(&days(100, |_| 50), &NO_PLAN, Over::All);
        assert!(
            page.contains("No line 90 days before to read it against."),
            "{page}"
        );
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
        assert!(page.contains("last flagged 11 Feb"), "{page}");
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
