-- The weeks a block does not run.
--
-- `0004` gave a programme a start date and a duration, and placement counted
-- calendar weeks between the two. That is wrong whenever life interrupts the
-- block: a week away silently consumed a ladder position, so the session after a
-- holiday was prescribed a rung the operator had never climbed. The programme's
-- duration counts *training* weeks, and the calendar it occupies is one week
-- longer for every row here.
--
-- Authored (§ 12) rather than consulted from the family calendar at generation
-- time (§ 14): what the block skipped is part of what was planned, and a
-- prescription has to be reproducible from the authored record long after the
-- holiday is off anyone's calendar.
--
-- A new migration rather than an edit to `0004`, which has now been applied.
CREATE TABLE programme_interruption (
    programme INTEGER NOT NULL REFERENCES programme(id),

    -- A date inside the week, as the operator named it — "we are away the week
    -- of the 31st". Which week that is depends on the weekday the block started
    -- on, so it is resolved against the programme rather than stored resolved.
    week      TEXT    NOT NULL,

    PRIMARY KEY (programme, week)
) WITHOUT ROWID;
