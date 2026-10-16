//! Overnight HRV as a trend against its own band (#417).

use domain::{
    analytical::{
        hrv::{Against, trend},
        per_device::Reading,
    },
    measure::{HeartRateVariability, InvalidQuantity},
};
use jiff::civil::{Date, date};

type Night = Reading<HeartRateVariability>;

fn nights(each: impl IntoIterator<Item = (Date, u32)>) -> Result<Vec<Night>, InvalidQuantity> {
    each.into_iter()
        .map(|(on, milliseconds)| {
            Ok(Reading {
                on,
                value: HeartRateVariability::from_milliseconds(milliseconds)?,
            })
        })
        .collect()
}

fn days(from: Date, count: usize) -> impl Iterator<Item = Date> {
    from.series(jiff::Span::new().days(1)).take(count)
}

#[test]
fn the_trend_is_the_mean_of_the_logs_and_breaks_below_three_of_seven() {
    // 1, 2 and 4 March recorded, then nothing until the 12th.
    let nights = nights([
        (date(2026, 3, 1), 40),
        (date(2026, 3, 2), 90),
        (date(2026, 3, 4), 60),
        (date(2026, 3, 12), 50),
    ])
    .expect("nights");
    let runs = trend(&nights);

    // The 4th is the first day with three of the seven; the 7th the last.
    assert_eq!(runs.len(), 1);
    let run = &runs[0];
    assert_eq!(run.first().map(|day| day.on), Some(date(2026, 3, 4)));
    assert_eq!(run.last().map(|day| day.on), Some(date(2026, 3, 7)));
    // exp(mean(ln 40, ln 90, ln 60)) is their geometric mean, 60 ms.
    assert!((run[0].milliseconds - 60.0).abs() < 1e-9);
    assert!(
        run.iter()
            .all(|day| day.band.is_none() && day.against().is_none())
    );
}

#[test]
fn the_band_is_the_sixty_nights_before_the_week_and_needs_twenty_six() {
    let start = date(2026, 1, 1);
    // Alternating 40 and 60 ms for 26 nights, then a week at 30 ms.
    let alternating = days(start, 26)
        .enumerate()
        .map(|(index, on)| (on, if index % 2 == 0 { 40 } else { 60 }));
    let week = days(date(2026, 1, 27), 7).map(|on| (on, 30));
    let nights = nights(alternating.chain(week)).expect("nights");

    let runs = trend(&nights);
    let day = |on: Date| runs.iter().flatten().find(|day| day.on == on).copied();

    // On 2 Feb the week is 27 Jan to 2 Feb and the 60 nights before hold all 26.
    let last = day(date(2026, 2, 2)).expect("a trend");
    let band = last.band.expect("26 of the 60");
    assert!(band.low < 40.0 && band.high > 60.0 && band.low > 30.0);
    assert_eq!(last.against(), Some(Against::Below));
    assert!(last.variation.abs() < 1e-9);

    // On 1 Feb the 60 nights before stop at 25 Jan: 25 recorded, so no band.
    assert!(day(date(2026, 2, 1)).expect("a trend").band.is_none());
}

#[test]
fn a_week_like_its_baseline_is_within() {
    let nights = nights(
        days(date(2026, 1, 1), 70)
            .enumerate()
            .map(|(index, on)| (on, if index % 2 == 0 { 50 } else { 56 })),
    )
    .expect("nights");
    let runs = trend(&nights);
    let last = runs.iter().flatten().next_back().copied().expect("a trend");
    assert_eq!(last.against(), Some(Against::Within));
    assert!(last.variation > 0.0);
}

#[test]
fn no_nights_is_no_trend() {
    assert!(trend(&[]).is_empty());
}
