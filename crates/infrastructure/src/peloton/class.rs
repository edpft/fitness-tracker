//! Reading a class out of Peloton, and deriving what it prescribes.
//!
//! **This replaces a transcription from screenshots.** `docs/cycling-peak-your-power-zones.md`
//! was read off the app by hand, and one of its cool-downs was five minutes out
//! — the class's own minute plus a separate ride the operator does after it,
//! silently merged. What is derived here is the API's own arithmetic.
//!
//! **The API serves classes and not programmes** (decision 0033). Which class
//! sits at which microcycle and session is the operator's to say; everything
//! below is about one class in isolation.
//!
//! ## The shape of the answer
//!
//! ```text
//! segments.segment_list        Warm Up · the ride · Cool Down, in order, with lengths
//! target_metrics_data          each entry a start/end offset and a power zone
//! is_ftp_test                  the one class with no zone plan at all
//! ```
//!
//! **`offsets.end` is inclusive**, so a run lasts `end - start + 1` seconds.
//! Reading it as exclusive produces one-second gaps between every interval that
//! look like real ones and are not.
//!
//! **The zone plan covers the warm-up too**, so it is clipped to the ride window
//! before anything is derived from it. Clipped that way, every riding class in
//! Build and Boost Your Base tiles exactly: the zone runs sum to the ride
//! segment's own length, to the second. A class that does not tile is a class
//! this reader has misunderstood, so [`ClassSession::tiles`] says which.
//!
//! **A class is not always a session.** The FTP warm-up is ten minutes with no
//! ride; the FTP test is twenty minutes of ride with no warm-up and no zones.
//! Neither builds a [`CyclingSession`] alone and both are half of one, which is
//! why this yields a class and the joining happens above.

use application::SourceError;
use domain::{
    cycling::{Interval, PowerZone, Ride},
    gym::{PositiveDuration, sequence::NonEmpty},
};
use serde::Deserialize;

/// What one class prescribes, before it is joined to any other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSession {
    pub id: String,
    pub title: String,
    /// Seconds. Not a [`PositiveDuration`]: the FTP test's is zero.
    pub warm_up_seconds: u64,
    pub cool_down_seconds: u64,
    /// The working part, where the class has one. Absent for the FTP warm-up,
    /// which is all warm-up.
    pub ride: Option<Ride>,
    /// The ride segment's own length, for the tiling check.
    pub ride_seconds: u64,
    pub is_ftp_test: bool,
}

impl ClassSession {
    /// Whether the zone plan accounts for the whole ride.
    ///
    /// True of every riding class read so far. **False is not a Peloton
    /// oddity, it is this reader having misread one** — except for the FTP
    /// test, which carries no zone plan by design (decision 0025).
    #[must_use]
    pub fn tiles(&self) -> bool {
        let zoned: u64 = match &self.ride {
            Some(Ride::Intervals(intervals)) => {
                intervals.iter().map(|i| i.duration().as_seconds()).sum()
            }
            Some(Ride::Effort(duration)) => duration.as_seconds(),
            None => 0,
        };
        zoned == self.ride_seconds
    }

    /// Time at each zone, in seconds. Empty for the test, which has no zones.
    #[must_use]
    pub fn time_in_zone(&self) -> Vec<(PowerZone, u64)> {
        self.ride
            .as_ref()
            .map(Ride::time_in_zone)
            .unwrap_or_default()
    }
}

/// Derive a class from the JSON `/api/ride/{id}/details` serves.
///
/// # Errors
///
/// [`SourceError::Malformed`] where the response cannot be read, or where a
/// class has no segments at all — which would mean Peloton has changed the
/// shape rather than that this class is unusual.
pub fn derive(id: &str, body: &str) -> Result<ClassSession, SourceError> {
    let detail: ClassDetail =
        serde_json::from_str(body).map_err(|error| SourceError::Malformed {
            detail: format!("class {id} could not be read: {error}"),
        })?;
    let segments = &detail.segments.segment_list;
    if segments.is_empty() {
        return Err(SourceError::Malformed {
            detail: format!("class {id} carries no segments"),
        });
    }

    // Segment lengths are contiguous and in order, so the boundaries fall out
    // of a running total rather than being stated.
    let mut offset = 0_u64;
    let (mut warm_up, mut cool_down, mut ride_window) = (0, 0, None);
    for segment in segments {
        let end = offset + segment.length;
        match segment.name.as_str() {
            "Warm Up" => warm_up += segment.length,
            "Cool Down" => cool_down += segment.length,
            _ => ride_window = Some((offset, end)),
        }
        offset = end;
    }
    let Some((from, to)) = ride_window else {
        // All warm-up and no ride: the FTP warm-up class is exactly this.
        return Ok(ClassSession {
            id: id.to_owned(),
            title: detail.ride.title,
            warm_up_seconds: warm_up,
            cool_down_seconds: cool_down,
            ride: None,
            ride_seconds: 0,
            is_ftp_test: detail.is_ftp_test,
        });
    };

    let mut runs: Vec<(u8, u64, u64)> = Vec::new();
    let mut metrics = detail.target_metrics_data.target_metrics;
    metrics.sort_by_key(|metric| metric.offsets.start);
    for metric in &metrics {
        let Some(zone) = metric.metrics.first().map(|band| band.lower) else {
            continue;
        };
        // Inclusive end, clipped to the ride.
        let (start, end) = (
            metric.offsets.start.max(from),
            (metric.offsets.end + 1).min(to),
        );
        if end <= start {
            continue;
        }
        match runs.last_mut() {
            Some(last) if last.0 == zone && last.2 == start => last.2 = end,
            _ => runs.push((zone, start, end)),
        }
    }

    let intervals = runs
        .into_iter()
        .map(|(zone, start, end)| {
            let zone = PowerZone::try_from(zone).map_err(|error| SourceError::Malformed {
                detail: format!("class {id} names a zone we do not know: {error}"),
            })?;
            let duration = PositiveDuration::from_seconds(end - start).map_err(|_| {
                SourceError::Malformed {
                    detail: format!("class {id} has an interval of no length"),
                }
            })?;
            Ok(Interval::new(zone, duration))
        })
        .collect::<Result<Vec<_>, SourceError>>()?;

    // **No intervals and a ride segment is the test**, not an empty class. A
    // zone is a share of FTP and this ride is what measures FTP, so prescribing
    // it in zones would be circular (decision 0025).
    let ride = if intervals.is_empty() {
        let duration =
            PositiveDuration::from_seconds(to - from).map_err(|_| SourceError::Malformed {
                detail: format!("class {id} has a ride of no length"),
            })?;
        Ride::Effort(duration)
    } else {
        Ride::Intervals(
            NonEmpty::new(intervals).map_err(|_| SourceError::Malformed {
                detail: format!("class {id} produced no intervals"),
            })?,
        )
    };

    Ok(ClassSession {
        id: id.to_owned(),
        title: detail.ride.title,
        warm_up_seconds: warm_up,
        cool_down_seconds: cool_down,
        ride: Some(ride),
        ride_seconds: to - from,
        is_ftp_test: detail.is_ftp_test,
    })
}

#[derive(Deserialize)]
struct ClassDetail {
    ride: RideMeta,
    segments: Segments,
    #[serde(default)]
    target_metrics_data: TargetMetricsData,
    #[serde(default)]
    is_ftp_test: bool,
}

#[derive(Deserialize)]
struct RideMeta {
    title: String,
}

#[derive(Deserialize)]
struct Segments {
    segment_list: Vec<Segment>,
}

#[derive(Deserialize)]
struct Segment {
    name: String,
    length: u64,
}

#[derive(Deserialize, Default)]
struct TargetMetricsData {
    #[serde(default)]
    target_metrics: Vec<TargetMetric>,
}

#[derive(Deserialize)]
struct TargetMetric {
    offsets: Offsets,
    #[serde(default)]
    metrics: Vec<Band>,
}

#[derive(Deserialize)]
struct Offsets {
    start: u64,
    end: u64,
}

#[derive(Deserialize)]
struct Band {
    lower: u8,
}

/// Peloton's class endpoint.
///
/// **Constructing this does no I/O**, the rule every adapter here follows.
#[derive(Debug)]
pub struct PelotonClasses {
    api_base: String,
    auth: super::auth::PelotonAuth,
    client: std::sync::OnceLock<Result<reqwest::Client, String>>,
}

impl PelotonClasses {
    pub fn new(api_base: impl Into<String>, auth: super::auth::PelotonAuth) -> Self {
        Self {
            api_base: api_base.into(),
            auth,
            client: std::sync::OnceLock::new(),
        }
    }

    fn client(&self) -> Result<&reqwest::Client, SourceError> {
        self.client
            .get_or_init(|| {
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(30))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .as_ref()
            .map_err(|detail| SourceError::Unavailable {
                detail: detail.clone(),
            })
    }

    /// One class, derived.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorised`] where the token is refused,
    /// [`SourceError::Unavailable`] where Peloton is not answering, and
    /// [`SourceError::Malformed`] where the response cannot be read.
    pub async fn class(&self, id: &str) -> Result<ClassSession, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}/api/ride/{id}/details", self.api_base))
            .bearer_auth(bearer)
            .header("Peloton-Platform", "web")
            .send()
            .await
            .map_err(|error| SourceError::Unavailable {
                detail: error.to_string(),
            })?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(SourceError::Unauthorised);
        }
        if !status.is_success() {
            return Err(SourceError::Unavailable {
                detail: format!("class {id} answered {status}"),
            });
        }
        let body = response
            .text()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;
        derive(id, &body)
    }
}
