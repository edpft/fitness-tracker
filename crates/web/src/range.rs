//! The range a chart is read over, chosen on every chart's page alike (#252,
//! #497).
//!
//! **Which days each cycle starts on is the configurator's**, read once for
//! every page; this only names the choices, offers them, and marks the date
//! axis across whichever was chosen.

use domain::analytical::range::{Over, Range, Ranges};
use jiff::civil::Date;
use maud::{Markup, html};

/// The range `?over=` names; the macrocycle for anything else.
pub fn named(over: Option<&str>) -> Over {
    Over::ALL
        .into_iter()
        .find(|each| Some(name(*each)) == over)
        .unwrap_or(Over::Macrocycle)
}

const fn name(over: Over) -> &'static str {
    match over {
        Over::Mesocycle => "mesocycle",
        Over::Macrocycle => "macrocycle",
        Over::All => "all",
    }
}

const fn label(over: Over) -> &'static str {
    match over {
        Over::Mesocycle => "this mesocycle",
        Over::Macrocycle => "this macrocycle",
        Over::All => "all",
    }
}

/// Every range, the chosen one unlinked.
///
/// **Another range redraws `main` as htmx**, so choosing one does not start
/// another run.
pub fn choices(path: &str, over: Over) -> Markup {
    html! {
        p.choices {
            @for each in Over::ALL {
                @if each == over {
                    strong { (label(each)) }
                } @else {
                    @let address = format!("{path}?over={}", name(each));
                    a href=(address) hx-get=(address) hx-select="main"
                        hx-target="main" hx-swap="outerHTML" hx-push-url="true"
                        { (label(each)) }
                }
            }
        }
    }
}

/// The days `over` names, or what to say instead: that no such cycle is
/// planned for today, or `nothing` where everything was asked for and nothing
/// was recorded.
pub fn chosen(
    ranges: &Ranges,
    over: Over,
    recorded: Option<Range>,
    nothing: &str,
) -> Result<Range, Markup> {
    ranges.over(over, recorded).ok_or_else(|| match over {
        Over::Mesocycle => html! { p { "No mesocycle is planned for today." } },
        Over::Macrocycle => html! { p { "No macrocycle is planned for today." } },
        Over::All => html! { p { (nothing) } },
    })
}

/// A day's place on a date axis: its start, in UTC, so every day is the same
/// width.
pub fn seconds(on: Date) -> i64 {
    on.to_zoned(jiff::tz::TimeZone::UTC)
        .map_or(0, |zoned| zoned.timestamp().as_second())
}

/// Where the date axis is marked: every day across a week, every Monday
/// across a few months, every month across a year or two, and every year
/// beyond. Never on the range's last day, whose label would run off the
/// chart's right edge.
pub fn ticks(range: Range) -> Vec<(Date, String)> {
    let days = range.from.until(range.to).map_or(0, |span| span.get_days());
    let (start, step, format) = if days <= 7 {
        (range.from, jiff::Span::new().days(1), "%a %-d")
    } else if days <= 120 {
        (
            monday_on_or_after(range.from),
            jiff::Span::new().weeks(1),
            "%-d %b",
        )
    } else if days <= 730 {
        (
            first_of_month_after(range.from),
            jiff::Span::new().months(1),
            "%b %y",
        )
    } else {
        (
            Date::new(range.from.year().saturating_add(1), 1, 1).unwrap_or(range.from),
            jiff::Span::new().years(1),
            "%Y",
        )
    };
    start
        .series(step)
        .take_while(|tick| *tick < range.to || range.from == range.to)
        .map(|tick| (tick, tick.strftime(format).to_string()))
        .collect()
}

fn monday_on_or_after(day: Date) -> Date {
    let ahead = (7 - i64::from(day.weekday().to_monday_zero_offset())) % 7;
    day.checked_add(jiff::Span::new().days(ahead))
        .unwrap_or(day)
}

fn first_of_month_after(day: Date) -> Date {
    day.first_of_month()
        .checked_add(jiff::Span::new().months(1))
        .unwrap_or(day)
}

#[cfg(test)]
mod tests {
    use domain::analytical::range::Over;

    use super::named;

    #[test]
    fn an_unknown_range_is_this_macrocycle() {
        assert_eq!(named(Some("fortnight")), Over::Macrocycle);
        assert_eq!(named(Some("microcycle")), Over::Macrocycle);
        assert_eq!(named(None), Over::Macrocycle);
        assert_eq!(named(Some("all")), Over::All);
    }
}
