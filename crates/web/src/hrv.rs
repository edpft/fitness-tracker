//! Overnight HRV, on the detail template (#622, #417).
//!
//! **The summary is the last 7 nights' mean and where it sits against the 60
//! nights before**: below, within or above, the band being how far a week can
//! differ from them by chance (#688). It is the latest whatever the range: the
//! range only chooses which days the chart shows. The 7 nights' variation is
//! not shown: there is nothing to read it against (operator, 2026-11-02).
//!
//! **No ± on the trend** (#616): the band it is read against is already the
//! noise.
//!
//! **One chart**: each night a dot, each watch's trend, and the band in
//! neutral grey. Each watch's trend is its own (§ 6); the watches never
//! overlapped, so a tap on a night names the one that recorded it.

use std::collections::BTreeMap;

use domain::analytical::{
    hrv::{BAND, Band, Day, METHOD, Trended},
    range::{Over, Range, Ranges},
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::detail::{self, Axis, Cell, Chart, Layer, Metric, Reading, Segment, Traced, name};

pub const PATH: &str = "/hrv";
pub const TITLE: &str = "Overnight HRV";

/// The summary, the chart over the range chosen and the latest nights, or
/// why there is no range.
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

/// Nights, over the range chosen.
fn nights_card(trended: &[Trended], ranges: &Ranges, over: Over) -> Markup {
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
            summary_card(trended, ranges, over),
            nights_card(trended, ranges, over),
            latest_card(trended, ranges, over),
        ],
    )
}

crate::card::ranged_card!(
    /// The summary.
    Summary,
    "hrv-summary",
    configurator::analyses::OvernightHrv,
    summary_card
);
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

/// The last 7 nights' mean against its band.
fn summary(trended: &[Trended], today: Date) -> Markup {
    let Some((one, day)) = trended
        .iter()
        .filter_map(|one| {
            one.trend
                .iter()
                .flatten()
                .next_back()
                .map(|day| (one, *day))
        })
        .max_by_key(|(_, day)| day.on)
    else {
        return html! {};
    };
    let against = day.band.map_or_else(
        || Reading::NoRule("Too few nights before to read it against.".to_owned()),
        |band| position(band, day.milliseconds),
    );
    detail::summary(
        &[Cell {
            metric: Metric::Hrv,
            label: "Last 7 nights",
            figure: format!("{:.0}", day.milliseconds),
            unit: "ms",
            margin: None,
            readings: vec![against],
        }],
        &format!(
            "Trend on {} · {}",
            detail::day(day.on, today),
            name(&one.series.sent_by)
        ),
    )
}

/// Below, within or above the band, the outer two as wide as the band.
fn position(band: Band, at: f64) -> Reading {
    let width = band.high - band.low;
    Reading::Position {
        segments: vec![
            segment((band.low - width).max(0.0), band.low, "below"),
            segment(band.low, band.high, "within"),
            segment(band.high, band.high + width, "above"),
        ],
        at,
    }
}

fn segment(from: f64, to: f64, name: &str) -> Segment {
    Segment {
        from,
        to,
        name: name.to_owned(),
    }
}

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
    type Cards = (Summary, NightsChart, Latest);

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
    fn the_summary_is_the_week_against_its_band() {
        // 67 nights alternating 50 and 56 ms, then a week at 30: far below.
        let page = shown(
            &nights(74, |index| match index {
                67.. => 30,
                _ if index % 2 == 0 => 50,
                _ => 56,
            }),
            &NO_PLAN,
            Over::All,
        );

        assert!(page.contains(r#"class="cell hrv""#), "{page}");
        assert!(page.contains("30<small>ms</small>"), "{page}");
        assert!(!page.contains("Variation"), "nothing to read it against");
        assert_eq!(page.matches(r#"class="segment lit""#).count(), 1);
        let lit = page.find(r#"class="segment lit""#).expect("lit");
        let below = page.find(">below<").expect("below");
        let within = page.find(">within<").expect("within");
        assert!(lit < below && below < within, "below is the one lit");
        assert!(page.contains("Trend on 15 Mar · device unknown"), "{page}");
        assert!(
            !page.contains(r#"class="margin""#),
            "no ± on the trend: the band is the noise already"
        );
    }

    #[test]
    fn too_few_nights_before_is_said() {
        let page = shown(&nights(10, |_| 50), &NO_PLAN, Over::All);
        assert!(
            page.contains("Too few nights before to read it against."),
            "{page}"
        );
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
