//! Turning Garmin's account of one gym session into a [`MeasuredGymSession`].
//!
//! **Two responses about one thing** (§ 3.1). The activity list states a
//! session's start, duration, device and heart-rate summary; its sets come from
//! a second endpoint fetched per activity, and Garmin names both by the same
//! `activityId`. Neither response is an entity on its own — an activity with no
//! sets is a session that was recorded without them, and a set list with no
//! activity is a set list with no clock, no device and no session.
//!
//! **A gym session, not every activity.** `garmin.activities` lands all 2,271 of
//! the operator's activities because the type is a field on the record rather
//! than a service of its own (#166), so choosing the gym ones is this layer's
//! work. `strength_training` is Garmin's bucket for them; a ride, a run or a
//! swim refuses here as unmodelled rather than being dropped (§ 37), so the
//! 1,723 records this build reads and does nothing with are counted rather than
//! invisible. Nothing derives them yet.
//!
//! **Seventy-two of the 548 strength activities were never on a gym floor.**
//! They are Peloton stretch and mobility classes pushed into Garmin by a sync,
//! identified by the operator against the Connect app on 2026-09-18: 65 by
//! [peloton-to-garmin](https://github.com/philosowaffle/peloton-to-garmin),
//! which files them under device 1, and 7 by the official sync since July 2026,
//! which files them under device 0 and says `PELOTON`. Every one of them is
//! already in Peloton's own record, so counting them here would count them
//! twice (§ 10).

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    gym::{
        Guess, GuessedExercise, Load, MeasuredGymSession, MeasuredSet, Recorded, SignedKg,
    },
    landing::{EventKind, LandedRecord},
    measure::{BeatsPerMinute, Duration, HeartRateSummary, Kg, RepCount},
    normalised::{OperatorZone, RefusalLocus, RefusalReason, StartedAt},
    sequence::NonEmpty,
};
use jiff::civil::DateTime;
use serde::Deserialize;
use serde_json::value::RawValue;

use crate::scribe::Scribe;

use super::{
    account::ActivityAccount,
    mapping::{LoadReading, Mapped, lookup},
};

/// Garmin's bucket for what happens in a gym.
///
/// It is a bucket and not a kind of training: it holds an hour under a barbell
/// and a five-minute stretch alike, which is why the device matters as well as
/// the type.
const GYM: &str = "strength_training";

/// The set type Garmin gives a set that was worked rather than rested.
const ACTIVE: &str = "ACTIVE";

/// What Garmin's classifier says when it could not tell.
const UNKNOWN: &str = "UNKNOWN";

/// One activity as the list states it.
///
/// **Numbers arrive as text**, as they do in [`crate::peloton::payload`] and for
/// the same reason: the characters go to the domain's own parsers, so nothing
/// here rounds a quantity or truncates one through an `f64` on its way into a
/// type that could not have held it.
#[derive(Debug, Deserialize)]
struct Activity {
    #[serde(rename = "activityType")]
    kind: ActivityType,
    #[serde(rename = "startTimeGMT")]
    started_at: Option<String>,
    duration: Option<Box<RawValue>>,
    #[serde(rename = "averageHR")]
    average_heart_rate: Option<Box<RawValue>>,
    #[serde(rename = "maxHR")]
    highest_heart_rate: Option<Box<RawValue>>,
    #[serde(rename = "deviceId")]
    device: Option<i64>,
    manufacturer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActivityType {
    #[serde(rename = "typeKey")]
    key: String,
}

/// One activity's sets, as the second endpoint serves them.
#[derive(Debug, Deserialize)]
struct ExerciseSets {
    #[serde(default, rename = "exerciseSets")]
    sets: Vec<ServedSet>,
}

#[derive(Debug, Deserialize)]
struct ServedSet {
    #[serde(default)]
    exercises: Vec<Candidate>,
    #[serde(rename = "repetitionCount")]
    reps: Option<u32>,
    weight: Option<Box<RawValue>>,
    #[serde(rename = "setType")]
    set_type: Option<String>,
    #[serde(rename = "startTime")]
    started_at: Option<String>,
}

/// One movement the classifier proposed, with how sure it was.
///
/// **The probability is read and not kept.** It is barely discriminating —
/// the median across the operator's corpus is 99.6, and he ruled on 2026-09-18
/// that a low one usually meant the watch *"guessed right and I added a
/// weight"* — so carrying it would put a vendor's scale into our record in
/// exchange for nothing. What the entity says is that the movement is a guess,
/// which is true at every probability Garmin serves.
#[derive(Debug, Deserialize)]
struct Candidate {
    category: Option<String>,
    name: Option<String>,
}

/// Garmin's adapter for gym sessions.
///
/// Stateless, as every translator here is: the zone arrives per call.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminGymTranslator;

impl Translator for GarminGymTranslator {
    type Account = ActivityAccount;
    type Entity = MeasuredGymSession;

    fn translate(
        &self,
        account: &ActivityAccount,
        zone: &OperatorZone,
    ) -> Result<Translation<MeasuredGymSession>, NormalisationError> {
        let record = account.activity();
        let mut scribe = Scribe::new(record);

        let event = crate::store::served_by_a_feed(record.provenance())?;
        match event.kind() {
            EventKind::Deleted => {
                return Ok(Translation::Retraction {
                    of: record.source_record_id().clone(),
                });
            }
            EventKind::Unrecognised(kind) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: format!("event kind {kind:?} is not one we translate"),
                    },
                ));
            }
            EventKind::Updated => {}
        }

        let activity: Activity = match serde_json::from_slice(record.payload().as_bytes()) {
            Ok(activity) => activity,
            Err(error) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: error.to_string(),
                    },
                ));
            }
        };

        if activity.kind.key != GYM {
            return Ok(scribe.only(
                RefusalLocus::Record,
                // Garmin's word for the type, kept verbatim: comparing it
                // against a list we control would make its taxonomy ours, and
                // the detail is then what an operator groups these by.
                RefusalReason::Unmodelled {
                    detail: activity.kind.key,
                },
            ));
        }

        if let Some(sync) = synced_from_peloton(&activity) {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::NotTheInstrument {
                    detail: sync.to_owned(),
                },
            ));
        }

        let started_at = match activity
            .started_at
            .as_deref()
            .map(|stamp| started(stamp, zone))
        {
            Some(Ok(started_at)) => started_at,
            Some(Err(reason)) => return Ok(scribe.only(RefusalLocus::Record, reason)),
            None => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::MissingFigure { figure: "a start" },
                ));
            }
        };

        let Some(duration) = seconds(activity.duration.as_deref()) else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::MissingFigure {
                    figure: "a duration",
                },
            ));
        };

        let heart_rate = heart_rate(&activity);
        let sets = account
            .sets()
            .and_then(|landed| measured_sets(landed, zone, &mut scribe));

        let recorded = match (heart_rate, sets) {
            (Some(heart_rate), Some(sets)) => Recorded::Both { heart_rate, sets },
            (Some(heart_rate), None) => Recorded::HeartRate(heart_rate),
            (None, Some(sets)) => Recorded::Sets(sets),
            // Neither part, so the activity asserts nothing about the session
            // beyond its having happened, which is not an entity here.
            (None, None) => {
                scribe.note(
                    RefusalLocus::Record,
                    RefusalReason::MissingFigure {
                        figure: "a heart rate or a set",
                    },
                );
                return Ok(scribe.nothing_translatable());
            }
        };

        let entity = MeasuredGymSession::new(
            started_at,
            duration,
            recorded,
            record.provenance().clone(),
            record.source_record_id().clone(),
            record.id(),
            account.sets().map(LandedRecord::id),
        );

        Ok(Translation::Entity {
            entity: Box::new(entity),
            refusals: scribe.into_refusals(),
        })
    }
}

/// Which sync pushed this activity in, where one did.
///
/// The device identifications are the operator's, made against the Connect app
/// on 2026-09-18 and derivable from nothing in the payload. Note that
/// peloton-to-garmin's uploads present as a Forerunner 945 in the app, which is
/// the tool's doing rather than a watch he owns.
fn synced_from_peloton(activity: &Activity) -> Option<&'static str> {
    match (activity.device, activity.manufacturer.as_deref()) {
        (Some(1), Some("GARMIN")) => Some("a Peloton class synced in by peloton-to-garmin"),
        (Some(0), Some("PELOTON")) => Some("a Peloton class synced in by Peloton"),
        _ => None,
    }
}

/// The summary Garmin states, where it states one.
///
/// **Absent rather than refused**, which is § 37's distinction between partial
/// data and a hole: a session recorded with no strap on is a session with no
/// heart rate, and 19 of the operator's carry a zero, which is a sensor saying
/// nothing rather than a heart that stopped.
fn heart_rate(activity: &Activity) -> Option<HeartRateSummary> {
    let beats = |stated: Option<&RawValue>| {
        whole(stated)
            .map(ToOwned::to_owned)
            .and_then(|rate| BeatsPerMinute::try_from(rate).ok())
    };
    Some(HeartRateSummary::new(
        beats(activity.average_heart_rate.as_deref())?,
        beats(activity.highest_heart_rate.as_deref())?,
    ))
}

/// The whole part of a number Garmin wrote, as the characters it wrote.
///
/// **Cut at the point rather than cast through an `f64`.** Garmin writes every
/// quantity here with a fraction — `2709.291` seconds, `48000.0` grams, `77.0`
/// beats — and what the domain holds is whole seconds, grams and beats. Cutting
/// the text is exact, keeps the parsing in the newtype that owns the unit, and
/// cannot silently truncate a value into a type too small for it (§ 26).
///
/// `None` for an absent field, a null, or anything that is not a number
/// written plainly — an exponent or a sign is not something this build has seen
/// Garmin write, and reading one wrongly would be worse than refusing it.
fn whole(stated: Option<&RawValue>) -> Option<&str> {
    let token = stated.map(RawValue::get).filter(|token| *token != "null")?;
    let (whole, _) = token.split_once('.').unwrap_or((token, ""));
    whole
        .chars()
        .all(|character| character.is_ascii_digit())
        .then_some(whole)
        .filter(|whole| !whole.is_empty())
}

/// The sets the watch recorded, where any of them are the operator's.
fn measured_sets(
    landed: &LandedRecord,
    zone: &OperatorZone,
    scribe: &mut Scribe,
) -> Option<NonEmpty<MeasuredSet>> {
    let served: ExerciseSets = match serde_json::from_slice(landed.payload().as_bytes()) {
        Ok(served) => served,
        Err(error) => {
            scribe.note(
                RefusalLocus::Record,
                RefusalReason::UnreadablePayload {
                    detail: format!("its sets: {error}"),
                },
            );
            return None;
        }
    };

    let active: Vec<&ServedSet> = served
        .sets
        .iter()
        .filter(|set| set.set_type.as_deref() == Some(ACTIVE))
        .collect();

    // **One unloaded set is the watch classifying on its own**, and never a
    // session he recorded: see `RefusalReason::OnlyTheWatchClassifying`.
    if let [only] = active.as_slice()
        && weight(only).is_none()
    {
        scribe.note(
            RefusalLocus::Ungrouped { set: 0 },
            RefusalReason::OnlyTheWatchClassifying,
        );
        return None;
    }

    let mut sets = Vec::with_capacity(active.len());
    for (ordinal, set) in active.iter().enumerate() {
        let at = u32::try_from(ordinal).unwrap_or(u32::MAX);
        if let Some(measured) = measured_set(set, zone, RefusalLocus::Ungrouped { set: at }, scribe)
        {
            sets.push(measured);
        }
    }

    carry_along_runs(&mut sets);
    NonEmpty::new(sets.into_iter().map(|working| working.set).collect()).ok()
}

/// A set on its way out of translation, with what only translation knows.
///
/// [`Working::unclassified`] does not reach the entity: whether the source said
/// *nothing* or said something this vocabulary has no member for is already
/// recorded, in the refusal beside the second of them, and a flag on the set
/// would be a second place to read it from.
struct Working {
    set: MeasuredSet,
    /// The source proposed no movement at all for this set, so a run may fill
    /// it.
    unclassified: bool,
}

/// Give each unclassified set the movement its run was performed at.
///
/// The operator's rule, 2026-09-28: *"if an Unknown appears between two named
/// exercises, with the same reps and loads, we can assume it is the same
/// exercise."* Taken as a run rather than as a strict sandwich, which is what
/// the record rewards: flanking alone resolves 11 of his 612 unclassified sets,
/// and a run of identical reps and load resolves 109.
///
/// **A run is contiguous, one rep count, one load.** Three sets of ten at 70 kg
/// back to back are one exercise whatever the classifier managed on each of
/// them; a ramp of 5×48, 5×53, 3×58 is not a run at all, which is why most of
/// the unclassified sets stay that way.
///
/// **A run naming two movements names none.** Three of the operator's do, and
/// picking between them would be this layer inventing an answer the record does
/// not hold (§ 37).
///
/// **Only a set the source left unclassified is filled.** A set whose term this
/// build has no member for was classified — it is our vocabulary that came up
/// short, and it has its own refusal — so overwriting it from a neighbour would
/// replace what the source said with something else.
fn carry_along_runs(sets: &mut [Working]) {
    for run in sets.chunk_by_mut(|earlier, later| {
        earlier.set.reps == later.set.reps && earlier.set.load == later.set.load
    }) {
        let Some(guess) = proposed_by(run) else {
            continue;
        };
        for working in run.iter_mut() {
            if working.unclassified && working.set.guess == GuessedExercise::Undetermined {
                working.set.guess = GuessedExercise::FromItsRun(guess);
            }
        }
    }
}

/// The one movement a run's classified sets agree on, where they agree.
///
/// `None` for a run nobody placed and for a run placed two ways, which are
/// different states and the same answer: there is nothing to carry.
fn proposed_by(run: &[Working]) -> Option<Guess> {
    let mut agreed: Option<Guess> = None;
    for working in run {
        if let GuessedExercise::Proposed(guess) = working.set.guess {
            match agreed {
                Some(held) if held != guess => return None,
                _ => agreed = Some(guess),
            }
        }
    }
    agreed
}

/// One set, or the reason it is not one.
fn measured_set(
    set: &ServedSet,
    zone: &OperatorZone,
    locus: RefusalLocus,
    scribe: &mut Scribe,
) -> Option<Working> {
    let at = match set.started_at.as_deref().map(|stamp| started(stamp, zone)) {
        Some(Ok(at)) => at,
        Some(Err(reason)) => {
            scribe.note(locus, reason);
            return None;
        }
        None => {
            scribe.note(locus, RefusalReason::MissingFigure { figure: "a start" });
            return None;
        }
    };

    let reps = match set.reps.map(RepCount::new) {
        Some(Ok(reps)) => reps,
        Some(Err(error)) => {
            scribe.note(
                locus,
                RefusalReason::UnreadableValue {
                    field: "repetitionCount",
                    detail: error.to_string(),
                },
            );
            return None;
        }
        None => {
            scribe.note(
                locus,
                RefusalReason::MissingFigure {
                    figure: "a rep count",
                },
            );
            return None;
        }
    };

    // The weight is read before the movement, not after: a bare category is
    // placed by what the operator typed against it — a 32.5 kg row is a barbell
    // row and a 15 kg row is not — so the lookup needs the number.
    let grams = weight(set);
    let (guess, reading, unclassified) = guessed(set, grams, locus, scribe);
    let load = grams.map(|grams| load(grams, reading));

    Some(Working {
        set: MeasuredSet {
            at,
            reps,
            load,
            guess,
        },
        unclassified,
    })
}

/// What the watch made of the movement, and how its weight is therefore read.
///
/// The load axis is a property of the exercise (§ 8), so a set the classifier
/// could not place has no axis to take from it. Its weight is read as external
/// load, which is what a number typed against an unidentified movement is: the
/// relative axis belongs to the pull-up family, and the watch names those.
fn guessed(
    set: &ServedSet,
    grams: Option<i64>,
    locus: RefusalLocus,
    scribe: &mut Scribe,
) -> (GuessedExercise, LoadReading, bool) {
    let unclassified = (GuessedExercise::Undetermined, LoadReading::Absolute, true);

    let Some(candidate) = set.exercises.first() else {
        return unclassified;
    };
    let category = candidate.category.as_deref().unwrap_or(UNKNOWN);
    if category == UNKNOWN && candidate.name.is_none() {
        return unclassified;
    }

    let weight = grams.map(|grams| Kg::from_grams(grams.unsigned_abs()));
    let Some(Mapped { guess, load }) = lookup(category, candidate.name.as_deref(), weight) else {
        scribe.note(
            locus,
            RefusalReason::UnguessableMovement {
                term: term(category, candidate.name.as_deref()),
            },
        );
        return (GuessedExercise::Undetermined, LoadReading::Absolute, false);
    };
    (GuessedExercise::Proposed(guess), load, false)
}

/// Garmin's term for a movement, as it serves it.
fn term(category: &str, name: Option<&str>) -> String {
    name.map_or_else(|| category.to_owned(), |name| format!("{category}/{name}"))
}

/// The weight the operator entered, in grams, where he entered one.
///
/// Three ways of saying he did not: no field at all, the `-1` sentinel he
/// identified on 2026-09-18, and a zero. A zero is Garmin's default for a set
/// nothing was typed against — every one of the 124 single-set sessions the
/// watch classified on its own carries an absent weight or a zero, and none
/// carries a number.
fn weight(set: &ServedSet) -> Option<i64> {
    // A negative is Garmin's `-1` sentinel and never a mass, so `whole` reading
    // no digits off it is the right answer rather than a near miss.
    let grams: i64 = whole(set.weight.as_deref())?.parse().ok()?;
    (grams > 0).then_some(grams)
}

const fn load(grams: i64, reading: LoadReading) -> Load {
    match reading {
        LoadReading::Absolute => Load::Absolute(Kg::from_grams(grams.unsigned_abs())),
        LoadReading::Relative => Load::Relative(SignedKg::from_grams(grams)),
    }
}

/// A duration in whole seconds, from the fractional seconds Garmin serves.
fn seconds(stated: Option<&RawValue>) -> Option<Duration> {
    whole(stated)
        .map(ToOwned::to_owned)
        .and_then(|seconds| Duration::try_from(seconds).ok())
}

/// A naive GMT stamp, placed.
///
/// Garmin serves no offset on either the activity's start or a set's, and both
/// are UTC — `startTimeLocal` beside it is the watch's wall clock and is not
/// carried (§ II.3). The zone is the operator's declared one.
fn started(stamp: &str, zone: &OperatorZone) -> Result<StartedAt, RefusalReason> {
    let unreadable = |detail: String| RefusalReason::UnreadableValue {
        field: "start",
        detail,
    };
    let civil: DateTime = stamp.parse().map_err(|error: jiff::Error| {
        unreadable(format!("{stamp:?} is not a date and time: {error}"))
    })?;
    civil
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| StartedAt::new(zoned.timestamp(), zone.clone()))
        .map_err(|error| unreadable(format!("{stamp:?} is not an instant: {error}")))
}
