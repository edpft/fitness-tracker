//! Overnight HRV, on the detail template (#622, #417).
//!
//! **It opens on the summary Progress shows** ([`crate::summary`], #830):
//! overnight HRV since the macrocycle's start
//! against the expected result of the training done, one card on both pages. On a
//! day no plan is in force there is no summary, and the page opens on its
//! chart.
//!
//! **No ± on the trend** (#616): the band it is read against is already the
//! noise.
//!
//! **One chart**: each night a dot, each watch's trend, and the band in
//! neutral grey. Each watch's trend is its own (§ 6); the watches never
//! overlapped, so a tap on a night names the one that recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    hrv::{BAND, Day, METHOD, Trended},
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Chart, Layer, Metric, Traced, name};

pub const PATH: &str = "/hrv";
pub const TITLE: &str = "Overnight HRV";

/// The days the cards are drawn over, or what to say instead.
fn range(trended: &[Trended], ranges: &Ranges, over: Over) -> Result<Range, Markup> {
    let recorded = Range::spanning(
        trended
            .iter()
            .flat_map(|one| one.series.readings.iter().map(|reading| reading.on)),
    );
    crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.")
}

/// Nights, over the range chosen.
fn nights_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
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
            "Dots: Garmin's average rMSSD for each night. Line: each watch's {METHOD}. Grey: {BAND}."
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
            nights_card(trended, ranges, over),
            latest_card(trended, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// Nights.
    NightsChart,
    "hrv-nights",
    configurator::analyses::OvernightHrv,
    nights_card
);
crate::card::ranged_card!(
    /// The latest readings.
    Latest,
    "hrv-latest",
    configurator::analyses::OvernightHrv,
    latest_card
);

/// Each night, each watch's trend, and its band in grey, one region per
/// unbroken stretch of it.
fn chart(within: &[Trended], range: Range, today: Date) -> Chart<'static> {
    let mut said: BTreeMap<Date, Vec<String>> = BTreeMap::new();
    let mut dots: Vec<(Date, f64)> = Vec::new();
    let mut bands: Vec<Vec<(Date, f64, f64)>> = Vec::new();
    for one in within {
        let sent_by = name(&one.series.sent_by);
        for reading in &one.series.readings {
            let milliseconds = reading.value.as_milliseconds();
            dots.push((reading.on, f64::from(milliseconds)));
            said.entry(reading.on)
                .or_default()
                .push(format!("{milliseconds} ms · {sent_by}"));
        }
        for run in &one.trend {
            let mut stretch: Vec<(Date, f64, f64)> = Vec::new();
            for day in run {
                let entry = said.entry(day.on).or_default();
                entry.push(format!("7 nights {:.0} ms", day.milliseconds));
                if let Some(band) = day.band {
                    stretch.push((day.on, band.low, band.high));
                    entry.push(format!("band {:.0}–{:.0} ms", band.low, band.high));
                } else if !stretch.is_empty() {
                    bands.push(std::mem::take(&mut stretch));
                }
            }
            if !stretch.is_empty() {
                bands.push(stretch);
            }
        }
    }
    let sub = format!("{} · ms", counted(dots.len()));
    Chart {
        id: "hrv",
        metric: Metric::Hrv,
        title: "Overnight HRV",
        sub,
        range,
        axis: Axis::Figures,
        layers: bands
            .into_iter()
            .map(Layer::Reference)
            .chain([
                Layer::Trend(within.iter().flat_map(|one| traced(&one.trend)).collect()),
                Layer::Dots(dots),
            ])
            .collect(),
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
                    value: day.milliseconds,
                    margin: None,
                })
                .collect()
        })
        .collect()
}

/// The latest seven nights, newest first.
fn latest(trended: &[Trended], today: Date) -> Markup {
    let mut every: Vec<(Date, u32)> = trended
        .iter()
        .flat_map(|one| {
            one.series
                .readings
                .iter()
                .map(|reading| (reading.on, reading.value.as_milliseconds()))
        })
        .collect();
    every.sort_by_key(|(on, _)| *on);
    let rows: Vec<(String, Markup)> = every
        .iter()
        .rev()
        .take(7)
        .map(|(on, milliseconds)| (detail::day(*on, today), html! { (milliseconds) " ms" }))
        .collect();
    if rows.is_empty() {
        return html! {};
    }
    detail::latest("Nights", "ms", &rows)
}

fn counted(count: usize) -> String {
    if count == 1 {
        "1 night".to_owned()
    } else {
        format!("{count} nights")
    }
}

/// Overnight HRV over a range, one series per watch and the trend through
/// each.
#[derive(Debug)]
pub struct Page {
    over: Over,
}

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (crate::summary::OvernightHrv, NightsChart, Latest);

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
        let [summary, nights, latest] = cards;
        laid_out(self.over, &[crate::summary::atop(&summary), nights, latest])
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            hrv::Trended,
            per_device::{DeviceSeries, Reading, SentBy},
            range::{Over, Range, Ranges},
        },
        measure::HeartRateVariability,
    };
    use jiff::civil::{Date, date};

    const NO_PLAN: Ranges = Ranges {
        mesocycle: None,
        macrocycle: None,
    };

    fn trended(nights: impl IntoIterator<Item = (Date, u32)>) -> [Trended; 1] {
        [Trended::of(DeviceSeries {
            sent_by: SentBy::Unknown,
            readings: nights
                .into_iter()
                .map(|(on, milliseconds)| Reading {
                    on,
                    value: HeartRateVariability::from_milliseconds(milliseconds).expect("a night"),
                })
                .collect(),
        })]
    }

    /// `count` nights from 1 January 2026, each `milliseconds` of its index.
    fn nights(count: i64, milliseconds: impl Fn(i64) -> u32) -> [Trended; 1] {
        trended((0..count).map(|index| {
            (
                date(2026, 1, 1)
                    .checked_add(jiff::Span::new().days(index))
                    .expect("a night"),
                milliseconds(index),
            )
        }))
    }

    fn shown(trended: &[Trended], ranges: &Ranges, over: Over) -> String {
        super::page(trended, ranges, over).into_string()
    }

    #[test]
    fn a_night_is_a_dot_the_trend_a_line_and_the_band_grey() {
        let page = shown(
            &nights(70, |index| if index % 2 == 0 { 50 } else { 56 }),
            &NO_PLAN,
            Over::All,
        );

        assert_eq!(page.matches(r#"class="dot""#).count(), 70, "{page}");
        assert_eq!(page.matches(r#"class="line""#).count(), 1, "{page}");
        assert_eq!(
            page.matches(r#"<polygon class="reference""#).count(),
            1,
            "{page}"
        );
        assert!(!page.contains(r#"class="band""#), "no band on the trend");
        assert!(
            page.contains("11 Mar</span> · 56 ms · device unknown · 7 nights 53 ms · band "),
            "{page}"
        );
    }

    #[test]
    fn a_range_shows_only_its_nights_and_nothing_recorded_is_said() {
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
        assert!(page.contains("21 Dec</span> · 63 ms"), "{page}");
        assert!(
            !page.contains("2 Dec</span> · 50 ms"),
            "before this mesocycle"
        );
        assert!(page.contains(r#"href="/hrv?over=all""#));

        let page = shown(&one, &ranges, Over::Macrocycle);
        assert!(page.contains("No macrocycle is planned for today."));

        let page = shown(&[], &NO_PLAN, Over::All);
        assert!(page.contains("Nothing has been recorded."));
    }
}
