//! A night's sleep, on the detail template (#625, #255, #418).
//!
//! **Time asleep and regularity side by side**: each its latest trend, where
//! that falls in its published reference, and its change on the trend
//! [`NIGHTS`] days before against the page's own threshold. It is the latest
//! whatever the range: the range only chooses which days the charts show.
//! The operator, 2026-10-28, on the mock: *"This is a really good summary!"*
//!
//! **No ± and no band** (operator, 2026-11-02): a night is what happened, so
//! a trend through nights has no measurement noise to show.
//!
//! **By the morning Garmin reports the night under**, one mark per night. A
//! night with no sleep recorded is the empty stretch it is (§ 37), and nothing
//! joins two nights. Each watch's trend is its own (§ 6); the watches never
//! overlapped, so the tap on a night names the one that recorded it.
//!
//! **When is a bar from falling asleep to waking**, on a clock that runs down
//! the chart, so a later night sits lower. That is the regularity series'
//! data point.

use std::collections::BTreeMap;

use domain::analytical::{
    per_device::SentBy,
    range::{Over, Range, Ranges},
    sleep::{
        self, ASLEEP_MAINTAINING_MINUTES, METHOD, NIGHTS, Night, REGULARITY_MAINTAINING, Trend,
        Trended, WINDRED,
    },
};
use jiff::civil::{Date, DateTime};
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Segment, Traced, name},
    run::View,
};

pub const PATH: &str = "/sleep";
pub const TITLE: &str = "Sleep";

/// Seconds in an hour, for the axes.
const HOUR: f64 = 3600.0;

/// The page: the summary, then each chart over the range chosen.
pub fn page(nights: &[Trended], ranges: &Ranges, over: Over, run: &View) -> Markup {
    detail::page(PATH, TITLE, over, run, &body(nights, ranges, over))
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

/// The summary, the three charts over the range chosen and the latest
/// nights, or why there is no range.
fn body(nights: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(
        nights
            .iter()
            .flat_map(|one| one.series.readings.iter().map(|reading| reading.on)),
    );
    let range = match crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.") {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let within: Vec<Trended> = nights.iter().map(|one| one.within(range)).collect();
    html! {
        (summary(nights, today))
        (detail::chart(&asleep(&within, range, today)))
        (detail::chart(&window(&within, range, today)))
        (detail::chart(&regularity(&within, range, today)))
        (latest(nights, today))
        (detail::foot(&format!(
            "Dots: each night. Bars: fell asleep to woke. Line: each watch's trend, the {METHOD}. Grey: the NHS's 7 to 9 hours for adults; the fifths of Windred et al. (Sleep, 2024), 7 days of wrist actigraphy in 61,000 UK adults, among whom higher was better throughout, darker higher."
        )))
    }
}

/// The watch whose trend is the latest, and that trend.
fn latest_trend(
    nights: &[Trended],
    trend: impl Fn(&Trended) -> &[Vec<Trend>],
) -> Option<(&SentBy, Trend)> {
    nights
        .iter()
        .filter_map(|one| {
            trend(one)
                .iter()
                .flatten()
                .next_back()
                .map(|day| (&one.series.sent_by, *day))
        })
        .max_by_key(|(_, day)| day.on)
}

/// Its change on the trend [`NIGHTS`] days before, against `maintaining`.
fn change(day: &Trend, maintaining: f64, said: impl Fn(f64) -> String) -> Reading {
    match (day.change, day.direction) {
        (Some(by), Some(direction)) => Reading::Change {
            by,
            noise: maintaining,
            direction: Some(direction),
            said: said(by),
            over: format!("in {NIGHTS} days"),
        },
        _ => Reading::NoRule(format!("No trend {NIGHTS} days before to read it against.")),
    }
}

/// Time asleep and regularity, each where its latest trend falls in its
/// reference, and its change.
fn summary(nights: &[Trended], today: Date) -> Markup {
    let asleep = latest_trend(nights, |one| &one.asleep);
    let regular = latest_trend(nights, |one| &one.regular);
    let mut cells: Vec<Cell<'_>> = Vec::new();
    let mut asof: Vec<String> = Vec::new();
    if let Some((sent_by, day)) = asleep {
        let (low, high) = sleep::NHS;
        cells.push(Cell {
            metric: Metric::Asleep,
            label: "Asleep",
            figure: hours_and_minutes(day.value, ""),
            unit: "min",
            margin: None,
            readings: vec![
                Reading::Position {
                    segments: vec![
                        segment(5.0, low / 60.0, "< 7 h"),
                        segment(low / 60.0, high / 60.0, "7–9 h"),
                        segment(high / 60.0, 11.0, "> 9 h"),
                    ],
                    at: day.value / 60.0,
                },
                change(&day, ASLEEP_MAINTAINING_MINUTES, |by| {
                    format!("{} min", signed(by, 0))
                }),
            ],
        });
        asof.push(format!(
            "Trend on {} · {}",
            detail::day(day.on, today),
            name(sent_by)
        ));
    }
    if let Some((sent_by, day)) = regular {
        let edges: Vec<f64> = std::iter::once(60.0)
            .chain(WINDRED)
            .chain(std::iter::once(100.0))
            .collect();
        cells.push(Cell {
            metric: Metric::Regularity,
            label: "Regularity",
            figure: format!("{:.1}", day.value),
            unit: "",
            margin: None,
            readings: vec![
                Reading::Position {
                    segments: edges
                        .windows(2)
                        .enumerate()
                        .filter_map(|(fifth, pair)| match pair {
                            [from, to] => {
                                Some(segment(*from, *to, if fifth == 2 { "fifths" } else { "" }))
                            }
                            _ => None,
                        })
                        .collect(),
                    at: day.value,
                },
                change(&day, REGULARITY_MAINTAINING, |by| signed(by, 1)),
            ],
        });
        asof.push(format!(
            "Trend on {} · {}",
            detail::day(day.on, today),
            name(sent_by)
        ));
    }
    if cells.is_empty() {
        return html! {};
    }
    asof.dedup();
    detail::summary(&cells, &asof.join("; "))
}

fn segment(from: f64, to: f64, name: &str) -> Segment {
    Segment {
        from,
        to,
        name: name.to_owned(),
    }
}

/// A grey region from `low` to `high` across the whole range.
fn across(range: Range, low: f64, high: f64) -> Layer {
    Layer::Reference(vec![(range.from, low, high), (range.to, low, high)])
}

/// A trend's runs, in the chart's unit.
fn traced(runs: &[Vec<Trend>], unit: impl Fn(f64) -> f64) -> Vec<Vec<Traced>> {
    runs.iter()
        .map(|run| {
            run.iter()
                .map(|day| Traced {
                    on: day.on,
                    value: unit(day.value),
                    margin: None,
                })
                .collect()
        })
        .collect()
}

/// Time asleep in hours: each night, each watch's trend, over the NHS's band.
fn asleep(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let (low, high) = sleep::NHS;
    let nights: Vec<(Date, f64, &SentBy)> = each(within, |night| minutes_asleep(night) / 60.0);
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    for one in within {
        for reading in &one.series.readings {
            said.entry(reading.on).or_default().push(format!(
                "{} · {}",
                length(&reading.value),
                name(&one.series.sent_by)
            ));
        }
        for day in one.asleep.iter().flatten() {
            let entry = said.entry(day.on).or_default();
            entry.push(format!("trend {}", hours_and_minutes(day.value, " min")));
            if let Some(by) = day.change {
                entry.push(format!("{} min in {NIGHTS} days", signed(by, 0)));
            }
        }
    }
    Chart {
        id: "asleep",
        metric: Metric::Asleep,
        title: "Time asleep",
        sub: format!("{} · hours", counted(nights.len(), "night")),
        range,
        axis: Axis::Figures,
        layers: vec![
            across(range, low / 60.0, high / 60.0),
            Layer::Trend(
                within
                    .iter()
                    .flat_map(|one| traced(&one.asleep, |minutes| minutes / 60.0))
                    .collect(),
            ),
            Layer::Dots(nights.iter().map(|(on, value, _)| (*on, *value)).collect()),
        ],
        days: tapped(said, today),
    }
}

/// Fell asleep to woke, one bar a night, on a clock running down the chart.
fn window(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let mut bars: Vec<(Date, f64, f64)> = Vec::new();
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    for one in within {
        for reading in &one.series.readings {
            let night = &reading.value;
            bars.push((
                reading.on,
                from_midnight(night.fell_asleep, reading.on),
                from_midnight(night.woke, reading.on),
            ));
            said.entry(reading.on).or_default().push(format!(
                "{} to {} · {}",
                night.fell_asleep.strftime("%H:%M"),
                night.woke.strftime("%H:%M"),
                name(&one.series.sent_by)
            ));
        }
    }
    Chart {
        id: "window",
        metric: Metric::Asleep,
        title: "Fell asleep to woke",
        sub: counted(bars.len(), "night"),
        range,
        axis: Axis::Clock,
        layers: vec![Layer::Bars(bars)],
        days: tapped(said, today),
    }
}

/// Each morning's Sleep Regularity Index against the night before, each
/// watch's trend, over Windred et al.'s fifths, each a shade darker than the
/// one below it.
fn regularity(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut mornings: Vec<(Date, f64)> = Vec::new();
    for one in within {
        for (on, value) in &one.regularity {
            mornings.push((*on, *value));
            said.entry(*on)
                .or_default()
                .push(format!("{value:.1} · {}", name(&one.series.sent_by)));
        }
        for day in one.regular.iter().flatten() {
            let entry = said.entry(day.on).or_default();
            entry.push(format!("trend {:.1}", day.value));
            if let Some(by) = day.change {
                entry.push(format!("{} in {NIGHTS} days", signed(by, 1)));
            }
        }
    }
    let layers = WINDRED
        .into_iter()
        .map(|edge| across(range, edge, 100.0))
        .chain([
            Layer::Trend(
                within
                    .iter()
                    .flat_map(|one| traced(&one.regular, |index| index))
                    .collect(),
            ),
            Layer::Dots(mornings.clone()),
        ])
        .collect();
    Chart {
        id: "regularity",
        metric: Metric::Regularity,
        title: "Regularity",
        sub: format!(
            "{}, each against the night before · Sleep Regularity Index",
            counted(mornings.len(), "morning")
        ),
        range,
        axis: Axis::Figures,
        layers,
        days: tapped(said, today),
    }
}

/// Every night, by its morning, in `unit`, and the watch that recorded it.
fn each(within: &[Trended], unit: impl Fn(&Night) -> f64) -> Vec<(Date, f64, &SentBy)> {
    within
        .iter()
        .flat_map(|one| {
            one.series
                .readings
                .iter()
                .map(|reading| (reading.on, unit(&reading.value), &one.series.sent_by))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// What a tap on each day shows, oldest first.
fn tapped(said: BTreeMap<Date, Vec<String>>, today: Date) -> Vec<(Date, Markup)> {
    said.into_iter()
        .map(|(on, said)| {
            (
                on,
                html! { span.on { (detail::day(on, today)) } " · " (said.join(" · ")) },
            )
        })
        .collect()
}

/// The latest seven nights, each how long and when.
fn latest(nights: &[Trended], today: Date) -> Markup {
    let mut every: Vec<(Date, Night)> = nights
        .iter()
        .flat_map(|one: &Trended| {
            one.series
                .readings
                .iter()
                .map(|reading| (reading.on, reading.value))
        })
        .collect();
    every.sort_by_key(|(on, _)| *on);
    let rows: Vec<(String, Markup)> = every
        .iter()
        .rev()
        .take(7)
        .map(|(on, night)| {
            (
                detail::day(*on, today),
                html! {
                    (length(night)) " · "
                    (night.fell_asleep.strftime("%H:%M")) " to " (night.woke.strftime("%H:%M"))
                },
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Nights", "asleep · fell asleep to woke", &rows)
}

/// A night's time asleep in hours and whole minutes, as the page before wrote
/// it.
fn length(night: &Night) -> String {
    let minutes = night.asleep.as_seconds() / 60;
    format!("{} h {:02} min", minutes / 60, minutes % 60)
}

fn minutes_asleep(night: &Night) -> f64 {
    let seconds = u32::try_from(night.asleep.as_seconds()).unwrap_or(u32::MAX);
    f64::from(seconds) / 60.0
}

fn hours(seconds: f64) -> f64 {
    seconds / HOUR
}

/// A wall clock as hours from the morning's midnight: the evening before is
/// negative.
fn from_midnight(at: DateTime, morning: Date) -> f64 {
    hours(
        at.duration_since(morning.to_datetime(jiff::civil::Time::midnight()))
            .as_secs_f64(),
    )
}

/// Minutes as hours and minutes, to the nearest minute, the minutes followed
/// by `unit`.
fn hours_and_minutes(minutes: f64, unit: &str) -> String {
    let whole = jiff::SignedDuration::try_from_secs_f64(minutes.round() * 60.0)
        .map_or(0, |duration| duration.as_mins());
    format!("{} h {:02}{unit}", whole / 60, whole % 60)
}

/// A change with its sign, to `places`.
fn signed(by: f64, places: usize) -> String {
    format!("{by:+.places$}").replace('-', "−")
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
            sleep::{Night, Trended},
        },
        measure::PositiveDuration,
    };
    use jiff::civil::date;

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn night(day: i8, asleep: u64, fell: (i8, i8, i8), woke: (i8, i8)) -> Reading<Night> {
        let (fell_day, fell_hour, fell_minute) = fell;
        Reading {
            on: date(2026, 10, day),
            value: Night {
                asleep: PositiveDuration::from_seconds(asleep).expect("asleep"),
                fell_asleep: date(2026, 10, fell_day).at(fell_hour, fell_minute, 0, 0),
                woke: date(2026, 10, day).at(woke.0, woke.1, 0, 0),
            },
        }
    }

    fn week() -> [Trended; 1] {
        [Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: vec![
                night(22, 29_040, (21, 22, 26), (6, 30)),
                night(25, 28_380, (24, 22, 23), (6, 16)),
                night(27, 24_300, (27, 0, 12), (6, 57)),
            ],
        })]
    }

    /// Every night from 2 to 27 October: 7 h 30 min from 23:00 to 06:30,
    /// but an hour later at both ends from the 14th, and the 20th missing.
    fn october() -> [Trended; 1] {
        let readings = (2..=27)
            .filter(|day| *day != 20)
            .map(|day| {
                if day < 14 {
                    night(day, 27_000, (day - 1, 23, 0), (6, 30))
                } else {
                    night(day, 27_000, (day, 0, 0), (7, 30))
                }
            })
            .collect();
        [Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings,
        })]
    }

    fn shown(nights: &[Trended], ranges: &Ranges, over: Over) -> String {
        super::page(nights, ranges, over, &crate::run::View::default()).into_string()
    }

    #[test]
    fn a_night_is_a_dot_and_a_bar_and_a_missing_night_is_neither() {
        let page = shown(&week(), &NO_PLAN, Over::All);

        assert!(
            page.contains("22 Oct</span> · 8 h 04 min · device unknown"),
            "{page}"
        );
        assert!(
            page.contains("22 Oct</span> · 22:26 to 06:30 · device unknown"),
            "{page}"
        );
        assert!(page.contains("27 Oct</span> · 00:12 to 06:57"), "{page}");
        assert!(
            !page.contains("26 Oct</span>"),
            "no sleep was recorded that night"
        );
        assert_eq!(page.matches(r#"class="bar""#).count(), 3, "{page}");
        assert_eq!(
            page.matches(r#"class="dot""#).count(),
            3,
            "time asleep's; no morning has an SRI, which needs the night before"
        );
        assert!(
            !page.contains(r#"class="line""#),
            "nothing joins two nights"
        );
        assert!(page.contains("3 nights · hours"), "{page}");
    }

    #[test]
    fn the_summary_is_each_trend_in_its_reference_and_its_change() {
        let page = shown(&october(), &NO_PLAN, Over::All);

        assert!(page.contains(r#"class="cell sleep""#), "{page}");
        assert!(page.contains(r#"class="cell sleep-regularity""#), "{page}");
        assert!(page.contains("7 h 30<small>min</small>"), "{page}");
        assert!(page.contains("99.3<small></small>"), "{page}");
        assert_eq!(page.matches(r#"class="segment lit""#).count(), 2, "{page}");
        assert_eq!(
            page.matches("No trend 28 days before to read it against.")
                .count(),
            2,
            "October alone has nothing 28 days before"
        );
        assert!(page.contains("Trend on 27 Oct · device unknown"), "{page}");
        assert!(!page.contains('±'), "a night is what happened");
        assert!(!page.contains(r#"class="band""#), "no band on either trend");
    }

    #[test]
    fn each_chart_draws_its_trend_and_regularity_skips_a_missing_night() {
        let page = shown(&october(), &NO_PLAN, Over::All);

        assert!(
            page.contains("14 Oct</span> · 83.3 · device unknown"),
            "{page}"
        );
        assert!(
            !page.contains("20 Oct</span> · 100") && !page.contains("21 Oct</span> · 100"),
            "a missing night has no index, nor does the morning after it"
        );
        assert_eq!(page.matches(r#"class="line""#).count(), 2, "{page}");
        assert_eq!(
            page.matches(r#"<polygon class="reference""#).count(),
            1 + 4,
            "the NHS's band, and the four fifths above the lowest"
        );
    }

    #[test]
    fn the_clock_reads_from_the_evening_before() {
        let page = shown(&week(), &NO_PLAN, Over::All);
        assert!(
            page.contains(">22:00<") && page.contains(">06:00<"),
            "{page}"
        );
    }

    #[test]
    fn the_latest_nights_are_newest_first() {
        let page = shown(&week(), &NO_PLAN, Over::All);
        let at = |on: &str| {
            page.find(&format!(
                r#"<span class="on">{on}</span><span class="value">"#
            ))
            .expect(on)
        };
        assert!(
            at("27 Oct") < at("25 Oct") && at("25 Oct") < at("22 Oct"),
            "{page}"
        );
        assert!(page.contains("6 h 45 min · 00:12 to 06:57"), "{page}");
    }

    #[test]
    fn a_range_with_no_plan_says_so_and_nothing_recorded_is_said() {
        let page = shown(&week(), &NO_PLAN, Over::Mesocycle);
        assert!(page.contains("No mesocycle is planned for today."));
        assert!(page.contains(r#"href="/sleep?over=all""#));

        let ranges = Ranges {
            mesocycle: Some(Range {
                from: date(2026, 9, 1),
                to: date(2026, 9, 7),
            }),
            macrocycle: None,
        };
        let page = shown(&week(), &ranges, Over::Mesocycle);
        assert_eq!(page.matches("Nothing in this range.").count(), 3, "{page}");

        let page = shown(&[], &NO_PLAN, Over::All);
        assert!(page.contains("Nothing has been recorded."));
    }
}
