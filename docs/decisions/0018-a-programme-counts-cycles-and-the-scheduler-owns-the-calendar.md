# 0018 — A programme counts cycles, and the scheduler owns the calendar

**Date**: 2026-08-25
**Status**: Proposed

**Amends** `0014-block-periodisation-keeps-its-endpoint.md`, which becomes a
scheduling policy rather than a fact about blocks — and then stops being needed,
because a programme with no endpoint cannot keep one.

## Context

The programme knows about dates, and it should not.

```rust
pub struct Calendar {
    start: Date,
    duration_weeks: u32,
    interruptions: Interruptions,   // runs of dates
    weekdays: Weekdays,             // monday → light, friday → heavy
    zone: TimeZone,
}
```

Every field there is a scheduling fact wearing a programming hat, and each has
cost something:

- **`interruptions`** produced a fortnight of argument about whether a
  programme should store the days it loses, why re-authoring is needed to adopt
  a schedule change, and whether a stored skip is stated or derived. None of
  those questions exists for a programme that holds no dates.
- **`weekdays`** encodes the recovery rule implicitly. The operator's heavy
  session is on a Friday for a reason, and the document records the Friday
  rather than the reason — so a week that changes leaves the Friday silently
  wrong.
- **`place(date)`** is used to interpret *history*: `progress` asks the calendar
  what a past performance's date was, and drops a performance the calendar
  refuses. A heavy session prescribed for Friday and performed on Saturday
  morning is dropped from the ladder. The operator's rule is that the
  performance is the only real fact, and this breaks it.

## Decision

**A programme counts sessions, microcycles, mesocycles and macrocycles. It
never learns a date.** The scheduler maps that ordinal structure onto the
calendar.

| was | becomes |
|---|---|
| `duration_weeks` | a count of microcycles |
| accumulation / intensification / realisation weeks | mesocycles |
| `weekdays: {mon: light, fri: heavy}` | a microcycle's shape: one heavy session, one light |
| `WeekIndex` | a microcycle index |
| `interruptions` | nothing — not a programme fact |
| `start`, `zone` | nothing — the scheduler places it |

### Three boundaries

**The programme never learns a date.** It emits ordered sessions.

**The scheduler never learns an exercise.** It places sessions into slots for a
discipline, and does not care whether a session is squats or an endurance ride.
That is what lets cycling — and anything after it — use the same scheduler.

**The record never learns intent.** Extraction and normalisation are unchanged:
what happened, from a source, whatever was planned. The only edge between the
two halves is the session↔performance link, which is `routine_id` and already
exists.

### A commitment is time spent that we do not allocate

The schedule module already says a padel game "constrains the week without
joining the pool", and then records nothing about it. The distinction is right
and the conclusion was wrong: not being allocatable is not a reason to be
invisible.

- A **training slot** is interchangeable time, allocated to a discipline we
  programme.
- A **commitment** is time that is spent and that we never allocate — padel on a
  Sunday evening, a match, an appointment. The operator may move one or add
  more; the tool may not.

### Spacing places the sessions, and there is no model of load

The rules, as the operator states them:

- gym and cycling alternate;
- **a heavy session needs a clear day before it** — clear meaning nothing else
  is on that day, whether training or commitment;
- nothing constrains the light session or a cycling session.

Run against the operator's week that derives the Friday rather than being told
it. Sunday carries a ride *and* padel, so Monday has nothing clear in front of
it; Wednesday is a ride, Thursday is clear, Friday qualifies. There is one heavy
slot in the week and the constraints find it.

**Occupancy, not cost.** A day is clear or it is not. There is deliberately no
model of training load, intensity or fatigue: the operator's rule is that two
things happen on a Sunday and so the hardest gym session does not go on a
Monday, and that is answerable without any of it. Modelling training cost is
later work, and this decision must not require it. Two forty-five minute
endurance rides are not distinguished from each other here, and making the
Sunday ride longer is cycling programming rather than scheduling.

### Spacing answers drop-versus-queue

A session goes in the next slot that satisfies its spacing. A lost week does not
delete a session and does not pile sessions up: they take the next admissible
slots and the block's wall-clock end moves.

Dropping was only ever necessary because dates were baked into the programme.
`0014` said a block keeps its endpoint; a block that counts microcycles has no
endpoint to keep, and when it finishes is a question for the scheduler and the
week.

## Consequences

**`gating_role` and `[programme.weekdays]` leave the document.** The programme
says a microcycle is one heavy session and one light one; where they land is
derived. The operator stops asserting Friday and is told it, and a week that
changes changes the answer instead of silently invalidating it.

**The interruption machinery goes.** `Interruptions`, `Skip`, the derivation at
authoring, the override, and the column that would have recorded whether a skip
was stated or derived. So does the freeze, and the re-authoring needed to adopt
a schedule change.

**`place(performance.on)` goes.** What a performance was is decided by the
session it fulfilled, which is the link, not the date.

**The macro layer arrives, minimally.** The roadmap puts Peloton, nutrition and
the family calendar out of scope on the grounds that allocating between
disciplines is planning. A commitment is not planning: it is a fact about time
already spent, and it is the minimum the gym scheduler needs to be correct. What
stays out is the tool *deciding* anything about padel.

**Prescriptions issued under the old model are unaffected**, because they are
recorded as issued (§ 12.1) rather than re-derived.

## Open

- Whether the scheduler eventually **derives the allocation** between gym and
  cycling as well as placing sessions within it. It has what it needs — slots,
  commitments, the alternation rule — but the operator states the allocation
  today and nothing yet asks for that to change.
- Whether a microcycle is **a fixed number of sessions** or "however many the
  week holds". The first makes it independent of the calendar, which is the
  point of this decision; the second is closer to how it is said out loud.
- What the scheduler does when **no slot satisfies the spacing**. Refusing to
  place a session says something true; placing it anyway and saying so may be
  more use.
