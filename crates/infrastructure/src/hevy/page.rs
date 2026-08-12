//! Reading a page of workout events without interpreting what is in it.
//!
//! The envelope is parsed; the event bodies are not. Each event is held as a
//! `RawValue`, which borrows the exact bytes the source sent, so what gets
//! landed is what arrived — not a re-serialisation of something we parsed,
//! which would reorder keys, renumber floats, and silently drop any field we
//! did not know to keep.

use application::{
    EventPage, SourceError, SourceEvent,
    paging::{PageCount, PageNumber},
};
use domain::landing::{EventKind, EventTime, RawPayload, SourceRecordId};
use serde::Deserialize;
use serde_json::value::RawValue;

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
struct Provenance<'a> {
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
/// # Errors
///
/// [`SourceError::Malformed`] if the envelope will not parse, or if an event
/// carries no identifier — a landing record that cannot say what it is about
/// is worse than a visible failure.
pub fn parse_page(body: &[u8]) -> Result<EventPage, SourceError> {
    let envelope: Envelope<'_> =
        serde_json::from_slice(body).map_err(|error| malformed(error.to_string()))?;

    // Either key means the same thing, and neither means an empty page.
    // Nothing here treats the empty shape as an error.
    let raw_events = envelope.events.or(envelope.workouts).unwrap_or_default();

    let mut events = Vec::with_capacity(raw_events.len());
    for raw in raw_events {
        events.push(parse_event(raw)?);
    }

    Ok(EventPage {
        page: page_number(envelope.page),
        page_count: PageCount::new(envelope.page_count),
        events,
    })
}

/// Pages are one-based at the source. A zero would be the source contradicting
/// its own contract; treating it as the first page is harmless, since the
/// number is only ever echoed back for reporting.
fn page_number(page: u32) -> PageNumber {
    (1..page).fold(PageNumber::first(), |number, _| number.next())
}

fn parse_event(raw: &RawValue) -> Result<SourceEvent, SourceError> {
    let provenance: Provenance<'_> =
        serde_json::from_str(raw.get()).map_err(|error| malformed(error.to_string()))?;

    let kind = provenance
        .kind
        .ok_or_else(|| malformed("an event carried no `type`"))?;
    let kind = EventKind::from_source(kind).map_err(|error| malformed(error.to_string()))?;

    // An update names its workout inside the body; a deletion names it at the
    // top level. A kind we do not recognise could do either, so both are tried
    // rather than assuming which.
    let id = provenance
        .workout
        .as_ref()
        .and_then(|workout| workout.id)
        .or(provenance.id)
        .ok_or_else(|| malformed("an event carried no identifier"))?;
    let source_record_id = SourceRecordId::new(id).map_err(|error| malformed(error.to_string()))?;

    // `updated_at` for an update, `deleted_at` for a deletion. Absent is a
    // legitimate answer and is recorded as absent: substituting the fetch time
    // would invent a fact, and would risk a resumption point stepping over
    // events this run never saw.
    let event_time = provenance
        .workout
        .as_ref()
        .and_then(|workout| workout.updated_at)
        .or(provenance.deleted_at)
        .map(EventTime::parse)
        .transpose()
        .map_err(|error| malformed(error.to_string()))?;

    let payload = RawPayload::new(raw.get().as_bytes().to_vec())
        .map_err(|error| malformed(error.to_string()))?;

    Ok(SourceEvent {
        kind,
        source_record_id,
        event_time,
        payload,
    })
}
