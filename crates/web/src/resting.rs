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
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced, name},
    run::View,
};

pub const PATH: &str = "/resting-heart-rate";
pub const TITLE: &str = "Resting heart rate";

/// The page: the summary, then the chart over the range chosen.
pub fn page(trended: &[Trended], ranges: &Ranges, over: Over, run: &View) -> Markup {
    detail::page(PATH, TITLE, over, run, &body(trended, ranges, over))
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

/// The summary, the chart over the range chosen and the latest nights, or why
/// there is no range.
fn body(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(
        trended
            .iter()
            .flat_map(|one| one.series.readings.iter().map(|reading| reading.on)),
    );
    let range = match crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.") {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let within: Vec<Trended> = trended.iter().map(|one| one.within(range)).collect();
    html! {
        (summary(trended, today))
        (detail::chart(&chart(&within, range, today)))
        (latest(trended, today))
        (detail::foot(&format!(
            "Dots: Garmin's resting heart rate for each day, its lowest 30-minute average. Line: each watch's {METHOD}. Band: 95%. Ringed: {FLAG}."
        )))
    }
}

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
        super::page(trended, ranges, over, &crate::run::View::default()).into_string()
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
