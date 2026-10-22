//! What decides whether a record a source serves again is a new serving.
//!
//! **A JSON record is compared on what it says, not on its bytes** (#532).
//! Neither Peloton nor Garmin states its keys in a stable order. Garmin's
//! activity list landed its whole first page again minutes after the first
//! run, every value identical and only the key order changed. Peloton's
//! account landed 40 times in a month, and 35 of the 39 repeats were key order
//! alone. Comparing the bytes as served makes "0 records landed" stop meaning
//! "nothing changed", and grows a table without adding a fact.
//!
//! So a JSON record's revision is its content re-serialised in one key order.
//! **Canonical because `serde_json::Map` is a `BTreeMap`**: this build does not
//! enable `preserve_order`, so keys come out in one order whatever order they
//! arrived in. Arrays keep their order, which is part of what they say.
//!
//! **Every JSON source, not just the two caught doing it.** A source that has
//! served stable bytes so far makes no promise to go on doing so, and the cost
//! of treating it as though it might not is nothing.
//!
//! The bytes still land exactly as served (§ II.1). This changes what
//! "changed" means, not what is stored.

use application::SourceEvent;
use domain::landing::{PayloadDigest, Provenance, RawPayload, SourceRecordId};

/// An event whose payload is JSON: a new serving only when what it says has
/// changed.
pub(crate) fn json_event(
    source_record_id: SourceRecordId,
    provenance: Provenance,
    payload: RawPayload,
) -> SourceEvent {
    SourceEvent {
        source_record_id,
        provenance,
        revision: of_json(&payload),
        payload,
    }
}

/// What a JSON payload is compared against: what it says, in one key order.
///
/// Falls back to the payload's own digest when it will not parse or will not
/// re-serialise, which is the safe direction: an unreadable payload compares on
/// everything, so it lands again rather than being wrongly judged unchanged.
pub(crate) fn of_json(payload: &RawPayload) -> PayloadDigest {
    serde_json::from_slice::<serde_json::Value>(payload.as_bytes())
        .ok()
        .and_then(|value| of_value(&value))
        .unwrap_or_else(|| payload.digest())
}

/// A parsed JSON value's revision: re-serialised in one key order and hashed.
///
/// `None` if it will not re-serialise, for the caller to fall back on the bytes.
pub(crate) fn of_value(value: &serde_json::Value) -> Option<PayloadDigest> {
    let bytes = serde_json::to_vec(value).ok()?;
    RawPayload::try_from(bytes.as_slice())
        .ok()
        .map(|stable| stable.digest())
}

#[cfg(test)]
mod tests {
    use super::{RawPayload, of_json};

    fn payload(bytes: &[u8]) -> RawPayload {
        RawPayload::try_from(bytes).expect("a payload")
    }

    /// **The live failure this exists for**, as Garmin served it: one activity
    /// twice, every value identical and only the key order changed.
    #[test]
    fn two_key_orders_of_one_record_are_one_revision() {
        let served = payload(br#"{"activityId":123,"duration":42.0,"activityName":"Strength"}"#);
        let served_again =
            payload(br#"{"activityName":"Strength","activityId":123,"duration":42.0}"#);

        assert_ne!(
            served.digest(),
            served_again.digest(),
            "the bytes differ, which is what made this necessary"
        );
        assert_eq!(of_json(&served), of_json(&served_again));
    }

    /// Nested objects are put in order too: Peloton's account nests the bike's
    /// pairings a level down.
    #[test]
    fn nested_key_orders_are_one_revision() {
        let served = payload(br#"{"id":"u","paired_devices":[{"name":"HRM","serial_number":"1"}]}"#);
        let again = payload(br#"{"paired_devices":[{"serial_number":"1","name":"HRM"}],"id":"u"}"#);
        assert_eq!(of_json(&served), of_json(&again));
    }

    /// A value that really changed is still a new serving, and so is a list
    /// in a different order: a list's order is part of what it says.
    #[test]
    fn a_changed_value_or_order_of_a_list_is_a_new_revision() {
        let before = payload(br#"{"activityId":123,"duration":42.0}"#);
        let after = payload(br#"{"activityId":123,"duration":43.0}"#);
        assert_ne!(of_json(&before), of_json(&after));

        let listed = payload(br#"{"paired_devices":["a","b"]}"#);
        let relisted = payload(br#"{"paired_devices":["b","a"]}"#);
        assert_ne!(of_json(&listed), of_json(&relisted));
    }

    /// A payload that will not parse compares on everything, so it lands again
    /// rather than being wrongly judged unchanged.
    #[test]
    fn an_unreadable_payload_falls_back_to_its_own_digest() {
        let unreadable = payload(b"not json at all");
        assert_eq!(of_json(&unreadable), unreadable.digest());
    }
}
