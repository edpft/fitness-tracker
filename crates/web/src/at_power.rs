//! Heart rate at a fixed power, on the detail template (#621, #688).
//!
//! **It opens on the summary Progress shows** ([`crate::summary`], #830):
//! heart rate at 140 W since the macrocycle's start, one card on both pages. On a
//! day no plan is in force there is no summary, and the page opens on its
//! chart.
//!
//! **One chart**, on the mock the operator reviewed on 2026-11-06: each ride a
//! dot, and each device's line over its band. Each device's line is its own
//! (§ 6), and the devices never overlapped, so a tap on a day names the one
//! that recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    heart_rate_at_power::{LINE, METHOD, Trended},
    range::{Over, Range, Ranges},
    resting::Day,
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Chart, Layer, Metric, Traced};

pub const PATH: &str = "/heart-rate";
pub const TITLE: &str = "Heart rate per power zone";

/// The days the cards are drawn over, or what to say instead.
fn range(trended: &[Trended], ranges: &Ranges, over: Over) -> Result<Range, Markup> {
    let recorded = Range::spanning(trended.iter().flat_map(|one| {
        one.rides
            .iter()
            .map(|ride| ride.on)
            .chain(one.constant.iter().copied())
    }));
    crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.")
}

/// Rides, over the range chosen.
fn rides_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
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
            "Dots: each ride's {METHOD}. Line: each device's {LINE}. Band: 95%."
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
            rides_card(trended, ranges, over),
            latest_card(trended, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// Rides.
    RidesChart,
    "heart-rate-rides",
    configurator::analyses::HeartRateAtPower,
    rides_card, over = crate::drawn::tests_over
);
crate::card::ranged_card!(
    /// The latest readings.
    Latest,
    "heart-rate-latest",
    configurator::analyses::HeartRateAtPower,
    latest_card, over = crate::drawn::tests_over
);

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

fn counted(count: usize, one: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{count} {one}s")
    }
}

/// Heart rate at a fixed power over a range (#621). It opens on All, as #621
/// settled.
#[derive(Debug)]
pub struct Page {
    over: Over,
}

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (crate::summary::AtPower, RidesChart, Latest);

    fn asked(
        _path: &axum::extract::RawPathParams,
        query: &std::collections::HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self {
            over: crate::drawn::tests_over(query),
        })
    }

    fn over(&self) -> Option<Over> {
        Some(self.over)
    }

    fn layout(&self, cards: [Markup; 3]) -> Markup {
        let [summary, rides, latest] = cards;
        laid_out(self.over, &[crate::summary::atop(&summary), rides, latest])
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
                    .to_zoned_datetime(zone.as_time_zone())
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
        super::page(trended, &NO_PLAN, Over::All).into_string()
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
