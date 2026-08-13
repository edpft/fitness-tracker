//! Reading a page of workout events without interpreting what is in it.
//!
//! The envelope is parsed; the event bodies are not. Each event is held as a
//! `RawValue`, which borrows the exact bytes the source sent, so what gets
//! landed is what arrived — not a re-serialisation of something we parsed,
//! which would reorder keys, renumber floats, and silently drop any field we
//! did not know to keep.

use application::{EventBatch, SourceError, SourceEvent};
use domain::landing::{
    Endpoint, EventKind, EventProvenance, EventTime, RawPayload, SourceRecordId,
};
use serde::Deserialize;
use serde_json::value::RawValue;

use super::paging::{PageCount, PageNumber};

/// The page envelope.
///
/// `events` is optional, and `workouts` exists, because the source uses a
/// **different key** when the page is empty:
///
/// ```jsonc
/// {"page":1,"page_count":17,"events":[ … ]}   // populated
/// {"page":1,"page_count":1,"workouts":[]}     // empty
/// ```
///
/// Its published schema marks `events` required and never mentions the second
/// shape. This is not a curiosity: the empty response is the steady state, and
/// every run after extraction has caught up receives exactly it. A
/// deserialiser written from the schema passes a first run and fails every one
/// thereafter.
#[derive(Debug, Deserialize)]
struct Envelope<'a> {
    page: u32,
    page_count: u32,
    #[serde(borrow, default)]
    events: Option<Vec<&'a RawValue>>,
    #[serde(borrow, default)]
    workouts: Option<Vec<&'a RawValue>>,
}

/// The provenance fields, read out of an event without touching the rest.
///
/// Everything is optional so that a shape we did not expect produces our own
/// error rather than serde's, and so that reading provenance never depends on
/// having recognised the event kind.
#[derive(Debug, Deserialize)]
struct EventFields<'a> {
    #[serde(rename = "type")]
    kind: Option<&'a str>,
    id: Option<&'a str>,
    deleted_at: Option<&'a str>,
    workout: Option<Workout<'a>>,
}

#[derive(Debug, Deserialize)]
struct Workout<'a> {
    id: Option<&'a str>,
    updated_at: Option<&'a str>,
}

fn malformed(detail: impl Into<String>) -> SourceError {
    SourceError::Malformed {
        detail: detail.into(),
    }
}

/// Split a page into one event per record, bytes intact.
///
/// The batch's `resume` is the next page, and is `None` on the last one:
/// asking beyond the last page is an error at the source rather than an empty
/// page, so a walk has to stop exactly here.
///
/// # Errors
///
/// [`SourceError::Malformed`] if the envelope will not parse, or if an event
/// carries no identifier — a landing record that cannot say what it is about
/// is worse than a visible failure.
pub fn parse_page(body: &[u8], endpoint: &Endpoint) -> Result<EventBatch<PageNumber>, SourceError> {
    let envelope: Envelope<'_> =
        serde_json::from_slice(body).map_err(|error| malformed(error.to_string()))?;

    // Either key means the same thing, and neither means an empty page.
    // Nothing here treats the empty shape as an error.
    let raw_events = envelope.events.or(envelope.workouts).unwrap_or_default();

    let mut events = Vec::with_capacity(raw_events.len());
    for raw in raw_events {
        events.push(parse_event(raw, endpoint)?);
    }

    let next = PageNumber::from(envelope.page).next();
    let resume = PageCount::from(envelope.page_count)
        .contains(next)
        .then_some(next);

    Ok(EventBatch { events, resume })
}

fn parse_event(raw: &RawValue, endpoint: &Endpoint) -> Result<SourceEvent, SourceError> {
    let fields: EventFields<'_> =
        serde_json::from_str(raw.get()).map_err(|error| malformed(error.to_string()))?;

    let kind = fields
        .kind
        .ok_or_else(|| malformed("an event carried no `type`"))?;
    let kind = EventKind::try_from(kind).map_err(|error| malformed(error.to_string()))?;

    // An update names its workout inside the body; a deletion names it at the
    // top level. A kind we do not recognise could do either, so both are tried
    // rather than assuming which.
    let id = fields
        .workout
        .as_ref()
        .and_then(|workout| workout.id)
        .or(fields.id)
        .ok_or_else(|| malformed("an event carried no identifier"))?;
    let source_record_id =
        SourceRecordId::try_from(id).map_err(|error| malformed(error.to_string()))?;

    // `updated_at` for an update, `deleted_at` for a deletion. Absent is a
    // legitimate answer and is recorded as absent: substituting the fetch time
    // would invent a fact, and would risk a resumption point stepping over
    // events this run never saw.
    let occurred_at = fields
        .workout
        .as_ref()
        .and_then(|workout| workout.updated_at)
        .or(fields.deleted_at)
        .map(EventTime::try_from)
        .transpose()
        .map_err(|error| malformed(error.to_string()))?;

    let payload =
        RawPayload::try_from(raw.get().as_bytes()).map_err(|error| malformed(error.to_string()))?;

    Ok(SourceEvent {
        source_record_id,
        provenance: EventProvenance::new(endpoint.clone(), kind, occurred_at).into(),
        payload,
    })
}
