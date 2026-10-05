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
    measure::PositiveDuration,
    sequence::NonEmpty,
};
use serde::Deserialize;

/// Peloton's own id for the *Cool Down Ride* class type.
///
/// A source's identifier, so it lives with the source's adapter (§ II.3). Read
/// off the browse endpoint's own `class_types` list on 2026-09-05, beside
/// *Cool Down Walking*, *Cool Down Running* and their kin — which is why it is
/// stated rather than derived from the word "cool down".
pub(crate) const COOL_DOWN_RIDE_CLASS_TYPE: &str = "a1fa617f3ba14c0a8c25468d5c88b3ea";

/// Peloton's own id for the *Power Zone* class type.
///
/// **It does not separate the formats**, which is why the series below exist.
/// *Power Zone Endurance Ride*, *Power Zone Ride* and *Power Zone Max Ride* all
/// carry this one type id, verified across the operator's 309 landed cycling
/// workouts on 2026-09-20. A browse filtered on it alone returns all three.
pub(crate) const POWER_ZONE_CLASS_TYPE: &str = "665395ff3abf4081bf315686227d1a51";

/// Peloton's series for its *Power Zone Endurance Ride* classes.
///
/// The lower-intensity holding ride (#180). A series is Peloton's own statement
/// that two classes are the same kind of thing — the same signal
/// [`super::sessions`] uses to recognise an FTP test — so the format is read
/// from the source rather than from the words in a title.
pub(crate) const POWER_ZONE_ENDURANCE_SERIES: &str = "0f63c48726fa4533a928cae5358d94d7";

/// Peloton's series for its plain *Power Zone Ride* classes.
///
/// The higher-intensity holding ride (#180). It excludes the *Max* rides and
/// the themed ones — Pop, Hip Hop, Rock, House, EDM — which carry series of
/// their own, and that is the operator's choice: he named *"a regular 45 minute
/// power zone ride"* on 2026-09-20.
pub(crate) const POWER_ZONE_SERIES: &str = "9fde039566054ea499130bed1c289eb3";

/// How many classes one page of the catalogue walk asks for.
///
/// A page size and not a limit: the walk continues until the source says there
/// is no further page, so this trades round trips against response size and
/// nothing else.
///
/// **It replaced a single oversized page.** Until #246 the candidates for a
/// holding ride were one browse of a hundred classes, sized large enough never
/// to run out — because running out would have meant offering a class already
/// ridden. A walk cannot run out, so the question does not arise.
const CATALOGUE_PAGE: u32 = 100;

/// Whose cool-down ride is used when a class's own instructor has none.
///
/// Matt Wilpers, by the operator's instruction on 2026-09-06. Four of the twelve
/// instructors these programmes use publish no five-minute cool-down ride —
/// the co-taught "Denis &amp; Matt", Christian Vande Velde, Charlotte
/// Weidenbach and Erik Jäger — so a session taught by one of them would
/// otherwise end with nowhere to ride the five minutes he rides anyway.
///
/// **His id, not his class.** Which cool-down is his most recent is resolved by
/// the same query as everyone else's, so the fallback does not go stale where
/// the ordinary path stays current.
const FALLBACK_INSTRUCTOR: &str = "304389e2bfe44830854e071bffc137c9";

/// How long a cool-down ride is, in seconds.
///
/// Five minutes, and it is a filter rather than a preference: the operator rides
/// the five-minute one, and the browse endpoint takes an exact duration.
const COOL_DOWN_SECONDS: u64 = 300;

/// A class as the browse endpoint lists it — enough to name it, stack it, and
/// hold it in the catalogue.
///
/// Not a [`ClassSession`]: nothing here is fetched in enough detail to say what
/// it prescribes, and a cool-down does not need to be. It has no zones by
/// construction, and [`PelotonClasses::class`] is what reads them.
///
/// **The last three fields are the catalogue's**, and they are here rather than
/// on a second type because this is already the concept — a class as the
/// listing gives it. A cool-down search ignores them; the catalogue stores them
/// so that a class can be chosen by its format, its length and its age without
/// reading what it prescribes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSummary {
    pub id: String,
    pub title: String,
    pub duration_seconds: u64,
    /// Which format it is, as Peloton groups them. `None` where the listing
    /// omits it, and a class with none is never chosen for a role a series
    /// defines.
    pub series: Option<String>,
    /// Who teaches it, by id. The name is not listed here, only on the detail.
    pub instructor: Option<String>,
    /// When it first aired, in Unix seconds. The catalogue's order, and so what
    /// "newest" means without a second request.
    pub aired_at: Option<i64>,
}

/// Read the first class out of a browse response.
///
/// # Errors
///
/// [`SourceError::Malformed`] where the response cannot be read at all. An
/// empty list is `None` rather than an error: an instructor with no cool-down
/// ride is a fact about the catalogue, not a fault.
pub fn cool_down_from(instructor: &str, body: &str) -> Result<Option<ClassSummary>, SourceError> {
    let listing: Listing = serde_json::from_str(body).map_err(|error| SourceError::Malformed {
        detail: format!(
            "the cool-down search for instructor {instructor} could not be read: {error}"
        ),
    })?;
    Ok(listing.data.into_iter().next().map(Listed::summary))
}

/// One page of the catalogue, and whether another follows.
///
/// **Every series and every length.** The catalogue holds every power zone
/// class Peloton serves, and which of them suits a slot is a later question
/// answered against the stored rows. The only filter is the one the query
/// already applied, and it is applied again here because this adapter does not
/// own the endpoint and cannot promise the parameter was honoured.
///
/// A class the listing gives no class type for is kept. Absence is not a
/// statement that it is the wrong kind: the query asked for one type, and a
/// listing that omits the field has not contradicted it.
///
/// **A full page continues the walk even with no `show_next`.** See the note in
/// the body: the field is the source's own answer where it gives one, and a
/// full page is the fallback where it does not.
///
/// # Errors
///
/// [`SourceError::Malformed`] where the response cannot be read.
pub fn power_zone_from(body: &str) -> Result<CataloguePage, SourceError> {
    let listing: Listing = serde_json::from_str(body).map_err(|error| SourceError::Malformed {
        detail: format!("a page of the power zone catalogue could not be read: {error}"),
    })?;
    // **Two reasons to keep walking, because one of them is an assumption.**
    // `show_next` is the source's own statement and is what the workout walk
    // reads — but that is a different endpoint, and this adapter has never seen
    // this one answer. If the field is absent the walk would stop at the first
    // page and a hundred classes would look like the whole library, which is
    // exactly the wrong default `CLAUDE.md` warns a stub cannot catch. So a
    // page that came back *full* also continues: the source filled the limit it
    // was given, which it would not have done if that were the end.
    //
    // Counted before filtering, because the question is whether the endpoint
    // had more to give, not how many of them were power zone classes.
    let served = listing.data.len();
    let more = listing.show_next || served >= usize::try_from(CATALOGUE_PAGE).unwrap_or(usize::MAX);
    let classes = listing
        .data
        .into_iter()
        .filter(|found| {
            found
                .class_type_ids
                .as_ref()
                .is_none_or(|types| types.iter().any(|kind| kind == POWER_ZONE_CLASS_TYPE))
        })
        .filter(Listed::is_rideable)
        .map(Listed::summary)
        .collect();
    Ok(CataloguePage { classes, more })
}

/// What one page of the catalogue walk yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CataloguePage {
    pub classes: Vec<ClassSummary>,
    /// Whether another page follows: the source said so, or it filled the page
    /// it was given.
    pub more: bool,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    data: Vec<Listed>,
    /// Whether another page follows.
    ///
    /// **The source's own answer, not `page` against `page_count`.** The
    /// workout walk reads the same field for the same reason: a derived answer
    /// would be this adapter deciding something the endpoint already said.
    #[serde(default)]
    show_next: bool,
}

#[derive(Deserialize)]
struct Listed {
    id: String,
    title: String,
    duration: u64,
    /// Which kind of class it is, as the source groups them.
    ///
    /// Optional because the browse endpoint is not ours. A class the listing
    /// gives no series for is held with none, and so is never offered for a
    /// role that is defined by a series — which is the honest answer, since
    /// what format it is has not been stated.
    #[serde(default)]
    series_id: Option<String>,
    /// Which kinds of class it is. Peloton lists several per class, so the
    /// power zone type is looked for among them rather than compared to one.
    #[serde(default)]
    class_type_ids: Option<Vec<String>>,
    #[serde(default)]
    instructor_id: Option<String>,
    /// Unix seconds. Nullable in principle, and a class without one simply
    /// sorts last rather than borrowing the clock.
    #[serde(default)]
    original_air_time: Option<i64>,
}

impl Listed {
    /// Whether this is a class at all.
    ///
    /// **A class of no length is not a class**, which is migration 0066's
    /// ruling about an activity of no duration, applied to the other side of
    /// the same question. One in a page is dropped rather than failing the
    /// walk: the catalogue's job is to hold what can be ridden, and a page
    /// refused over one unrideable entry would hold nothing.
    ///
    /// A nameless class goes the same way, because what the catalogue shows the
    /// operator is the title and a blank one names nothing.
    fn is_rideable(&self) -> bool {
        self.duration > 0 && !self.title.trim().is_empty()
    }

    fn summary(self) -> ClassSummary {
        ClassSummary {
            id: self.id,
            title: self.title,
            duration_seconds: self.duration,
            series: self.series_id,
            instructor: self.instructor_id,
            aired_at: self.original_air_time,
        }
    }
}

/// What one class prescribes, before it is joined to any other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassSession {
    pub id: String,
    pub title: String,
    /// Who teaches it, and what they are called.
    ///
    /// **The id, because a name is not an identifier** — this module already
    /// says so about class titles, and the same is true one level up: two
    /// instructors can share a name and one instructor can change theirs. The
    /// name is carried beside it for reporting only. `None` where the payload
    /// omits the instructor, which no class read so far does.
    pub instructor: Option<Instructor>,
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
            instructor: detail.ride.instructor,
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
        instructor: detail.ride.instructor,
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

/// Who teaches a class.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Instructor {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct RideMeta {
    title: String,
    #[serde(default)]
    instructor: Option<Instructor>,
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

    /// The cool-down ride a session ends with.
    ///
    /// The class's own instructor where they have one, and
    /// [`FALLBACK_INSTRUCTOR`]'s where they do not — the operator, 2026-09-06,
    /// asked what a session by an instructor with no cool-down should do:
    /// *"fall back to Matt Wilpers"*.
    ///
    /// **The fallback is here rather than in [`cool_down_for`](Self::cool_down_for)**,
    /// which keeps answering what the source actually says. One of the two is a
    /// fact about Peloton's catalogue and the other is the operator's choice
    /// about his own training, and a function that quietly did both would make
    /// the first untestable.
    ///
    /// # Errors
    ///
    /// [`SourceError`] as [`cool_down_for`](Self::cool_down_for) gives it, and
    /// [`SourceError::Malformed`] if even the fallback has no cool-down ride —
    /// which would mean the catalogue is not what this adapter was built
    /// against, rather than that this session has none.
    pub async fn cool_down_after(
        &self,
        instructor: Option<&str>,
    ) -> Result<ClassSummary, SourceError> {
        if let Some(instructor) = instructor
            && instructor != FALLBACK_INSTRUCTOR
            && let Some(theirs) = self.cool_down_for(instructor).await?
        {
            return Ok(theirs);
        }
        self.cool_down_for(FALLBACK_INSTRUCTOR)
            .await?
            .ok_or_else(|| SourceError::Malformed {
                detail: format!(
                    "no five-minute cool-down ride was found for instructor \
                     {FALLBACK_INSTRUCTOR}, who is the one every session falls back to"
                ),
            })
    }

    /// The most recent five-minute cool-down ride taught by one instructor.
    ///
    /// **The operator's own app filters, as a query.** Cycling, five minutes,
    /// class type *Cool Down Ride*, that instructor, newest first — which is
    /// what he does by hand: *"I just use the most recent 5 minute cool down
    /// ride from the same instructor"* (2026-09-05).
    ///
    /// **A query rather than a table**, and not for tidiness: what he rides is
    /// the *most recent* one, so a table of class ids would be wrong as soon as
    /// Peloton published another. The answer is resolved when a session is
    /// delivered and recorded in what was delivered, so a stacked session stays
    /// reproducible without the lookup being repeatable (§ 12).
    ///
    /// `None` is an instructor with no such class — a real answer, and the one
    /// the co-taught "Denis &amp; Matt" may well give. The caller decides what a
    /// session missing its cool-down means; § 37 says it is not quietly dropped.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source is unreachable, refuses the token, or
    /// answers something this cannot read.
    pub async fn cool_down_for(
        &self,
        instructor: &str,
    ) -> Result<Option<ClassSummary>, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}/api/v2/ride/archived", self.api_base))
            .bearer_auth(bearer)
            .header("Peloton-Platform", "web")
            .query(&[
                ("browse_category", "cycling"),
                ("duration", &COOL_DOWN_SECONDS.to_string()),
                ("class_type_id", COOL_DOWN_RIDE_CLASS_TYPE),
                ("instructor_id", instructor),
                ("sort_by", "original_air_time"),
                ("desc", "true"),
                ("limit", "1"),
            ])
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
                detail: format!(
                    "the cool-down search for instructor {instructor} answered {status}"
                ),
            });
        }
        let body = response
            .text()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;
        cool_down_from(instructor, &body)
    }

    /// One page of every power zone class Peloton serves, newest first.
    ///
    /// **No duration and no series.** The catalogue holds the whole class
    /// library and the choosing happens against the stored rows.
    /// The operator, 2026-10-05: *"I don't just want the classes I've already
    /// ridden, I want all power classes"*.
    ///
    /// Pages are walked until [`CataloguePage::more`] is false.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source is unreachable, refuses the token, or
    /// answers something this cannot read.
    pub async fn power_zone_page(&self, page: u32) -> Result<CataloguePage, SourceError> {
        let bearer = self.auth.bearer().await?;
        let response = self
            .client()?
            .get(format!("{}/api/v2/ride/archived", self.api_base))
            .bearer_auth(bearer)
            .header("Peloton-Platform", "web")
            .query(&[
                ("browse_category", "cycling"),
                ("class_type_id", POWER_ZONE_CLASS_TYPE),
                ("sort_by", "original_air_time"),
                ("desc", "true"),
                ("limit", &CATALOGUE_PAGE.to_string()),
                ("page", &page.to_string()),
            ])
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
                detail: format!("page {page} of the power zone catalogue answered {status}"),
            });
        }
        let body = response
            .text()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
            })?;
        power_zone_from(&body)
    }

    /// What `/api/ride/{id}/details` serves for one class, as served.
    ///
    /// **The bytes, not the reading of them.** The catalogue keeps this so that
    /// a corrected reader costs a re-read of the store rather than a re-fetch
    /// of the whole library — and this module's own history is the argument:
    /// the transcription it replaced had a cool-down five minutes out, and
    /// finding that out again should not need the network.
    ///
    /// # Errors
    ///
    /// [`SourceError::Unauthorised`] where the token is refused,
    /// [`SourceError::Unavailable`] where Peloton is not answering, and
    /// [`SourceError::Malformed`] where the body cannot be read as text.
    pub async fn detail(&self, id: &str) -> Result<String, SourceError> {
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
        response
            .text()
            .await
            .map_err(|error| SourceError::Malformed {
                detail: error.to_string(),
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
        let body = self.detail(id).await?;
        derive(id, &body)
    }
}
