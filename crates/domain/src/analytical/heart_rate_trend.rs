//! Heart rate per power zone, as a trend (#415).
//!
//! **A line through each FTP period's rides** (operator, 2026-10-28), as the
//! estimated 1RM's is through each stretch between tests (#420). Heart rate
//! in a zone falls with fitness only while the zone stays fixed, and a test
//! moves every zone's watts, so a period from one test to the next is its own
//! line and no line crosses a test.
//!
//! **The direction is the slope's**: rising or falling only where its 95%
//! interval lies wholly above or below zero, and otherwise maintaining. Fewer
//! than three rides give no line, since two fix one exactly and leave nothing
//! to say how sure it is.
//!
//! **Days across.** The line is fitted against days since the period's test,
//! so a period's rides keep their dates on the chart.
//!
//! **One series per device, and per zone** (§ 6). A ride whose device is not
//! known has no line: those rides may be two devices', and a line through
//! them would join them.

use jiff::civil::Date;

use super::{
    body_weight::Direction,
    heart_rate::{ByZone, RideHeartRate, SentBy},
    one_rep_max::Line,
    range::Range,
};
use crate::{
    cycling::{Ftp, PowerZone},
    device::Device,
};

pub use super::one_rep_max::CONFIDENCE;

/// What the trend is, so a different method is never mistaken for it.
pub const TREND: &str = "least-squares line through each FTP period's rides, at least 3 of them";

/// One ride's figure in one series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub on: Date,
    pub beats: u32,
    /// The FTP in force on the ride's date.
    pub ftp: Option<Ftp>,
}

/// One FTP period's rides in one series, and the line through them.
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub ftp: Option<Ftp>,
    /// Oldest first.
    pub points: Vec<Point>,
    /// How many rides the line is fitted to: the whole period's, whatever
    /// range [`Self::points`] is cut to.
    pub rides: usize,
    /// Against days since [`Self::since`].
    pub line: Option<Line>,
}

impl Period {
    /// The day the line counts from: the FTP's, or the first ride's before
    /// any test.
    #[must_use]
    pub fn since(&self) -> Option<Date> {
        self.ftp
            .map(Ftp::from)
            .or_else(|| self.points.first().map(|point| point.on))
    }

    /// The day `day` days after [`Self::since`].
    #[must_use]
    pub fn day(&self, day: i32) -> Option<Date> {
        self.since()?
            .checked_add(jiff::Span::new().days(i64::from(day)))
            .ok()
    }

    /// Which way the line goes, where there is one.
    #[must_use]
    pub fn direction(&self) -> Option<Direction> {
        let line = self.line?;
        let (slope, margin) = (line.per_session(), line.slope_margin());
        Some(if slope - margin > 0.0 {
            Direction::Rising
        } else if slope + margin < 0.0 {
            Direction::Falling
        } else {
            Direction::Maintaining
        })
    }
}

/// One series' rides, period by period.
#[derive(Debug, Clone, PartialEq)]
pub struct Trended {
    /// Oldest first.
    pub periods: Vec<Period>,
}

impl Trended {
    /// The rides, oldest first, split at each FTP test.
    #[must_use]
    pub fn of(points: Vec<Point>) -> Self {
        let mut periods: Vec<Period> = Vec::new();
        for point in points {
            match periods.last_mut() {
                Some(period) if period.ftp == point.ftp => period.points.push(point),
                _ => periods.push(Period {
                    ftp: point.ftp,
                    points: vec![point],
                    rides: 0,
                    line: None,
                }),
            }
        }
        for period in &mut periods {
            period.rides = period.points.len();
            let Some(since) = period.since() else {
                continue;
            };
            let along: Option<Vec<(i32, f64)>> = period
                .points
                .iter()
                .map(|point| {
                    let days = since.until(point.on).ok()?.get_days();
                    Some((days, f64::from(point.beats)))
                })
                .collect();
            period.line = along.as_deref().and_then(Line::through);
        }
        Self { periods }
    }

    /// Only `range`'s rides, and only the periods holding one. Each line is
    /// the whole period's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            periods: self
                .periods
                .iter()
                .filter_map(|period| {
                    let points: Vec<Point> = period
                        .points
                        .iter()
                        .filter(|point| range.covers(point.on))
                        .copied()
                        .collect();
                    (!points.is_empty()).then_some(Period {
                        ftp: period.ftp,
                        points,
                        rides: period.rides,
                        line: period.line,
                    })
                })
                .collect(),
        }
    }

    /// Whether there is any ride to show.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.periods.is_empty()
    }
}

/// One device's series: each zone it held, and the Z3 intervals.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceTrends {
    pub device: Device,
    /// Lowest zone first.
    pub zones: Vec<(PowerZone, Trended)>,
    pub intervals: Trended,
}

impl DeviceTrends {
    /// Only `range`'s rides, every series kept.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            device: self.device.clone(),
            zones: self
                .zones
                .iter()
                .map(|(zone, trended)| (*zone, trended.within(range)))
                .collect(),
            intervals: self.intervals.within(range),
        }
    }
}

/// Every device's series, in the order the devices first sent a ride's heart
/// rate. `rides` is oldest first.
#[must_use]
pub fn trends(rides: &[RideHeartRate]) -> Vec<DeviceTrends> {
    let mut devices: Vec<&Device> = Vec::new();
    for ride in rides {
        if let SentBy::Device(device) = &ride.sent_by
            && !devices
                .iter()
                .any(|held| held.identifiers() == device.identifiers())
        {
            devices.push(device);
        }
    }
    devices
        .into_iter()
        .map(|device| {
            let own: Vec<&RideHeartRate> = rides
                .iter()
                .filter(|ride| {
                    matches!(&ride.sent_by, SentBy::Device(sent) if sent.identifiers() == device.identifiers())
                })
                .collect();
            let zones = PowerZone::ALL
                .into_iter()
                .filter_map(|zone| {
                    let points: Vec<Point> = own
                        .iter()
                        .filter_map(|ride| match &ride.by_zone {
                            ByZone::Zones(held) => held
                                .iter()
                                .find(|held| held.zone == zone)
                                .map(|held| Point {
                                    on: ride.on(),
                                    beats: held.average.as_u32(),
                                    ftp: ride.ftp,
                                }),
                            _ => None,
                        })
                        .collect();
                    (!points.is_empty()).then(|| (zone, Trended::of(points)))
                })
                .collect();
            let intervals = Trended::of(
                own.iter()
                    .filter_map(|ride| {
                        ride.in_intervals.map(|read| Point {
                            on: ride.on(),
                            beats: read.average.as_u32(),
                            ftp: ride.ftp,
                        })
                    })
                    .collect(),
            );
            DeviceTrends {
                device: device.clone(),
                zones,
                intervals,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Direction, Point, Trended};
    use crate::cycling::{Ftp, FtpProvenance, Watts};

    fn ftp(watts: u32, from: Date) -> Option<Ftp> {
        Ftp::new(Watts::from_u32(watts), from, FtpProvenance::Estimated).ok()
    }

    fn rides(under: Option<Ftp>, rides: &[(Date, u32)]) -> Vec<Point> {
        rides
            .iter()
            .map(|&(on, beats)| Point {
                on,
                beats,
                ftp: under,
            })
            .collect()
    }

    #[test]
    fn a_new_ftp_starts_a_new_line() {
        let mut points = rides(
            ftp(172, date(2026, 7, 22)),
            &[
                (date(2026, 9, 6), 143),
                (date(2026, 9, 16), 144),
                (date(2026, 9, 20), 141),
            ],
        );
        points.extend(rides(
            ftp(177, date(2026, 9, 23)),
            &[(date(2026, 9, 27), 140), (date(2026, 9, 30), 133)],
        ));
        let trended = Trended::of(points);
        let lines: Vec<(usize, bool)> = trended
            .periods
            .iter()
            .map(|period| (period.points.len(), period.line.is_some()))
            .collect();
        assert_eq!(
            lines,
            vec![(3, true), (2, false)],
            "two rides under 177 W fix no line"
        );
    }

    #[test]
    fn a_direction_needs_the_whole_slope_interval_on_one_side_of_zero() {
        let under = ftp(199, date(2024, 9, 15));
        let falling = Trended::of(rides(
            under,
            &[
                (date(2025, 1, 1), 152),
                (date(2025, 1, 8), 149),
                (date(2025, 1, 15), 147),
                (date(2025, 1, 22), 143),
                (date(2025, 1, 29), 141),
            ],
        ));
        assert_eq!(falling.periods[0].direction(), Some(Direction::Falling));

        let scattered = Trended::of(rides(
            under,
            &[
                (date(2025, 1, 1), 145),
                (date(2025, 1, 8), 138),
                (date(2025, 1, 15), 147),
                (date(2025, 1, 22), 136),
            ],
        ));
        assert_eq!(
            scattered.periods[0].direction(),
            Some(Direction::Maintaining)
        );
    }

    #[test]
    fn a_range_keeps_the_whole_period_s_line() {
        let under = ftp(172, date(2026, 7, 22));
        let trended = Trended::of(rides(
            under,
            &[
                (date(2026, 8, 5), 143),
                (date(2026, 8, 19), 134),
                (date(2026, 9, 16), 144),
            ],
        ));
        let shown = trended.within(super::Range {
            from: date(2026, 9, 1),
            to: date(2026, 9, 30),
        });
        assert_eq!(shown.periods[0].points.len(), 1);
        assert_eq!(shown.periods[0].rides, 3);
        assert_eq!(shown.periods[0].line, trended.periods[0].line);
    }
}
