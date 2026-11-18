//! Each reading's summary, one card shown both on Progress and on the
//! reading's own page (#830).
//!
//! The operator, 2026-11-16: *"Progress is a summary page, its cards are
//! composites of visualisations that are, themselves, or should be
//! themselves, summary cards on their own pages."* **So each reading has one
//! summary**, measured against the macrocycle's start and the plan's
//! expectation, and it is the same card wherever it is shown: a reading that
//! moves is drawn once and the drawing is shared.
//!
//! **A card is one reading's row, and the page lays it out**: Progress groups
//! rows into its tiles and opens each row's page, and a reading's page puts
//! its row at the top. Body weight's is the exception: its sparkline is a
//! simplification of `/body`'s chart, so `/body` does not repeat it
//! (operator, 2026-11-18). A reading with nothing to show draws nothing, and the
//! stylesheet hides a tile left with no row. On a day no plan is in force
//! there is no macrocycle to read, so no summary is drawn.

use std::{collections::HashMap, sync::Arc};

use application::working::{Answering, Framed, Responding};
use configurator::{
    analyses::{
        AtPowerSinceStart, BodyWeightAgainstAims, FatFreeSinceStart, FatSinceStart, FtpSinceEntry,
        OvernightHrvResponse, RestingHeartRateResponse, StressResponse,
    },
    held::Of,
};
use domain::analytical::{
    body_weight::Weight, range::Range, since_entry::SinceEntry, since_start::SinceStart,
};
use jiff::civil::Date;
use maud::{Markup, html};

use crate::{
    card::Card,
    detail::{self, Metric},
};

/// How a reading's figures are written.
#[derive(Clone, Copy)]
pub struct Written {
    pub metric: Metric,
    pub unit: &'static str,
    /// What one of the figure's own units is in the unit shown: a gram is a
    /// thousandth of a kilogram.
    pub scale: f64,
    pub places: usize,
}

impl Written {
    pub fn figure(self, value: f64) -> String {
        let shown = format!("{:.*}", self.places, value * self.scale);
        if self.unit.is_empty() {
            shown
        } else {
            format!("{shown} {}", self.unit)
        }
    }

    pub fn signed(self, by: f64) -> String {
        let shown = self.figure(by.abs());
        if shown == self.figure(0.0) {
            shown
        } else if by < 0.0 {
            format!("−{shown}")
        } else {
            format!("+{shown}")
        }
    }
}

pub const FTP: Written = Written {
    metric: Metric::Ftp,
    unit: "W",
    scale: 1.0,
    places: 0,
};

pub const ONE_REP_MAX: Written = Written {
    metric: Metric::OneRepMax,
    unit: "kg",
    scale: 0.001,
    places: 1,
};

pub const KG: Written = Written {
    metric: Metric::BodyWeight,
    unit: "kg",
    scale: 1.0,
    places: 1,
};

pub const AT_POWER: Written = Written {
    metric: Metric::HeartRateAtPower,
    unit: "bpm",
    scale: 1.0,
    places: 1,
};

pub const fn answering(reading: Answering) -> (&'static str, Written) {
    match reading {
        Answering::RestingHeartRate => (
            "Resting heart rate",
            Written {
                metric: Metric::RestingHeartRate,
                unit: "bpm",
                scale: 1.0,
                places: 1,
            },
        ),
        Answering::OvernightHrv => (
            "Overnight HRV",
            Written {
                metric: Metric::Hrv,
                unit: "ms",
                scale: 1.0,
                places: 1,
            },
        ),
        Answering::Stress => (
            "Stress",
            Written {
                metric: Metric::Stress,
                unit: "",
                scale: 1.0,
                places: 1,
            },
        ),
    }
}

/// One reading: its name, its figure now, and what is said beneath.
pub fn row(metric: Metric, label: &str, figure: Option<String>, beneath: &Markup) -> Markup {
    html! {
        div class=(format!("reading {}", metric.class())) {
            div.name {
                i.swatch {}
                (label)
                @if let Some(figure) = figure { em { (figure) } }
            }
            (beneath)
        }
    }
}

/// A change on the bar: how far, over what, and the expected result where
/// there is one.
pub fn change(written: Written, by: f64, noise: f64, over: &str, expected: Option<f64>) -> Markup {
    html! {
        div.change.plain {
            div {
                b { (written.signed(by)) }
                span { (over) }
                @if let Some(expected) = expected {
                    span.aimed { i {} "expected " (written.signed(expected)) }
                }
            }
            (detail::gauge(by, noise, expected))
        }
    }
}

/// A test since the entry test; `None` where there is no test since it.
pub fn test(written: Written, label: &str, since: Option<SinceEntry>) -> Option<Markup> {
    let since = since?;
    let (_, latest) = since.latest?;
    let by = f64::from(latest) - f64::from(since.entry.1);
    Some(row(
        written.metric,
        label,
        Some(written.figure(f64::from(latest))),
        &change(written, by, since.noise, "since the entry test", None),
    ))
}

/// An estimate since the start, on its bar.
pub fn since_start(written: Written, label: &str, since: SinceStart) -> Markup {
    let beneath = since.margin.map_or_else(
        || {
            html! {
                div.change.plain {
                    div { b { (written.signed(since.by())) } span { "since the start" } }
                }
            }
        },
        |noise| change(written, since.by(), noise, "since the start", None),
    );
    row(
        written.metric,
        label,
        Some(written.figure(since.now)),
        &beneath,
    )
}

/// The change since the start, and the expected result where there is one,
/// above a line rather than a bar.
fn moved(written: Written, by: f64, expected: Option<f64>) -> Markup {
    html! {
        div.change.plain {
            div {
                b { (written.signed(by)) }
                span { "since the start" }
                @if let Some(expected) = expected {
                    span.aimed { i {} "expected " (written.signed(expected)) }
                }
            }
        }
    }
}

/// Body weight's trend since the start against where the plan's aims would
/// have taken it.
pub fn weight(weight: &Weight, began: Date, today: Date) -> Markup {
    let runs: Vec<Vec<Traced>> = weight
        .days
        .iter()
        .map(|run| {
            run.iter()
                .filter_map(|day| {
                    Some(Traced {
                        on: day.trend.on,
                        read: kilograms(day.trend.mass.as_grams())?,
                        margin: day.trend.margin,
                    })
                })
                .collect()
        })
        .collect();
    row(
        Metric::BodyWeight,
        "Body weight",
        Some(KG.figure(weight.change.now)),
        &html! {
            (moved(KG, weight.change.by(), weight.expected))
            (trace(&runs, &detail::expected(&weight.aimed, Range { from: began, to: today }), began, today))
        },
    )
}

/// Grams as kilograms, where they fit.
fn kilograms(grams: u64) -> Option<f64> {
    u32::try_from(grams)
        .ok()
        .map(|grams| f64::from(grams) / 1_000.0)
}

/// A response reading: its change since the start, and each week since it
/// against the expected result. `None` where it has no change to show.
pub fn responding(one: &Responding, began: Date, today: Date) -> Option<Markup> {
    let (label, written) = answering(one.reading);
    let response = one.response.as_ref()?;
    let read = response.change?;
    let runs: Vec<Vec<Traced>> = response
        .weeks
        .iter()
        .map(|run| {
            run.iter()
                .map(|week| Traced {
                    on: week.to,
                    read: week.read,
                    margin: week.margin,
                })
                .collect()
        })
        .collect();
    let expected: Vec<Vec<(Date, f64)>> = response
        .weeks
        .iter()
        .map(|run| {
            run.iter()
                .filter_map(|week| Some((week.to, week.expected?)))
                .collect()
        })
        .collect();
    Some(row(
        written.metric,
        label,
        Some(written.figure(read.now)),
        &html! {
            (moved(written, read.by, read.expected))
            (trace(&runs, &expected, began, today))
        },
    ))
}

/// One day of a line on a summary.
struct Traced {
    on: Date,
    read: f64,
    margin: Option<f64>,
}

const TRACE_WIDTH: f64 = 300.0;
const TRACE_HEIGHT: f64 = 64.0;

/// Each run of a reading from the start to today: its band, the expected
/// result dashed, the reading in its colour, and a dot on the latest day.
fn trace(runs: &[Vec<Traced>], expected: &[Vec<(Date, f64)>], began: Date, today: Date) -> Markup {
    let days = |on: Date| f64::from(began.until(on).map_or(0, |gap| gap.get_days()));
    let width = days(today).max(1.0);
    let x = |on: Date| (days(on) / width).mul_add(TRACE_WIDTH - 8.0, 4.0);
    let all = runs.iter().flatten();
    let (low, high) = all.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), day| {
        let margin = day.margin.unwrap_or(0.0);
        (low.min(day.read - margin), high.max(day.read + margin))
    });
    let (low, high) = expected
        .iter()
        .flatten()
        .fold((low, high), |(low, high), (_, value)| {
            (low.min(*value), high.max(*value))
        });
    if !low.is_finite() || !high.is_finite() {
        return html! {};
    }
    let spread = (high - low).max(f64::EPSILON);
    let y = |value: f64| ((high - value) / spread).mul_add(TRACE_HEIGHT - 6.0, 3.0);
    let points = |run: &[Traced], value: &dyn Fn(&Traced) -> Option<f64>| -> String {
        run.iter()
            .filter_map(|day| Some(format!("{:.1},{:.1}", x(day.on), y(value(day)?))))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let band = |run: &[Traced]| -> String {
        let above = run
            .iter()
            .filter_map(|day| Some((day.on, day.read + day.margin?)));
        let below = run
            .iter()
            .rev()
            .filter_map(|day| Some((day.on, day.read - day.margin?)));
        above
            .chain(below)
            .map(|(on, value)| format!("{:.1},{:.1}", x(on), y(value)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let last = runs.iter().flatten().next_back();
    html! {
        svg.trace viewBox=(format!("0 0 {TRACE_WIDTH} {TRACE_HEIGHT}")) aria-hidden="true" {
            @for run in runs { polygon.band points=(band(run)) {} }
            @for run in expected {
                polyline.expected points=(run.iter().map(|(on, value)| format!("{:.1},{:.1}", x(*on), y(*value))).collect::<Vec<_>>().join(" ")) {}
            }
            @for run in runs { polyline.read points=(points(run, &|day| Some(day.read))) {} }
            @if let Some(last) = last {
                circle.fell cx=(format!("{:.1}", x(last.on))) cy=(format!("{:.1}", y(last.read))) r="4" {}
            }
        }
    }
}

/// A summary drawn from what its analysis holds; nothing where it holds none.
fn shown<T>(read: Option<&T>, draw: impl FnOnce(&T) -> Option<Markup>) -> Markup {
    read.and_then(draw).unwrap_or_default()
}

/// The FTP test since the entry test.
pub struct FtpTest;

impl Card for FtpTest {
    const NAME: &'static str = "summary-ftp-test";
    type Reads = Of<FtpSinceEntry>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, since: Arc<Option<SinceEntry>>) -> Markup {
        test(FTP, "FTP test", *since).unwrap_or_default()
    }
}

/// Heart rate at 140 W since the start.
pub struct AtPower;

impl Card for AtPower {
    const NAME: &'static str = "summary-heart-rate-at-power";
    type Reads = Of<AtPowerSinceStart>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, since: Arc<Option<SinceStart>>) -> Markup {
        shown(since.as_ref().as_ref(), |since| {
            Some(since_start(AT_POWER, "Heart rate at 140 W", *since))
        })
    }
}

/// Body weight since the start, against the plan's aims.
pub struct BodyWeightSince;

impl Card for BodyWeightSince {
    const NAME: &'static str = "summary-body-weight";
    type Reads = Of<BodyWeightAgainstAims>;

    fn asked(_: &HashMap<String, String>) -> Self {
        Self
    }

    fn render(&self, since: Arc<Option<Framed<Weight>>>) -> Markup {
        shown(since.as_ref().as_ref(), |since| {
            Some(weight(&since.read, since.frame.began, since.frame.today))
        })
    }
}

macro_rules! component_card {
    (
        $(#[$meta:meta])*
        $card:ident, $name:literal, $node:ty, $metric:expr, $label:literal
    ) => {
        $(#[$meta])*
        pub struct $card;

        impl Card for $card {
            const NAME: &'static str = $name;
            type Reads = Of<$node>;

            fn asked(_: &HashMap<String, String>) -> Self {
                Self
            }

            fn render(&self, since: Arc<Option<SinceStart>>) -> Markup {
                shown(since.as_ref().as_ref(), |since| {
                    Some(since_start(Written { metric: $metric, ..KG }, $label, *since))
                })
            }
        }
    };
}

component_card!(
    /// Fat mass since the start.
    Fat,
    "summary-fat",
    FatSinceStart,
    Metric::Fat,
    "Fat"
);
component_card!(
    /// Fat-free mass since the start.
    FatFree,
    "summary-fat-free",
    FatFreeSinceStart,
    Metric::FatFree,
    "Fat-free"
);

macro_rules! response_card {
    ($(#[$meta:meta])* $card:ident, $name:literal, $node:ty) => {
        $(#[$meta])*
        pub struct $card;

        impl Card for $card {
            const NAME: &'static str = $name;
            type Reads = Of<$node>;

            fn asked(_: &HashMap<String, String>) -> Self {
                Self
            }

            fn render(&self, read: Arc<Option<Framed<Responding>>>) -> Markup {
                shown(read.as_ref().as_ref(), |read| {
                    responding(&read.read, read.frame.began, read.frame.today)
                })
            }
        }
    };
}

response_card!(
    /// Resting heart rate since the start, against the training done.
    RestingHeartRate,
    "summary-resting-heart-rate",
    RestingHeartRateResponse
);
response_card!(
    /// Overnight HRV since the start, against the training done.
    OvernightHrv,
    "summary-overnight-hrv",
    OvernightHrvResponse
);
response_card!(
    /// Stress since the start, against the training done.
    Stress,
    "summary-stress",
    StressResponse
);

/// A reading's summary at the top of its own page.
pub fn atop(summary: &Markup) -> Markup {
    html! { section.card.tile.summary-of { (summary) } }
}
