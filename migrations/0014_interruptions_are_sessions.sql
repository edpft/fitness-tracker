-- An interruption is a run of days, not a week.
--
-- `programme_interruption` held one date per skipped week, which cannot say
-- "away Friday but back for Monday". The operator's September is four
-- individual sessions across three weeks — Monday 31 August, Friday 4
-- September, Friday 11 September, Monday 14 September — and under the week
-- model naming the week of 7 September to lose its Friday also lost its usable
-- Monday, while the week of 14 September could not be named at all because its
-- Friday is the block's test.
--
-- **A start and a day count, not a start and an end.** `days` is at least one,
-- so an empty skip is unrepresentable, and there is no `end < start` to reject
-- because there is no end column to disagree with the start. That is the
-- operator's own design, settled 2026-08-21.
--
-- **A week is interrupted only as a consequence.** It is now a training week if
-- at least one of its sessions survives, so the old whole-week behaviour is the
-- case where nothing does. That rule lives in `Calendar`, not here: which
-- weekdays a block runs is another table, and a CHECK cannot see it.
--
-- **Existing rows become Monday-to-Sunday ranges**, which is a restatement
-- rather than a guess: a week named under the old model did not run at all, so
-- every day in it was skipped. The stored date named the week rather than
-- starting it, so the range starts at that date's Monday. `days = 7` covers the
-- week whichever day was named.
--
-- Only `programme_interruption` changes and nothing references it, so this is
-- the one table. It has no children to rename aside.

ALTER TABLE programme_interruption RENAME TO programme_interruption_old;

CREATE TABLE programme_interruption (
    programme  INTEGER NOT NULL REFERENCES programme(id),

    -- The first day the block does not run.
    start_date TEXT    NOT NULL,
    -- How many consecutive days it covers, itself included. At least one: a
    -- skip of no days would author successfully and skip nothing.
    days       INTEGER NOT NULL CHECK (days >= 1 AND days <= 255),

    PRIMARY KEY (programme, start_date)
) STRICT, WITHOUT ROWID;

INSERT INTO programme_interruption (programme, start_date, days)
SELECT
    programme,
    -- SQLite's `weekday 1` jumps forward to the next Monday, so stepping back a
    -- day first lands on this week's Monday and leaves an existing Monday alone.
    date(week, '-1 day', 'weekday 1'),
    7
FROM programme_interruption_old;

DROP TABLE programme_interruption_old;
