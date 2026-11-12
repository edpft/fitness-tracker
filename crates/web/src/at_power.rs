//! Heart rate at a fixed power, on the detail template (#621, #688).
//!
//! **The summary is the latest 42-day mean, its ±, and its change on the 42
//! days before**, read against the noise of the two: the arrow only where the
//! change is beyond it. It is the latest whatever the range: the range only
//! chooses which rides the chart shows.
//!
//! **One chart**, on the mock the operator reviewed on 2026-11-06: each ride a
//! dot, and each device's line over its band. Each device's line is its own
//! (§ 6), and the devices never overlapped, so a tap on a day names the one
//! that recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    heart_rate_at_power::{LINE, METHOD, Trended},
    range::{Over, Range, Ranges},
    resting::{Change, Day},
};
use jiff::civil::Date;
use maud::{DOCTYPE, Markup, html};

use crate::{
    detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Traced},
    run::View,
};

pub const PATH: &str = "/heart-rate";
pub const TITLE: &str = "Heart rate per power zone";

/// The page: the summary, then the chart over the range chosen.
pub fn page(trended: &[Trended], ranges: &Ranges, over: Over, run: &View) -> Markup {
    detail::page(PATH, TITLE, Some(over), run, &body(trended, ranges, over))
}

/// The page when the store could not be read.
pub fn unreadable(error: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head { (crate::page::head(TITLE, &[])) }
            body {
                main {
                    (crate::page::bar(PATH))
                    p.failed { "the store could not be read: " (error) }
                }
            }
        }
    }
}

/// The summary, the chart over the range chosen and the latest rides, or why
/// there is no range.
fn body(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
    let recorded = Range::spanning(trended.iter().flat_map(|one| {
        one.rides
            .iter()
            .map(|ride| ride.on)
            .chain(one.constant.iter().copied())
    }));
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
            "Dots: each ride's {METHOD}. Line: each device's {LINE}. Band: 95%."
        )))
    }
}

/// The device whose latest ride is the latest, the line on that ride, and its
/// change on the 42 days before.
fn latest_line(trended: &[Trended]) -> Option<(&Trended, Option<Day>, Option<Change>)> {
    trended
        .iter()
        .filter_map(|one| {
            let last = one.rides.last()?;
            Some((one, last.on, one.window(last.on), one.change()))
        })
        .max_by_key(|(_, on, ..)| *on)
        .map(|(one, _, day, change)| (one, day, change))
}

/// The latest line, its ±, and its change, read against its noise.
fn summary(trended: &[Trended], today: Date) -> Markup {
    let Some((one, day, change)) = latest_line(trended) else {
        return html! {};
    };
    let Some(day) = day else {
        return detail::summary(
            &[Cell {
                metric: Metric::HeartRateAtPower,
                label: "Heart rate at 140 W",
                figure: "—".to_owned(),
                unit: "bpm",
                margin: None,
                readings: vec![Reading::NoRule(
                    "Fewer than 3 rides in the last 42 days.".to_owned(),
                )],
            }],
            one.device.name().as_str(),
        );
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
            over: "in 42 days".to_owned(),
        },
        Some(_) => Reading::NoRule("Too few rides to measure the noise.".to_owned()),
        None => Reading::NoRule("Fewer than 3 rides in the 42 days before.".to_owned()),
    };
    let asof = format!(
        "Line on {} · {}",
        detail::day(day.on, today),
        one.device.name()
    );
    detail::summary(
        &[Cell {
            metric: Metric::HeartRateAtPower,
            label: "Heart rate at 140 W",
            figure: format!("{:.1}", day.bpm),
            unit: "bpm",
            margin: day.margin.map(|margin| format!("{margin:.1}")),
            readings: vec![reading],
        }],
        &asof,
    )
}

/// Each ride, and each device's line over its band.
fn chart(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut dots: Vec<(Date, f64)> = Vec::new();
    for one in within {
        let device = one.device.name().to_string();
        for ride in &one.rides {
            dots.push((ride.on, ride.bpm));
            said.entry(ride.on)
                .or_default()
                .push(format!("{:.1} bpm · {device}", ride.bpm));
        }
        for on in &one.constant {
            said.entry(*on)
                .or_default()
                .push(format!("heart rate did not change · {device}"));
        }
        for day in one.line.iter().flatten() {
            let line = day.margin.map_or_else(
                || format!("line {:.1}", day.bpm),
                |margin| format!("line {:.1} ± {margin:.1}", day.bpm),
            );
            said.entry(day.on).or_default().push(line);
        }
    }
    let sub = format!("{} · bpm at 140 W", counted(dots.len(), "ride"));
    Chart {
        id: "at-power",
        metric: Metric::HeartRateAtPower,
        title: "Heart rate at 140 W",
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
                    value: day.bpm,
                    margin: day.margin,
                })
                .collect()
        })
        .collect()
}

/// The latest seven rides, newest first.
fn latest(trended: &[Trended], today: Date) -> Markup {
    let mut every: Vec<(Date, Option<f64>)> = trended
        .iter()
        .flat_map(|one| {
            one.rides
                .iter()
                .map(|ride| (ride.on, Some(ride.bpm)))
                .chain(one.constant.iter().map(|on| (*on, None)))
        })
        .collect();
    every.sort_by_key(|(on, _)| *on);
    let rows: Vec<(String, Markup)> = every
        .iter()
        .rev()
        .take(7)
        .map(|(on, bpm)| {
            (
                detail::day(*on, today),
                bpm.map_or_else(
                    || html! { "heart rate did not change" },
                    |bpm| html! { (format!("{bpm:.1}")) " bpm" },
                ),
            )
        })
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Rides", "at 140 W", &rows)
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
    use std::collections::BTreeSet;

    use domain::{
        analytical::{
            heart_rate_at_power::{Ride, Trended},
            range::{Over, Ranges},
            resting::Day,
        },
        device::{
            Device, DeviceIdentifier, DeviceName, InUse, Measurand, Role, SerialNumber, Sighting,
            Wear, register,
        },
        normalised::{OperatorZone, StartedAt},
    };
    use jiff::{
        ToSpan as _,
        civil::{Date, date},
    };

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn sighting(on: Date) -> Sighting {
        let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
        Sighting {
            device: DeviceIdentifier::Serial(
                SerialNumber::try_from("3483897389".to_owned()).expect("a serial"),
            ),
            also: Vec::new(),
            name: DeviceName::try_from("Forerunner 165").expect("a name"),
            role: Role::Recorder,
            measured: BTreeSet::from([Measurand::HeartRate]),
            sent: BTreeSet::from([Measurand::HeartRate]),
            at: StartedAt::new(
                on.at(12, 0, 0, 0)
                    .to_zoned(zone.as_time_zone())
                    .expect("a time")
                    .timestamp(),
                zone,
            ),
            source: "Garmin",
            worn: false,
        }
    }

    fn forerunner() -> Device {
        let register = register(
            &[sighting(date(2026, 1, 1)), sighting(date(2026, 12, 31))],
            Wear::default(),
        );
        match register.sent_by(Measurand::HeartRate, date(2026, 6, 1)) {
            InUse::One(device) => device.clone(),
            InUse::Several(_) | InUse::Unknown => panic!("one watch"),
        }
    }

    /// Rides every fourth day from 1 August, each the figure given, and the
    /// line through them as the domain draws it.
    fn rides(figures: &[f64], constant: &[Date]) -> [Trended; 1] {
        let rides: Vec<Ride> = figures
            .iter()
            .zip(0_i32..)
            .map(|(bpm, n)| Ride {
                on: date(2026, 8, 1) + (4 * n).days(),
                bpm: *bpm,
            })
            .collect();
        let mut trended = Trended {
            device: forerunner(),
            rides,
            constant: constant.to_vec(),
            line: Vec::new(),
            noise: Some(5.5),
            per_watt: 0.36,
            per_minute: 0.24,
        };
        let line: Vec<Day> = trended
            .rides
            .iter()
            .filter_map(|ride| trended.window(ride.on))
            .collect();
        trended.line = vec![line];
        [trended]
    }

    fn shown(trended: &[Trended]) -> String {
        super::page(trended, &NO_PLAN, Over::All, &crate::run::View::default()).into_string()
    }

    #[test]
    fn the_summary_is_the_last_42_days_its_band_and_its_change_on_the_42_before() {
        // 140 for twelve rides, then 128 for eleven: the last 42 days are
        // 128.0 against 140.0, a fall far beyond the noise.
        let mut figures = vec![140.0; 12];
        figures.extend([128.0; 11]);
        let page = shown(&rides(&figures, &[]));

        assert!(
            page.contains(r#"class="cell heart-rate-at-power""#),
            "{page}"
        );
        assert!(page.contains("Heart rate at 140 W"), "{page}");
        assert!(page.contains("128.0<small>bpm</small>"), "{page}");
        assert!(page.contains("−12.0 bpm"), "{page}");
        assert!(page.contains("in 42 days"), "{page}");
        assert!(page.contains(r#"class="badge""#), "falling, with its arrow");
        assert!(page.contains("Line on 28 Oct · Forerunner 165"), "{page}");
    }

    #[test]
    fn a_change_within_the_noise_has_no_arrow() {
        let figures: Vec<f64> = (0..23)
            .map(|n| if n % 2 == 0 { 138.0 } else { 141.0 })
            .collect();
        let page = shown(&rides(&figures, &[]));

        assert!(page.contains(r#"class="change plain""#), "{page}");
        assert!(!page.contains(r#"class="badge"#), "{page}");
    }

    #[test]
    fn fewer_than_three_rides_in_the_42_days_before_is_said() {
        let page = shown(&rides(&[140.0; 6], &[]));
        assert!(
            page.contains("Fewer than 3 rides in the 42 days before."),
            "{page}"
        );
    }

    #[test]
    fn a_ride_is_a_dot_and_a_stuck_sensor_is_said_not_drawn() {
        let stuck = date(2026, 8, 3);
        let page = shown(&rides(&[140.0, 138.5, 141.0], &[stuck]));

        assert_eq!(page.matches(r#"class="dot""#).count(), 3, "{page}");
        assert!(
            page.contains("1 Aug</span> · 140.0 bpm · Forerunner 165"),
            "{page}"
        );
        assert!(page.contains("heart rate did not change"), "{page}");
    }

    #[test]
    fn nothing_recorded_is_said() {
        assert!(shown(&[]).contains("Nothing has been recorded."));
    }
}
