//! Body weight and body composition, read together on one page (#786).
//!
//! The operator, 2026-11-12, on #769: *"When I say read together, I mean
//! grouped together both in the summary tiles and a page."* In a cut, the page
//! answers whether he is losing weight at the rate the plan intends and
//! whether what he loses is fat or fat-free mass.
//!
//! **Two sections, each with its own summary** (operator, 2026-11-13): body
//! weight, its trend against where the plan's aims would have taken it; then
//! body composition, fat and fat-free mass against what the aims expect of
//! them. Each expectation resets at every mesocycle's start. Beneath both,
//! the weigh-ins, each its weight, fat and fat-free mass.

use domain::analytical::{
    body_weight::{Aim, METHOD, Series},
    composition::{ComponentSeries, TREND},
    confidence::BAND,
    range::{Over, Range, Ranges},
};
use maud::{DOCTYPE, Markup, html};

use crate::{composition, detail, page::Group, run::View, weight};

pub const PATH: &str = "/body";

/// What the page is called: its group's heading, since it is the whole group.
pub const TITLE: &str = Group::Body.heading();

/// The pages this one replaced, which now open it.
pub const REPLACED: [&str; 2] = ["/body-weight", "/body-composition"];

/// The whole page, over the range chosen.
pub fn page(
    series: &Series,
    aims: &[Aim],
    components: &[ComponentSeries],
    ranges: &Ranges,
    over: Over,
    run: &View,
) -> Markup {
    detail::page(
        PATH,
        TITLE,
        Some(over),
        run,
        &body(series, aims, components, ranges, over),
    )
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

/// The summary, the charts over the range chosen and the latest weigh-ins, or
/// why there is no range.
fn body(
    series: &Series,
    aims: &[Aim],
    components: &[ComponentSeries],
    ranges: &Ranges,
    over: Over,
) -> Markup {
    let recorded = Range::spanning(
        [weight::recorded(series), composition::recorded(components)]
            .into_iter()
            .flatten()
            .flat_map(|range| [range.from, range.to]),
    );
    let range = match crate::range::chosen(ranges, over, recorded, "Nothing has been recorded.") {
        Ok(range) => range,
        Err(said) => return said,
    };
    let today = range.to;
    let began = ranges.macrocycle.map(|cycle| cycle.from);
    let weighed = weight::cell(series).map(|(cell, on)| {
        detail::summary(&[cell], &format!("Trend on {}", detail::day(on, today)))
    });
    let (cells, asof) = composition::summary(components, today);
    let measured = (!cells.is_empty()).then(|| detail::summary(&cells, &asof.join("; ")));
    html! {
        @if weight::recorded(series).is_some() {
            h2.group { "Body weight" }
            @if let Some(summary) = weighed { (summary) }
            (weight::charts(series, aims, began, range))
        }
        @if composition::recorded(components).is_some() {
            h2.group { "Body composition" }
            @if let Some(summary) = measured { (summary) }
            (composition::charts(components, aims, began, range))
        }
        (weight::latest(series, today))
        (detail::foot(&foot(series, components, range)))
    }
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

    /// **One page reads both, in two sections**: body weight with its own
    /// summary, then composition with its own.
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
            &crate::run::View::default(),
        )
        .into_string();

        assert!(
            page.contains("<h1>Body weight and composition</h1>"),
            "{page}"
        );
        assert_eq!(
            page.matches(r#"class="card summary""#).count(),
            2,
            "a summary for each section: {page}"
        );
        for label in ["Trend weight", "Fat", "Fat-free"] {
            assert!(
                page.contains(&format!(r#"<i class="swatch"></i>{label}</div>"#)),
                "{label}: {page}"
            );
        }
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
        assert!(
            page.contains("Trend on 28 Oct · Body Scan, algorithm 218300672<"),
            "{page}"
        );
        assert!(page.contains(r#"href="/body?over=all""#), "{page}");
    }
}
