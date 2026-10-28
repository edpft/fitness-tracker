//! A night's sleep, as two charts: how long, and when (#255).
//!
//! **Both by the morning Garmin reports the night under**, one mark per night
//! and one colour per watch, as [`crate::daily`]'s are. A night with no sleep
//! recorded is the empty stretch it is (§ 37), and nothing joins two nights.
//!
//! **When is a bar from falling asleep to waking**, on a clock that runs down
//! the page from the evening before to the morning's noon, so a later night
//! sits lower. That is the regularity series' data point; a figure over the
//! windows is the trend's (#418).

use domain::analytical::{
    per_device::DeviceSeries,
    range::{Over, Range, Ranges},
    sleep::Night,
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

/// The page: each watch's nights over the range chosen.
pub fn page(nights: &[DeviceSeries<Night>], ranges: &Ranges, over: Over, run: &View) -> Markup {
    let range = crate::range::chosen(ranges, over, recorded(nights.iter()), NOTHING);
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (TITLE) }
                style { (crate::page::STYLE) (crate::chart::STYLE) (crate::daily::STYLE) (STYLE) }
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

/// Both charts over `range`, the legend and the table.
fn charts(nights: &[DeviceSeries<Night>], range: Range) -> Markup {
    let within: Vec<DeviceSeries<Night>> = nights.iter().map(|one| one.within(range)).collect();
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
        div.plot { (asleep(&within, &dates)) }
        h2 { "Fell asleep to woke" }
        div.plot { (window(&within, &dates)) }
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

/// Time asleep, 0 h on the floor, in whole two hours.
fn asleep(series: &[DeviceSeries<Night>], dates: &Dates) -> Markup {
    let longest = series
        .iter()
        .flat_map(|one| {
            one.readings
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
            aria-label="Time asleep, one point per night, by date" {
            @for hours in marks {
                @let y = rising(hours, 0.0, high);
                line.grid x1=(LEFT) x2=(WIDTH - RIGHT) y1=(y) y2=(y) {}
                text.axis x=(LEFT - 6.0) y=(y + 4.0) text-anchor="end" { (format!("{hours} h")) }
            }
            (dates.grid())
            @for (index, one) in series.iter().enumerate() {
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
        }
    }
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
";

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            per_device::{DeviceSeries, Reading, SentBy},
            range::{Over, Range, Ranges},
            sleep::Night,
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

    fn week() -> [DeviceSeries<Night>; 1] {
        [DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: vec![
                night(22, 29_040, (21, 22, 26), (6, 30)),
                night(25, 28_380, (24, 22, 23), (6, 16)),
                night(27, 24_300, (27, 0, 12), (6, 57)),
            ],
        }]
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
