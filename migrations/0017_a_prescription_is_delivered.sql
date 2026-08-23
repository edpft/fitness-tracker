-- A prescription, and where it was put so the operator could act on it.
--
-- **Not a landing table, and not a layer.** § II governs observation data, and
-- a routine we created is not an observation of anything — so this acquires no
-- raw, normalised or canonical form. It is § 12 authored data: a record of what
-- we intended, extended by where that intention was sent. Nothing regenerates
-- it if lost, which is why it is stored at all: the reference below is minted by
-- a system we do not own, and a rebuild cannot ask for it again.
--
-- **One row per prescription per destination.** An issued prescription is
-- written once and never rewritten (§ 12), so a session asked about twice is the
-- same session and a session that should be replaced is a *different*
-- prescription with a row of its own. The primary key is what makes that
-- structural rather than a convention the caller has to keep. It is deliberately
-- not unique on the reference: what a destination does with its own identifiers
-- is its business, and a constraint here would be us asserting a rule the
-- destination never agreed to.
--
-- The destination is a column rather than a table per destination, which is the
-- opposite of the landing side and not an inconsistency: a landing table is
-- *shaped* by its stream, and this is the same four columns whatever received
-- the session.
CREATE TABLE prescription_delivery (
    prescription INTEGER NOT NULL REFERENCES prescribed_workout(id),

    -- Ours, not the destination's. Lowercase and without whitespace, the rules
    -- every name we assign answers to.
    destination  TEXT    NOT NULL
        CHECK (destination <> '' AND destination = lower(destination)),

    -- What the destination called the session. Opaque: never parsed, never
    -- compared to anything but another of its own kind, and constrained only
    -- against being empty — the value belongs to the system that issued it.
    reference    TEXT    NOT NULL CHECK (reference <> ''),

    delivered_at TEXT    NOT NULL,

    PRIMARY KEY (prescription, destination)
) STRICT, WITHOUT ROWID;
