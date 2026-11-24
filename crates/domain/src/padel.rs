//! A padel session (#517).
//!
//! **A padel session, not one of the watch's activities** (operator,
//! 2026-11-11: *"no, it's a padel game"*; a session, which holds matches,
//! 2026-11-24: *"yes, session holds matches"*). The watch
//! files it in the same list as a gym session and a ride, and that list is
//! where it was found, not what it is.
//!
//! **It is training done, and nothing prescribes it.** A session occupies its
//! evening without being a slot ([`crate::schedule`]), and its hours count
//! towards what resting heart rate, overnight HRV and stress are read against
//! as a ride's and a gym session's do (operator, 2026-11-11: *"yes, all
//! activity hours"*).
//!
//! **When, how long, and the heart rate the watch recorded over it**: the
//! average and highest it stated, and the readings its activity file holds
//! (#919), as a gym session's are, so a session whose start or end moves can be
//! read again between them (#894). The operator's record holds 16, from
//! 2026-03-22, every one recorded by the Forerunner 165.

use std::fmt;

use jiff::Timestamp;

use crate::{
    canonical::edit::{EditId, EditReason},
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::{MeasuredHeartRate, PositiveDuration},
    normalised::{NormalisedEntity, StartedAt},
};

/// Everything one session's record says, on its way into [`PadelSession`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadelSessionRecord {
    pub started_at: StartedAt,
    pub duration: PositiveDuration,
    /// Absent where the watch stated none, as a gym session's is.
    pub heart_rate: Option<MeasuredHeartRate>,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
}

/// One session of padel, as the watch that recorded it told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadelSession {
    started_at: StartedAt,
    duration: PositiveDuration,
    heart_rate: Option<MeasuredHeartRate>,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
}

impl PadelSession {
    pub fn new(record: PadelSessionRecord) -> Self {
        Self {
            started_at: record.started_at,
            duration: record.duration,
            heart_rate: record.heart_rate,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
        }
    }

    pub const fn started_at(&self) -> &StartedAt {
        &self.started_at
    }

    pub const fn duration(&self) -> PositiveDuration {
        self.duration
    }

    pub const fn heart_rate(&self) -> Option<&MeasuredHeartRate> {
        self.heart_rate.as_ref()
    }

    pub const fn landed_as(&self) -> LandingRecordId {
        self.landed_as
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

impl NormalisedEntity for PadelSession {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for PadelSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — padel, {}",
            self.started_at.wall_clock().datetime(),
            self.duration
        )
    }
}

/// The operator's change to when one padel session started and how long it
/// lasted (#894).
///
/// **Both, always.** The watch is the only source, so there is no source's
/// start or end to choose: he states the two, as every session is held, a
/// start and a duration (operator, 2026-11-24). A change to the end alone is
/// the same start and a new length; a change to the start alone keeps the
/// end with a new length. The latest change is the one in force, and undoing
/// it brings back the one before, or the watch's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadelEdit {
    pub id: EditId,
    pub asserted_at: Timestamp,
    pub reason: EditReason,
    pub started: StartedAt,
    pub duration: PositiveDuration,
}

/// Why a change to when a padel session started and ended is not made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unmoved {
    #[error("Not changed: it would end before it started.")]
    EndsBeforeItStarts,
    #[error("Not changed: it would start on another day.")]
    MovesDay,
    #[error("nothing was changed")]
    Unchanged,
}

/// One padel session as it was played: the watch's account of it, with the
/// operator's change over it where he made one.
///
/// **Its heart rate is read again only between bounds he moved inward**
/// (operator, 2026-11-24): readings outside them are dropped and the summary
/// read from those kept, as a gym session's is. Moved outward, there is
/// nothing more to read, so the watch's figures stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayedPadelSession {
    session: SourceRecordId,
    recorded: StartedAt,
    started_at: StartedAt,
    duration: PositiveDuration,
    heart_rate: Option<MeasuredHeartRate>,
    change: Option<PadelEdit>,
}

impl PlayedPadelSession {
    /// The session as the watch recorded it, `session` being the watch's
    /// own name for it.
    #[must_use]
    pub fn recorded(
        session: SourceRecordId,
        started_at: StartedAt,
        duration: PositiveDuration,
        heart_rate: Option<MeasuredHeartRate>,
    ) -> Self {
        Self {
            session,
            recorded: started_at.clone(),
            started_at,
            duration,
            heart_rate,
            change: None,
        }
    }

    /// The session with the latest of `edits` in force over the watch's
    /// account. One that would move it to another day is left out, since
    /// [`Self::moved`] never asserts one.
    #[must_use]
    pub fn changed(self, edits: impl IntoIterator<Item = PadelEdit>) -> Self {
        let Some(edit) = edits
            .into_iter()
            .max_by_key(|edit| (edit.asserted_at, edit.id))
        else {
            return self;
        };
        if edit.started.wall_clock().date() != self.recorded.wall_clock().date() {
            return self;
        }
        let start = edit.started.instant().as_second() - self.recorded.instant().as_second();
        let end = start + i64::try_from(edit.duration.as_seconds()).unwrap_or(i64::MAX);
        let recorded = i64::try_from(self.duration.as_seconds()).unwrap_or(i64::MAX);
        // Only a bound moved inward reads the readings again: one moved
        // outward, or left where the watch put it, cuts nothing, since the
        // watch writes readings a second or so past the length it states.
        let from = if start > 0 { start } else { i64::MIN };
        let to = if end < recorded { end } else { i64::MAX };
        Self {
            heart_rate: self
                .heart_rate
                .map(|heart_rate| heart_rate.between(from, to)),
            started_at: edit.started.clone(),
            duration: edit.duration,
            change: Some(edit),
            ..self
        }
    }

    /// What the operator asks of when it started and ended, as a change
    /// holds it: the start, read in the zone the session started in, and the
    /// length from it to the end.
    ///
    /// # Errors
    ///
    /// [`Unmoved`]: an end at or before the start, a start on another day,
    /// or both as they read now.
    pub fn moved(
        &self,
        start: Timestamp,
        end: Timestamp,
    ) -> Result<(StartedAt, PositiveDuration), Unmoved> {
        let started = StartedAt::new(start, self.started_at.zone().clone());
        if started.wall_clock().date() != self.started_at.wall_clock().date() {
            return Err(Unmoved::MovesDay);
        }
        let seconds = u64::try_from(end.as_second() - start.as_second())
            .map_err(|_| Unmoved::EndsBeforeItStarts)?;
        let duration =
            PositiveDuration::from_seconds(seconds).map_err(|_| Unmoved::EndsBeforeItStarts)?;
        if started == self.started_at && duration == self.duration {
            return Err(Unmoved::Unchanged);
        }
        Ok((started, duration))
    }

    /// The watch's name for it, which a change is held against.
    pub const fn session(&self) -> &SourceRecordId {
        &self.session
    }

    pub const fn started_at(&self) -> &StartedAt {
        &self.started_at
    }

    pub const fn duration(&self) -> PositiveDuration {
        self.duration
    }

    /// When it ended: its start and how long it lasted, read as a time for
    /// the page.
    pub fn ended(&self) -> Timestamp {
        let seconds = i64::try_from(self.duration.as_seconds()).unwrap_or(i64::MAX);
        self.started_at
            .instant()
            .checked_add(jiff::SignedDuration::from_secs(seconds))
            .unwrap_or(Timestamp::MAX)
    }

    pub const fn heart_rate(&self) -> Option<&MeasuredHeartRate> {
        self.heart_rate.as_ref()
    }

    /// The change in force on it, where there is one.
    pub const fn change(&self) -> Option<&PadelEdit> {
        self.change.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::{
        BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, HeartRateSummary,
    };
    use crate::normalised::OperatorZone;
    use crate::sequence::NonEmpty;

    fn at(instant: &str) -> Timestamp {
        instant.parse().expect("an instant")
    }

    fn london(instant: &str) -> StartedAt {
        StartedAt::new(
            at(instant),
            OperatorZone::try_from("Europe/London".to_owned()).expect("a zone"),
        )
    }

    fn seconds(count: u64) -> PositiveDuration {
        PositiveDuration::from_seconds(count).expect("a length")
    }

    fn beats(count: u32) -> BeatsPerMinute {
        BeatsPerMinute::new(count).expect("a rate")
    }

    /// The session of 22 March 2026 as the watch recorded it, from 19:12:44
    /// for 3,030 seconds, with readings at its start, ten minutes in, fifty
    /// minutes in and a second past the length it stated, as the watch's own
    /// recording of that session has.
    fn twenty_second_of_march() -> PlayedPadelSession {
        let readings =
            [(0, 100), (600, 150), (3000, 90), (3031, 95)].map(|(at, rate)| HeartRateSample {
                at: Duration::from_seconds(at),
                beats_per_minute: beats(rate),
            });
        PlayedPadelSession::recorded(
            SourceRecordId::try_from("1".to_owned()).expect("an id"),
            london("2026-03-22T19:12:44Z"),
            seconds(3030),
            Some(MeasuredHeartRate::new(
                HeartRateSummary::new(beats(120), beats(150)),
                Some(HeartRateSeries::new(NonEmpty::of(
                    readings[0],
                    readings[1..].to_vec(),
                ))),
            )),
        )
    }

    fn edit(id: i64, asserted: &str, started: &str, duration: u64) -> PadelEdit {
        PadelEdit {
            id: EditId::from(id),
            asserted_at: at(asserted),
            reason: EditReason::try_from("pressed the watch late".to_owned()).expect("a reason"),
            started: london(started),
            duration: seconds(duration),
        }
    }

    /// **A start of 19:00 reads 19:00 and the end stays**: the change is
    /// the start and the length from it to the watch's end.
    #[test]
    fn a_stated_start_keeps_the_end() {
        let session = twenty_second_of_march();
        let (started, duration) = session
            .moved(at("2026-03-22T19:00:00Z"), session.ended())
            .expect("bounds it can take");
        assert_eq!(started, london("2026-03-22T19:00:00Z"));
        assert_eq!(duration, seconds(3030 + 764));

        let changed = session.changed([PadelEdit {
            started,
            duration,
            ..edit(1, "2026-11-24T09:00:00Z", "2026-03-22T19:00:00Z", 1)
        }]);
        assert_eq!(changed.started_at(), &london("2026-03-22T19:00:00Z"));
        assert_eq!(changed.ended(), at("2026-03-22T20:03:14Z"));
        assert_eq!(
            changed.heart_rate().map(MeasuredHeartRate::stated),
            Some(HeartRateSummary::new(beats(120), beats(150))),
            "moved outward, the watch's figures stand"
        );
    }

    /// **An end moved inward reads the heart rate again** from the readings
    /// before it.
    #[test]
    fn an_end_moved_inward_reads_the_heart_rate_again() {
        let changed = twenty_second_of_march().changed([edit(
            1,
            "2026-11-24T09:00:00Z",
            "2026-03-22T19:12:44Z",
            1200,
        )]);
        let heart_rate = changed.heart_rate().expect("a heart rate");
        assert_eq!(
            heart_rate.series().map(|series| series.samples().count()),
            Some(2)
        );
        assert_eq!(heart_rate.stated().highest(), beats(150));
    }

    /// **The latest change is in force**, whichever order they come in.
    #[test]
    fn the_latest_change_is_in_force() {
        let first = edit(1, "2026-11-24T09:00:00Z", "2026-03-22T19:00:00Z", 3600);
        let second = edit(2, "2026-11-24T10:00:00Z", "2026-03-22T19:05:00Z", 3000);
        let changed = twenty_second_of_march().changed([second.clone(), first]);
        assert_eq!(changed.change(), Some(&second));
        assert_eq!(changed.started_at(), &london("2026-03-22T19:05:00Z"));
    }

    /// **Bounds that cannot be the session's are refused**, each saying why.
    #[test]
    fn impossible_bounds_are_refused() {
        let session = twenty_second_of_march();
        assert_eq!(
            session.moved(at("2026-03-22T19:30:00Z"), at("2026-03-22T19:30:00Z")),
            Err(Unmoved::EndsBeforeItStarts)
        );
        assert_eq!(
            session.moved(at("2026-03-22T19:30:00Z"), at("2026-03-22T19:00:00Z")),
            Err(Unmoved::EndsBeforeItStarts)
        );
        assert_eq!(
            session.moved(at("2026-03-23T19:00:00Z"), at("2026-03-23T20:00:00Z")),
            Err(Unmoved::MovesDay)
        );
        assert_eq!(
            session.moved(at("2026-03-22T19:12:44Z"), session.ended()),
            Err(Unmoved::Unchanged)
        );
    }
}
