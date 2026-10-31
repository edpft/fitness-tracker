//! A night's sleep, as three charts: how long, when, and how regularly (#255,
//! #418).
//!
//! **Both by the morning Garmin reports the night under**, one mark per night
//! and one colour per watch, as [`crate::daily`]'s are. A night with no sleep
//! recorded is the empty stretch it is (§ 37), and nothing joins two nights.
//!
//! **When is a bar from falling asleep to waking**, on a clock that runs down
//! the page from the evening before to the morning's noon, so a later night
//! sits lower. That is the regularity series' data point.
//!
//! **Each figure's trend is drawn over its published band**, the NHS's for
//! time asleep and Windred et al.'s fifths for regularity, and the page says
//! where each watch's latest trend sits and which way it is going.

use domain::analytical::{
    body_weight::Direction,
    hrv::Against,
    per_device::DeviceSeries,
    range::{Over, Range, Ranges},
    sleep::{self, Fifth, Night, Trend, Trended},
};
use jiff::civil::{Date, DateTime};
use maud::{DOCTYPE, Markup, html};

use crate::{
    chart::along,
    daily::{NOTHING, class, mark, name, recorded},
    range::{seconds, ticks},
    run::View,
};

pub const PATH: &str = "/sleep";
pub const TITLE: &str = "Sleep";

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 300.0;
const LEFT: f64 = 48.0;
const RIGHT: f64 = 12.0;
const TOP: f64 = 12.0;
const FLOOR: f64 = 276.0;

/// Seconds in an hour, for the axes.
const HOUR: f64 = 3600.0;

/// The page: each watch's nights and their trends over the range chosen.
pub fn page(nights: &[Trended], ranges: &Ranges, over: Over, run: &View) -> Markup {
    let range = crate::range::chosen(
        ranges,
        over,
        recorded(nights.iter().map(|one| &one.series)),
        NOTHING,
    );
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                (crate::page::head(TITLE, &[crate::chart::STYLE, crate::daily::STYLE, STYLE]))
            }
            body {
                (crate::page::following(run))
                main.wide {
                    (crate::page::menu(PATH))
                    h1 { (TITLE) }
                    p.method { "One mark per night, on the morning Garmin reports it under." }
                    (crate::range::choices(PATH, over))
                    @match range {
                        Err(said) => (said),
                        Ok(range) => (charts(nights, range)),
                    }
                }
            }
        }
    }
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    crate::daily::unreadable(
        crate::daily::Shown {
            title: TITLE,
            path: PATH,
            point: "",
        },
        error,
    )
}

/// The charts over `range`, the legend and the table.
fn charts(nights: &[Trended], range: Range) -> Markup {
    let trended: Vec<Trended> = nights.iter().map(|one| one.within(range)).collect();
    let within: Vec<DeviceSeries<Night>> = trended.iter().map(|one| one.series.clone()).collect();
    if within.iter().all(|one| one.readings.is_empty()) {
        return html! { p { "Nothing recorded in this range." } };
    }
    let dates = Dates {
        first: seconds(range.from),
        last: seconds(range.to),
        range,
    };
    html! {
        h2 { "Time asleep" }
        @for one in &trended {
            @if let Some(latest) = latest_asleep(one) { p.reading { (latest) } }
        }
        div.plot { (asleep(&trended, &dates)) }
        p.method {
            "points: each night · line: the " (sleep::METHOD)
            " · shaded: the NHS's 7 to 9 hours for adults · against 28 days before: rising or falling beyond 15 minutes, maintaining within it"
        }
        h2 { "Fell asleep to woke" }
        div.plot { (window(&within, &dates)) }
        h2 { "Regularity" }
        @for one in &trended {
            @if let Some(latest) = latest_regular(one) { p.reading { (latest) } }
        }
        div.plot { (regularity(&trended, &dates)) }
        p.method {
            "points: each morning's Sleep Regularity Index against the night before, from falling asleep to waking · line: the "
            (sleep::METHOD)
            " · shaded: the fifths of Windred et al. (Sleep, 2024), 7 days of wrist actigraphy in 61,000 UK adults, among whom higher was better throughout · against 28 days before: rising or falling beyond 2, maintaining within it"
        }
        ul.legend {
            @for (index, one) in within.iter().enumerate().filter(|(_, one)| !one.readings.is_empty()) {
                li {
                    svg.key viewBox="0 0 12 12" { (mark(class(index, &one.sent_by), &one.sent_by, 6.0, 6.0, "")) }
                    (name(&one.sent_by)) " · " (one.readings.len()) (if one.readings.len() == 1 { " night" } else { " nights" })
                }
            }
        }
        (table(&within))
    }
}

/// Where a morning falls across the chart.
struct Dates {
    range: Range,
    first: i64,
    last: i64,
}

impl Dates {
    fn x(&self, on: Date) -> f64 {
        (WIDTH - LEFT - RIGHT).mul_add(
            along(seconds(on) - self.first, self.last - self.first),
            LEFT,
        )
    }

    fn grid(&self) -> Markup {
        html! {
            @for (tick, label) in ticks(self.range) {
                line.grid x1=(self.x(tick)) x2=(self.x(tick)) y1=(TOP) y2=(FLOOR) {}
                text.axis x=(self.x(tick) + 3.0) y=(FLOOR + 14.0) { (label) }
            }
            line.baseline x1=(LEFT) x2=(WIDTH - RIGHT) y1=(FLOOR) y2=(FLOOR) {}
        }
    }
}

/// Hours up the chart from `low` to `high`, `low` on the floor.
fn rising(hours: f64, low: f64, high: f64) -> f64 {
    (FLOOR - TOP).mul_add(-(hours - low) / (high - low), FLOOR)
}

/// Hours down the chart from `low` to `high`, `low` at the top.
fn falling(hours: f64, low: f64, high: f64) -> f64 {
    (FLOOR - TOP).mul_add((hours - low) / (high - low), TOP)
}

fn hours_asleep(night: &Night) -> f64 {
    let seconds = u32::try_from(night.asleep.as_seconds()).unwrap_or(u32::MAX);
    f64::from(seconds) / HOUR
}

/// A wall clock as hours from the morning's midnight: the evening before is
/// negative.
fn from_midnight(at: DateTime, morning: Date) -> f64 {
    at.duration_since(morning.to_datetime(jiff::civil::Time::midnight()))
        .as_secs_f64()
        / HOUR
}

/// Whole hours, as a clock reads them.
fn clock(hours: f64) -> String {
    format!("{:02}:00", hours.rem_euclid(24.0))
}

/// A length of time asleep, in hours and minutes.
fn length(night: &Night) -> String {
    let minutes = night.asleep.as_seconds() / 60;
    format!("{} h {:02} min", minutes / 60, minutes % 60)
}

/// Time asleep, 0 h on the floor, in whole two hours, over the NHS's band.
fn asleep(trended: &[Trended], dates: &Dates) -> Markup {
    let longest = trended
        .iter()
        .flat_map(|one| {
            one.series
                .readings
                .iter()
                .map(|reading| hours_asleep(&reading.value))
        })
        .fold(0.0_f64, f64::max);
    let high = ((longest / 2.0).ceil() * 2.0).max(12.0);
    let marks = (0_u8..)
        .map(|step| f64::from(step) * 2.0)
        .take_while(|hours| *hours <= high);
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Time asleep, one point per night, by date, and the trend through them" {
            @let (nhs_low, nhs_high) = sleep::NHS;
            rect.band x=(LEFT) width=(WIDTH - LEFT - RIGHT)
                y=(rising(nhs_high / 60.0, 0.0, high))
                height=(rising(nhs_low / 60.0, 0.0, high) - rising(nhs_high / 60.0, 0.0, high)) {}
            @for hours in marks {
                @let y = rising(hours, 0.0, high);
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(y) y2=(y) {}
                text.axis x=(LEFT - 6.0) y=(y + 4.0) text-anchor="end" { (format!("{hours} h")) }
            }
            (dates.grid())
            @for (index, one) in trended.iter().map(|one| &one.series).enumerate() {
                @for reading in &one.readings {
                    (mark(
                        class(index, &one.sent_by),
                        &one.sent_by,
                        dates.x(reading.on),
                        rising(hours_asleep(&reading.value), 0.0, high),
                        &format!("{} · asleep {} · {}", reading.on, length(&reading.value), name(&one.sent_by)),
                    ))
                }
            }
            @for one in trended {
                (line(&one.asleep, dates, |minutes| rising(minutes / 60.0, 0.0, high), |day| {
                    format!("{} · {} · {}", day.on, asleep_reading(day), name(&one.series.sent_by))
                }))
            }
        }
    }
}

/// Each morning's SRI and its trend, over Windred et al.'s fifths, 100 at
/// the top and the floor low enough for every morning in 20s.
fn regularity(trended: &[Trended], dates: &Dates) -> Markup {
    let lowest = trended
        .iter()
        .flat_map(|one| one.regularity.values())
        .fold(100.0_f64, |low, index| low.min(*index));
    let low = ((lowest / 20.0).floor() * 20.0).min(60.0);
    let y = |index: f64| rising(index, low, 100.0);
    let edges: Vec<f64> = std::iter::once(low)
        .chain(sleep::WINDRED)
        .chain(std::iter::once(100.0))
        .collect();
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Sleep Regularity Index, one point per morning, by date, and the trend through them" {
            @for (fifth, pair) in edges.windows(2).enumerate() {
                @if let [from, to] = pair {
                    rect class=(format!("fifth f{fifth}")) x=(LEFT) width=(WIDTH - LEFT - RIGHT)
                        y=(y(*to)) height=(y(*from) - y(*to)) {}
                }
            }
            @for edge in &edges {
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(y(*edge)) y2=(y(*edge)) {}
                text.axis x=(LEFT - 6.0) y=(y(*edge) + 4.0) text-anchor="end" { (edge) }
            }
            (dates.grid())
            @for (index, one) in trended.iter().enumerate() {
                @for (on, value) in &one.regularity {
                    (mark(
                        class(index, &one.series.sent_by),
                        &one.series.sent_by,
                        dates.x(*on),
                        y(*value),
                        &format!("{on} · {value:.1} against the night before · {}", name(&one.series.sent_by)),
                    ))
                }
            }
            @for one in trended {
                (line(&one.regular, dates, y, |day| {
                    format!("{} · {} · {}", day.on, regular_reading(day), name(&one.series.sent_by))
                }))
            }
        }
    }
}

/// A trend's unbroken runs, each a line, with a title on every day.
fn line(
    runs: &[Vec<Trend>],
    dates: &Dates,
    y: impl Fn(f64) -> f64,
    title: impl Fn(&Trend) -> String,
) -> Markup {
    html! {
        @for run in runs {
            polyline.trend points=(run.iter().map(|day| format!("{:.1},{:.1}", dates.x(day.on), y(day.value))).collect::<Vec<_>>().join(" ")) {}
            @for day in run {
                circle.hover cx=(dates.x(day.on)) cy=(y(day.value)) r="3" { title { (title(day)) } }
            }
        }
    }
}

/// Which way a trend went, and by how much, in `unit`.
fn moved(day: &Trend, by: impl Fn(f64) -> String) -> String {
    match (day.direction, day.change) {
        (Some(direction), Some(change)) => format!(
            "{} ({} against 28 days before)",
            match direction {
                Direction::Rising => "rising",
                Direction::Maintaining => "maintaining",
                Direction::Falling => "falling",
            },
            by(change)
        ),
        _ => "no direction yet".to_owned(),
    }
}

/// A day's trend in time asleep, against the NHS's band.
fn asleep_reading(day: &Trend) -> String {
    let against = match sleep::against_nhs(day.value) {
        Against::Below => "below",
        Against::Within => "within",
        Against::Above => "above",
    };
    format!(
        "last 28 nights {}, {against} the NHS's 7 to 9 hours, {}",
        minutes(day.value),
        moved(day, |change| {
            let sign = if change < 0.0 { "−" } else { "+" };
            format!("{sign}{:.0} min", change.abs())
        })
    )
}

/// A day's trend in regularity, against Windred et al.'s fifths.
fn regular_reading(day: &Trend) -> String {
    let fifth = match Fifth::of(day.value) {
        Fifth::Lowest => "lowest",
        Fifth::Second => "second",
        Fifth::Third => "middle",
        Fifth::Fourth => "fourth",
        Fifth::Highest => "highest",
    };
    format!(
        "last 28 nights {:.1}, in Windred et al.'s {fifth} fifth, {}",
        day.value,
        moved(day, |change| {
            let sign = if change < 0.0 { "−" } else { "+" };
            format!("{sign}{:.1}", change.abs())
        })
    )
}

fn latest_asleep(one: &Trended) -> Option<String> {
    let day = one.asleep.iter().flatten().next_back()?;
    Some(format!(
        "{}, {}: {}",
        name(&one.series.sent_by),
        day.on,
        asleep_reading(day)
    ))
}

fn latest_regular(one: &Trended) -> Option<String> {
    let day = one.regular.iter().flatten().next_back()?;
    Some(format!(
        "{}, {}: {}",
        name(&one.series.sent_by),
        day.on,
        regular_reading(day)
    ))
}

/// Minutes as hours and minutes, to the nearest minute.
fn minutes(minutes: f64) -> String {
    let whole = jiff::SignedDuration::try_from_secs_f64(minutes.round() * 60.0)
        .map_or(0, |duration| duration.as_mins());
    format!("{} h {:02} min", whole / 60, whole % 60)
}

/// Fell asleep to woke, 18:00 the evening before at the top and noon at the
/// foot, widened in three hours to hold every night.
fn window(series: &[DeviceSeries<Night>], dates: &Dates) -> Markup {
    let ends = series.iter().flat_map(|one| {
        one.readings.iter().flat_map(|reading| {
            [
                from_midnight(reading.value.fell_asleep, reading.on),
                from_midnight(reading.value.woke, reading.on),
            ]
        })
    });
    let earliest = ends.clone().fold(-6.0_f64, f64::min);
    let latest = ends.fold(12.0_f64, f64::max);
    let low = (earliest / 3.0).floor() * 3.0;
    let high = (latest / 3.0).ceil() * 3.0;
    let marks = (0_u8..)
        .map(|step| f64::from(step).mul_add(3.0, low))
        .take_while(|hours| *hours <= high);
    html! {
        svg.chart viewBox=(format!("0 0 {WIDTH} {HEIGHT}")) role="img"
            aria-label="Fell asleep to woke, one bar per night, by date" {
            @for hours in marks {
                @let y = falling(hours, low, high);
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(y) y2=(y) {}
                text.axis x=(LEFT - 6.0) y=(y + 4.0) text-anchor="end" { (clock(hours)) }
            }
            (dates.grid())
            @for (index, one) in series.iter().enumerate() {
                @for reading in &one.readings {
                    @let x = dates.x(reading.on);
                    line class=(format!("slept {}", class(index, &one.sent_by)))
                        x1=(x) x2=(x)
                        y1=(falling(from_midnight(reading.value.fell_asleep, reading.on), low, high))
                        y2=(falling(from_midnight(reading.value.woke, reading.on), low, high)) {
                        title {
                            (format!(
                                "{} · fell asleep {}, woke {} · {}",
                                reading.on,
                                reading.value.fell_asleep.strftime("%H:%M"),
                                reading.value.woke.strftime("%H:%M"),
                                name(&one.sent_by)
                            ))
                        }
                    }
                }
            }
        }
    }
}

/// The same nights, for whoever cannot tell two colours apart.
fn table(series: &[DeviceSeries<Night>]) -> Markup {
    let mut rows: Vec<(Date, Night, String)> = series
        .iter()
        .flat_map(|one| {
            one.readings
                .iter()
                .map(|reading| (reading.on, reading.value, name(&one.sent_by)))
        })
        .collect();
    rows.sort_by_key(|(on, ..)| *on);
    html! {
        details {
            summary { "Every night as a table" }
            table.figures {
                thead { tr { th { "morning" } th { "asleep" } th { "fell asleep" } th { "woke" } th { "device" } } }
                tbody {
                    @for (on, night, device) in rows {
                        tr {
                            td { (on) }
                            td { (length(&night)) }
                            td { (night.fell_asleep.strftime("%Y-%m-%d %H:%M")) }
                            td { (night.woke.strftime("%H:%M")) }
                            td { (device) }
                        }
                    }
                }
            }
        }
    }
}

/// A night's bar in its watch's colour; a night no watch can be placed on in
/// ink.
const STYLE: &str = "
.slept { stroke-width: 2; stroke-linecap: butt; }
.slept.d1 { stroke: var(--z1); } .slept.d2 { stroke: var(--z6); }
.slept.d3 { stroke: var(--z5); } .slept.d4 { stroke: var(--z2); }
.slept.ink { stroke: var(--ink); }
.fifth { fill: var(--ink); stroke: none; }
.f0 { fill-opacity: 0; } .f1 { fill-opacity: 0.03; } .f2 { fill-opacity: 0.06; }
.f3 { fill-opacity: 0.09; } .f4 { fill-opacity: 0.12; }
";

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

    #[test]
    fn a_night_is_a_point_and_a_bar_and_a_missing_night_is_neither() {
        let page =
            super::page(&week(), &NO_PLAN, Over::All, &crate::run::View::default()).into_string();

        assert!(page.contains("2026-10-22 · asleep 8 h 04 min · device unknown"));
        assert!(page.contains("2026-10-22 · fell asleep 22:26, woke 06:30 · device unknown"));
        assert!(page.contains("2026-10-27 · fell asleep 00:12, woke 06:57"));
        assert!(
            !page.contains("2026-10-26 ·"),
            "no sleep was recorded that night"
        );
        assert_eq!(page.matches(r#"class="slept ink""#).count(), 3);
        assert!(!page.contains("<polyline"), "nothing joins two nights");
        assert!(page.contains("device unknown · 3 nights"));
    }

    #[test]
    fn each_trend_is_read_against_its_band() {
        let page = super::page(
            &october(),
            &NO_PLAN,
            Over::All,
            &crate::run::View::default(),
        )
        .into_string();

        assert!(page.contains(
            "device unknown, 2026-10-27: last 28 nights 7 h 30 min, within the NHS's 7 to 9 hours, no direction yet"
        ));
        assert!(page.contains("device unknown, 2026-10-27: last 28 nights 99.3, in Windred et al.'s highest fifth, no direction yet"));
        assert!(page.contains("2026-10-14 · 83.3 against the night before"));
        assert!(
            !page.contains("2026-10-20 · 100") && !page.contains("2026-10-21 · 100"),
            "a missing night has no index, nor does the morning after it"
        );
        assert_eq!(page.matches(r#"class="trend""#).count(), 2);
        assert_eq!(page.matches(r#"class="band""#).count(), 1);
    }

    #[test]
    fn the_clock_runs_from_the_evening_before_to_noon() {
        let page =
            super::page(&week(), &NO_PLAN, Over::All, &crate::run::View::default()).into_string();
        assert!(page.contains(">18:00<") && page.contains(">12:00<"));
        assert!(page.contains(">00:00<"));
    }

    #[test]
    fn a_range_with_no_plan_says_so_and_nothing_recorded_is_said() {
        let run = crate::run::View::default();
        let page = super::page(&week(), &NO_PLAN, Over::Mesocycle, &run).into_string();
        assert!(page.contains("No mesocycle is planned for today."));
        assert!(page.contains(r#"href="/sleep?over=all""#));

        let ranges = Ranges {
            mesocycle: Some(Range {
                from: date(2026, 9, 1),
                to: date(2026, 9, 7),
            }),
            macrocycle: None,
        };
        let page = super::page(&week(), &ranges, Over::Mesocycle, &run).into_string();
        assert!(page.contains("Nothing recorded in this range."));

        let page = super::page(&[], &NO_PLAN, Over::All, &run).into_string();
        assert!(page.contains("Nothing has been recorded."));
    }
}
