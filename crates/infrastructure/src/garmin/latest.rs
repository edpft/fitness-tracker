//! Garmin's metrics that it serves only as they stand now (#269): fitness age,
//! race predictions and personal records.
//!
//! **One request is the whole of it**, as for Peloton's account
//! ([`crate::peloton::paired`]). None of these has a history to walk: on
//! 2026-10-21 fitness age answered identically for any date asked, and the
//! predictions and records answer only for now. Landing each run keeps what
//! each stood at when it changed, since an unchanged answer lands nothing — so
//! the history starts the day this is first collected.

use std::sync::Arc;

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, RawPayload, SourceRecordId, Watermark,
};
use jiff::civil::Date;

use super::{auth::GarminAuth, connect::Connect};

/// A metric Garmin serves only as it stands now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GarminLatest {
    /// Fitness age, and what it is made of.
    FitnessAge,
    /// Predicted 5K, 10K, half marathon and marathon times.
    RacePredictions,
    /// The best efforts on record, for each distance and lift.
    PersonalRecords,
}

impl GarminLatest {
    /// The endpoint, without the account's name or the date: what its records
    /// are tagged with.
    pub const fn endpoint(self) -> &'static str {
        match self {
            Self::FitnessAge => "/fitnessage-service/fitnessage",
            Self::RacePredictions => "/metrics-service/metrics/racepredictions/latest",
            Self::PersonalRecords => "/personalrecord-service/personalrecord/prs",
        }
    }

    /// The path that asks for it, for the account named `name`.
    ///
    /// **Fitness age is addressed by a date and answers for now whichever is
    /// given**, so it is given today's.
    fn path(self, name: &str, today: Date) -> String {
        match self {
            Self::FitnessAge => format!("{}/{today}", self.endpoint()),
            Self::RacePredictions | Self::PersonalRecords => {
                format!("{}/{name}", self.endpoint())
            }
        }
    }
}

/// One of Garmin's metrics, as it stands.
#[derive(Debug)]
pub struct GarminLatestOf {
    latest: GarminLatest,
    connect: Connect,
    today: Date,
}

impl GarminLatestOf {
    pub fn new(
        latest: GarminLatest,
        api_base: impl Into<String>,
        auth: impl Into<Arc<GarminAuth>>,
        today: Date,
    ) -> Self {
        Self {
            latest,
            connect: Connect::new(api_base, auth),
            today,
        }
    }
}

impl WorkoutEventSource for GarminLatestOf {
    /// One request is the whole of it.
    type Resume = ();

    async fn fetch(
        &self,
        _since: Option<Watermark>,
        _resume: Option<()>,
    ) -> Result<EventBatch<()>, SourceError> {
        let malformed = |detail: String| SourceError::Malformed { detail };
        let name = self.connect.name().await?;
        let body = self
            .connect
            .get(&self.latest.path(&name, self.today), &[])
            .await?;
        let endpoint = Endpoint::try_from(self.latest.endpoint())
            .map_err(|error| malformed(error.to_string()))?;
        let event = crate::revision::json_event(
            // **The account**, since there is one of each per account: a
            // changed answer is a new version of the same record.
            SourceRecordId::try_from(name).map_err(|error| malformed(error.to_string()))?,
            // As it stands: no event, and no time it happened.
            EventProvenance::new(endpoint, EventKind::Updated, None).into(),
            RawPayload::try_from(body).map_err(|error| malformed(error.to_string()))?,
        );
        Ok(EventBatch {
            events: vec![event],
            resume: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::GarminLatest;
    use domain::landing::Endpoint;
    use jiff::civil::date;

    /// The requests a live run composes, which no stub can catch.
    #[test]
    fn each_is_asked_for_where_garmin_serves_it() {
        let today = date(2026, 10, 21);
        assert_eq!(
            GarminLatest::FitnessAge.path("someone", today),
            "/fitnessage-service/fitnessage/2026-10-21"
        );
        assert_eq!(
            GarminLatest::RacePredictions.path("someone", today),
            "/metrics-service/metrics/racepredictions/latest/someone"
        );
        assert_eq!(
            GarminLatest::PersonalRecords.path("someone", today),
            "/personalrecord-service/personalrecord/prs/someone"
        );
        for latest in [
            GarminLatest::FitnessAge,
            GarminLatest::RacePredictions,
            GarminLatest::PersonalRecords,
        ] {
            assert!(Endpoint::try_from(latest.endpoint()).is_ok());
        }
    }
}
