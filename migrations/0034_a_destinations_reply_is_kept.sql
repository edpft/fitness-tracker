-- What a destination answered, kept as it was answered.
--
-- **We kept every byte a source served and nothing a destination said back**
-- (#124). Extraction lands raw payloads, digest and all; delivery kept four
-- columns and read the response for an id before dropping it. That cost two
-- diagnoses in one day: a create reply that would not parse left only serde's
-- complaint about a column number, and the routine that reply described turned
-- out to be missing an exercise — a question the discarded bytes would have
-- answered on the spot and that only the live API could answer afterwards.
--
-- **Not a landing table**, for the reason `prescription_delivery` is not one.
-- § II governs observation data, and a routine we created is not an observation
-- of anything; what a destination said about it is no more one. This is the
-- delivery record extended by the answer that produced it — § 12 authored data,
-- and stored for the same reason the reference beside it is: nothing
-- regenerates it. A reply cannot be asked for a second time.
--
-- **One row per attempt, not per delivery.** A failed create leaves no
-- `prescription_delivery` row, and that is exactly the case worth keeping — so
-- this hangs off the prescription rather than off the delivery, and a
-- prescription sent, refused and sent again leaves three rows. Append-only for
-- the reason raw is: an answer that is rewritten is an answer nobody can trust.
--
-- **That pins the prescription harder, and the withdrawal that is not built yet
-- will have to say what it wants.** `prescribed_item` already holds a
-- prescription against deletion whatever state it is in; this holds a published
-- one against deletion that no cascade can lift, because the triggers below
-- refuse a delete outright. Recorded here rather than left to be discovered
-- when withdrawal is built: either it keeps the replies of a session it
-- withdrew, or this table needs to survive the prescription it names.
CREATE TABLE delivery_reply (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Which prescription was being delivered, and where to. Together they are
    -- what makes a reply retrievable afterwards: the same pair that keys
    -- `prescription_delivery`, minus its uniqueness.
    prescription INTEGER NOT NULL REFERENCES prescribed_workout(id),
    destination  TEXT    NOT NULL
        CHECK (destination <> '' AND destination = lower(destination)),

    -- The destination's own word for how it took the request — `201 Created`,
    -- `400 Bad Request`. Opaque, like the reference: it is Hevy's vocabulary
    -- and this side does not parse it.
    status       TEXT    NOT NULL CHECK (status <> ''),

    -- Whether the act succeeded, which is the one part of the status that is
    -- not the destination's to define. Stored rather than derived from the text
    -- above, because deriving it would mean this side learning every
    -- destination's vocabulary.
    succeeded    INTEGER NOT NULL CHECK (succeeded IN (0, 1)),

    -- Verbatim, and allowed to be empty: a destination that refuses with a bare
    -- status has still answered, and that it said nothing more is the fact.
    -- Unparseable bytes are the point rather than an edge case, so nothing here
    -- asks whether they are JSON.
    body         BLOB    NOT NULL,

    answered_at  TEXT    NOT NULL
) STRICT;

-- Read by prescription and destination, newest first, which is the one question
-- asked of it: what did that destination last say about this session?
CREATE INDEX delivery_reply_latest
    ON delivery_reply (prescription, destination, id DESC);

CREATE TRIGGER delivery_reply_is_append_only_update
BEFORE UPDATE ON delivery_reply
BEGIN
    SELECT RAISE(ABORT, 'a destination''s reply is kept as it was answered');
END;

CREATE TRIGGER delivery_reply_is_append_only_delete
BEFORE DELETE ON delivery_reply
BEGIN
    SELECT RAISE(ABORT, 'a destination''s reply is kept as it was answered');
END;
