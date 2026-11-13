//! Body weight and body composition, read together on one page (#786).
//!
//! The operator, 2026-11-12, on #769: *"When I say read together, I mean
//! grouped together both in the summary tiles and a page."* In a cut, the page
//! answers whether he is losing weight at the rate the plan intends and
//! whether what he loses is fat or fat-free mass.
//!
//! **Body weight's charts, then composition's**: the summary holds trend
//! weight, fat and fat-free mass; then body weight and weight change against
//! the plan's aim; then fat, fat-free mass and the mesocycle matrix; then the
//! weigh-ins, each its weight, fat and fat-free mass.

use application::composition::AgainstPhase;
use domain::analytical::{
    body_weight::{Aim, CHANGE_METHOD, METHOD, Series},
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
    against: &AgainstPhase,
    ranges: &Ranges,
    over: Over,
    run: &View,
) -> Markup {
    detail::page(
        PATH,
        TITLE,
        Some(over),
        run,
        &body(series, aims, components, against, ranges, over),
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
    against: &AgainstPhase,
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
    html! {
        (summary(series, components, range))
        (weight::charts(series, aims, range))
        (composition::charts(components, against, range))
        (weight::latest(series, range.to))
        (detail::foot(&foot(series, components, range)))
    }
}

/// Trend weight, then fat and fat-free mass, and the day each trend is on.
fn summary(series: &Series, components: &[ComponentSeries], range: Range) -> Markup {
    let today = range.to;
    let weighed = weight::cell(series);
    let (fat, mut asof) = composition::summary(components, today);
    if let Some((_, on)) = &weighed {
        let said = format!("Trend on {}", detail::day(*on, today));
        if !asof.iter().any(|one| one.starts_with(&said)) {
            asof.insert(0, said);
        }
    }
    let cells: Vec<detail::Cell<'_>> = weighed
        .map(|(cell, _)| cell)
        .into_iter()
        .chain(fat)
        .collect();
    if cells.is_empty() {
        return html! {};
    }
    detail::summary(&cells, &asof.join("; "))
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
        said.push(format!(
            "Weight change: {CHANGE_METHOD}; dashed, the plan's aim"
        ));
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

    /// **One page reads both**: trend weight, fat and fat-free mass in one
    /// summary, and body weight's charts before composition's.
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
            &application::composition::AgainstPhase::NoMacrocycle,
            &ranges,
            Over::Mesocycle,
            &crate::run::View::default(),
        )
        .into_string();

        assert!(
            page.contains("<h1>Body weight and composition</h1>"),
            "{page}"
        );
        for label in ["Trend weight", "Fat", "Fat-free"] {
            assert!(
                page.contains(&format!(r#"<i class="swatch"></i>{label}</div>"#)),
                "{label}: {page}"
            );
        }
        let order: Vec<usize> = [
            "<h2>Body weight</h2>",
            "<h2>Weight change</h2>",
            "<h2>Fat</h2>",
            "<h2>Fat-free mass</h2>",
            "<h2>Weigh-ins</h2>",
        ]
        .iter()
        .map(|heading| page.find(heading).expect(heading))
        .collect();
        assert!(order.is_sorted(), "{order:?}");
        assert!(
            page.contains("Trend on 28 Oct · Body Scan, algorithm 218300672<"),
            "one day said once: {page}"
        );
        assert!(page.contains(r#"href="/body?over=all""#), "{page}");
    }
}
