# 0018 — A programme counts cycles, and the scheduler owns the calendar

**Date**: 2026-08-25
**Status**: Proposed. The three questions it was drafted with were answered on
2026-08-25 and folded in; what is left under *Open* is new.

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

**And it allocates rather than being told.** Given the slots, the commitments,
each discipline's demand and the alternation rule, which slots are the gym's is
*derived*. The operator states his week and what occupies it; he does not state
that Monday is a gym day.

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

### A microcycle exists at two levels

The operator's, stated on 2026-08-25:

- the **training microcycle** is his repeating unit — **four slots and one
  commitment**;
- the **gym microcycle** is **two sessions**, one heavy and one light;
- the **cycling microcycle** is **two sessions**.

Two and two fill the four. A discipline's microcycle is a fixed count of
sessions, which is what makes it independent of the calendar; the training
microcycle is the container the scheduler fills, and it is the only one that
knows what a week looks like.

**Every discipline states its demand, whether or not it is programmed.** The
scheduler cannot allocate four slots between gym and cycling without knowing
each wants two. For the gym that number comes from its programme. For cycling,
which has no programme, it is simply stated — two forty-five minute endurance
rides — and that is enough for allocation to work.

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

### No admissible slot is a refusal

Where nothing satisfies the spacing, the scheduler says so and places nothing.
It does not put the session somewhere inadmissible and hope, and it does not
silently drop it — both would be the tool asserting something the operator never
agreed to. A refusal that names the session and the rule it could not satisfy is
the same answer an underivable slot already gives (FR-011), and for the same
reason.

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

**The authored allocation goes.** `TrainingPattern` holds a slot-to-discipline
map, added days earlier at the operator's request so that the schedule would own
the allocation rather than a programme having to know about alterations. Owning
it now means *deriving* it: the map becomes an output, the `discipline` columns
on `training_slot` and `alteration_slot` go, and the wizard stops asking "and
which of those are the gym's?".

That is a further step rather than a reversal — the reasoning that put
allocation here is exactly the reasoning that lets the scheduler compute it —
but it does undo a schema that has only just landed, and saying so plainly is
cheaper than having it discovered.

**A derived allocation moves when the week does, and only for drafted
sessions.** Recomputation cannot disturb a published or performed session,
because those are fixed by § 12.1. So a commitment added mid-block changes what
is prescribed next and nothing that already happened.

**Prescriptions issued under the old model are unaffected**, because they are
recorded as issued (§ 12.1) rather than re-derived.

## Open

The three questions this was drafted with were answered on 2026-08-25 and are
recorded above: the scheduler allocates, a discipline's microcycle is a fixed
count of sessions, and no admissible slot is a refusal.

What remains open:

- **Whether a derived allocation is ever overridable.** Sunday morning may be a
  ride because of who he rides with rather than because the constraints put it
  there. Nothing has asked for an override yet, and adding one before it is
  wanted would be inventing a rule.
- **What happens when the constraints admit more than one allocation.** Four
  slots split two and two leaves little room, but five would. A tie-break has to
  come from somewhere, and the operator's stated order is the obvious candidate.
- **Whether cycling's demand belongs in the schedule or in a cycling
  programme.** Stated in the schedule it is a fact about the week; stated in a
  programme it is intent, and cycling has no programme to hold it.
