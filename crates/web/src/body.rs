//! Body weight and body composition, read together on one page (#786).
//!
//! The operator, 2026-11-12, on #769: *"When I say read together, I mean
//! grouped together both in the summary tiles and a page."* In a cut, the page
//! answers whether he is losing weight at the rate the plan intends and
//! whether what he loses is fat or fat-free mass.
//!
//! **Two sections** (operator, 2026-11-13): body weight, its trend against
//! where the plan's aims would have taken it; then body composition, fat and
//! fat-free mass against what the aims expect of them, under their summary.
//! Each expectation resets at every mesocycle's start. Beneath both, the
//! weigh-ins, each its weight, fat and fat-free mass.
//!
//! **Body weight is one chart here** (operator, 2026-11-18): Progress's body
//! weight card is a sparkline of it, read from the same analyses, and is not
//! repeated above it.

use std::{collections::HashMap, sync::Arc};

use configurator::{analyses::WeightAndComposition, held::Ranged};
use domain::analytical::{
    body_weight::{Aim, METHOD, Series},
    composition::{ComponentSeries, TREND},
    confidence::BAND,
    range::{Over, Range, Ranges},
};
use maud::{Markup, html};

use crate::{card::Card, composition, detail, page::Group, weight};

pub const PATH: &str = "/body";

/// What the page is called: its group's heading, since it is the whole group.
pub const TITLE: &str = Group::Body.heading();

/// The pages this one replaced, which now open it.
pub const REPLACED: [&str; 2] = ["/body-weight", "/body-composition"];

/// What every chart on the page reads: the weigh-ins and the composition,
/// whose recorded days together are the range each is drawn over, the aims,
/// and the ranges.
type Reads = Ranged<configurator::analyses::Body>;

type Read = (Arc<WeightAndComposition>, Arc<Ranges>);

/// What the page's cards are drawn from, over a range.
pub struct Body<'a> {
    pub series: &'a Series,
    pub components: &'a [ComponentSeries],
    pub aims: &'a [Aim],
    pub ranges: &'a Ranges,
    pub over: Over,
}

impl Body<'_> {
    /// Draw `card` from `read`, over `over`.
    fn drawn((read, ranges): &Read, over: Over, card: impl FnOnce(&Body<'_>) -> Markup) -> Markup {
        card(&Body {
            series: &read.series,
            components: &read.components,
            aims: &read.aims,
            ranges,
            over,
        })
    }

    /// The days the cards are drawn over, or what to say instead.
    fn range(&self) -> Result<Range, Markup> {
        let recorded = Range::spanning(
            [
                weight::recorded(self.series),
                composition::recorded(self.components),
            ]
            .into_iter()
            .flatten()
            .flat_map(|range| [range.from, range.to]),
        );
        crate::range::chosen(
            self.ranges,
            self.over,
            recorded,
            "Nothing has been recorded.",
        )
    }

    /// Body weight's charts, against where the aims would have taken it.
    pub fn weight_charts(&self) -> Markup {
        match self.range() {
            Ok(range) if weight::recorded(self.series).is_some() => weight::charts(
                self.series,
                self.aims,
                self.ranges.macrocycle.map(|cycle| cycle.from),
                range,
            ),
            _ => html! {},
        }
    }

    /// Fat and fat-free mass, against what the aims expect of them.
    pub fn composition_charts(&self) -> Markup {
        match self.range() {
            Ok(range) if composition::recorded(self.components).is_some() => composition::charts(
                self.components,
                self.aims,
                self.ranges.macrocycle.map(|cycle| cycle.from),
                range,
            ),
            _ => html! {},
        }
    }

    /// The latest weigh-ins, and what the dots, lines and bands are; or why
    /// there is no range, which leaves every other card empty.
    pub fn weigh_ins(&self) -> Markup {
        match self.range() {
            Ok(range) => html! {
                (weight::latest(self.series, range.to))
                (detail::foot(&foot(self.series, self.components, range)))
            },
            Err(said) => said,
        }
    }
}

/// The page: each section under its heading, then the weigh-ins.
///
/// **A heading over nothing is not shown**: a section none of whose cards
/// drew anything is hidden by the stylesheet.
fn laid_out(over: Over, cards: [Markup; 5]) -> Markup {
    let [weight_charts, fat, fat_free, composition_charts, weigh_ins] = cards;
    detail::page(
        PATH,
        TITLE,
        Some(over),
        &html! {
            div.cards {
                h2.group { "Body weight" }
                (weight_charts)
            }
            div.cards {
                h2.group { "Body composition" }
                section.card.tile.summary-of { (fat) (fat_free) }
                (composition_charts)
            }
            (weigh_ins)
        },
    )
}

/// The whole page over `over`, each card drawn from the same reads, as a
/// load draws it.
#[cfg(test)]
pub fn page(
    series: &Series,
    aims: &[Aim],
    components: &[ComponentSeries],
    ranges: &Ranges,
    over: Over,
) -> Markup {
    let body = Body {
        series,
        components,
        aims,
        ranges,
        over,
    };
    laid_out(
        over,
        [
            body.weight_charts(),
            maud::html! {},
            maud::html! {},
            body.composition_charts(),
            body.weigh_ins(),
        ],
    )
}

/// What the dots, lines and bands are, for what the page draws.
fn foot(series: &Series, components: &[ComponentSeries], range: Range) -> String {
    let mut said = vec![if composition::hollow(components, range) {
        "Dots: each weigh-in, hollow from the spreadsheets".to_owned()
    } else {
        "Dots: each weigh-in".to_owned()
    }];
    if !series.weigh_ins.is_empty() {
        said.push(format!("Line: the trend, the {METHOD}"));
        said.push(
            "Dashed: where the plan's aims would have taken each trend since its mesocycle began; fat takes the aim, and fat-free mass holds"
                .to_owned(),
        );
    }
    if composition::recorded(components).is_some() {
        said.push(format!(
            "Fat and fat-free mass: each method's trend, the {TREND}"
        ));
    }
    said.push(format!("Band: {BAND}"));
    format!("{}.", said.join(". "))
}

macro_rules! body_card {
    ($(#[$meta:meta])* $card:ident, $name:literal, $draw:ident) => {
        $(#[$meta])*
        pub struct $card {
            over: Over,
        }

        impl Card for $card {
            const NAME: &'static str = $name;
            type Reads = Reads;

            fn asked(query: &HashMap<String, String>) -> Self {
                Self {
                    over: crate::drawn::over(query),
                }
            }

            fn over(&self) -> Option<Over> {
                Some(self.over)
            }

            fn render(&self, read: Read) -> Markup {
                Body::drawn(&read, self.over, |body| body.$draw())
            }
        }
    };
}

body_card!(
    /// Body weight's charts.
    WeightCharts,
    "body-weight-charts",
    weight_charts
);
body_card!(
    /// Fat and fat-free mass.
    CompositionCharts,
    "body-composition-charts",
    composition_charts
);
body_card!(
    /// The latest weigh-ins.
    WeighIns,
    "body-weigh-ins",
    weigh_ins
);

/// Body weight and body composition over a range (#786).
#[derive(Debug)]
pub struct Page {
    over: Over,
}

impl crate::card::Page for Page {
    const ROUTE: &'static str = PATH;
    type Cards = (
        WeightCharts,
        crate::summary::Fat,
        crate::summary::FatFree,
        CompositionCharts,
        WeighIns,
    );

    fn asked(
        _path: &axum::extract::RawPathParams,
        query: &HashMap<String, String>,
    ) -> Result<Self, axum::response::Redirect> {
        Ok(Self {
            over: crate::drawn::over(query),
        })
    }

    fn over(&self) -> Option<Over> {
        Some(self.over)
    }

    fn layout(&self, cards: [Markup; 5]) -> Markup {
        laid_out(self.over, cards)
    }
}

#[cfg(test)]
mod tests {
    use domain::{
        analytical::{
            Weighed,
            body_weight::Series,
            composition::{ComponentSeries, Method, MethodSeries, Reading, Unit},
            range::{Over, Range, Ranges},
        },
        body::Component,
        canonical::Occurred,
        measure::Kg,
    };
    use jiff::civil::date;

    use super::page;

    /// **One page reads both, in two sections**: body weight's chart alone,
    /// then composition under fat's and fat-free mass's summary.
    #[test]
    fn body_weight_and_composition_are_one_page() {
        let weigh_ins = (1..=28)
            .map(|d| Weighed {
                at: Occurred::On(date(2026, 10, d)),
                mass: Kg::from_grams(84_000),
                fat_free: Some(Kg::from_grams(66_000)),
            })
            .collect();
        let component = |component, value| ComponentSeries {
            component,
            unit: Unit::Kilograms,
            series: vec![MethodSeries::of(
                Method::BodyScan {
                    algorithm: 218_300_672,
                },
                (1..=28)
                    .map(|d| Reading {
                        on: date(2026, 10, d),
                        value,
                    })
                    .collect(),
            )],
        };
        let ranges = Ranges {
            mesocycle: Some(Range {
                from: date(2026, 9, 28),
                to: date(2026, 10, 28),
            }),
            macrocycle: None,
        };
        let page = page(
            &Series::of(weigh_ins),
            &[],
            &[
                component(Component::Fat, 18.0),
                component(Component::FatFree, 66.0),
            ],
            &ranges,
            Over::Mesocycle,
        )
        .into_string();

        assert!(
            page.contains("<h1>Body weight and composition</h1>"),
            "{page}"
        );
        assert_eq!(
            page.matches(r#"class="card tile summary-of""#).count(),
            1,
            "composition's summary alone: {page}"
        );
        let order: Vec<usize> = [
            r#"<h2 class="group">Body weight</h2>"#,
            "<h2>Body weight</h2>",
            r#"<h2 class="group">Body composition</h2>"#,
            "<h2>Fat</h2>",
            "<h2>Fat-free mass</h2>",
            "<h2>Weigh-ins</h2>",
        ]
        .iter()
        .map(|heading| page.find(heading).expect(heading))
        .collect();
        assert!(order.is_sorted(), "{order:?}");
        assert!(!page.contains("Weight change"), "{page}");
        assert!(page.contains(r#"href="/body?over=all""#), "{page}");
    }
}
