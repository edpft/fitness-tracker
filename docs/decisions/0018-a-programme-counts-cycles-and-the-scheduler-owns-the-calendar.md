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
- **`place(date)`** was used to interpret *history*: `progress` asked the
  calendar what a past performance's date was, and dropped a performance the
  calendar refused. A heavy session prescribed for Friday and performed on
  Saturday morning was dropped from the ladder. The operator's rule is that the
  performance is the only real fact, and this broke it.

  **Amended 2026-08-28: a performance now carries the session it fulfilled**,
  resolved through the published id, and the gate reads its role off that where
  there is one. It did not need the ordinal programme or the allocator, and
  leaving it until they land meant a whole autumn block gated on the day of the
  week a session happened to be trained on.

  **The calendar stays as the fallback, and that is not a compromise.** The
  blocks trained before a prescription could be delivered were still trained,
  and a performance that cannot say which session it was is not a record of
  nothing — dropping those moved the summer block from week four of its ladder
  to week one and prescribed its last six sessions 7.5kg light. So the
  prescription answers where it can and the calendar answers otherwise. The
  fallback keeps the defect for the sessions it applies to: an *unlinked*
  performance a day late is still lost, and there is no fact in the record that
  could recover it.

  What this decision deletes, then, is the fallback — by deleting the calendar.
  `place` also survives on the prescribing path, where the question really is
  what a date is for.

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
any pins and the alternation rule, which slots are the gym's is *derived*. The
operator states his week, what occupies it, and the one slot that is spoken for;
he does not state that Monday is a gym day.

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

### A microcycle exists at two levels, and only one of them is stated

The **training microcycle** is the operator's repeating unit: four slots and one
commitment. It is the container, and the only one that knows what a week looks
like.

A **discipline's microcycle is however many slots alternation gives it** — not a
number anybody states. One open slot is a gym session; two is one of each; three
is two gym and one cycling. So the gym microcycle is two sessions today because
the week yields two gym slots, and a week with more slots makes a denser block
rather than a longer one.

**Nothing states a demand**, which is the correction that matters: an earlier
draft had each discipline declaring how many sessions it wanted, and cycling
needing to state two despite having no programme to state them in. Alternation
removes the question. A programme says what a session *is*; how many are in a
microcycle is a fact about the week.

### What a microcycle holds when it holds more than two

Stated on 2026-08-25, and the honest version is that only half of it is settled.

**Three gym sessions is two light and one heavy** — "until we think of something
better", which is the operator's own gloss and is recorded rather than smoothed
over.

**Linear periodisation has a scheme for it.** He follows top-set-and-back-off
sessions after <https://youtu.be/MujqxSdHH60>, so a third session is `1 × 5` at
80% followed by `2 × 5` at 75%. Those numbers are stated, not fitted.

**Block periodisation does not.** Every source consulted programmes one squat
session a week and suggests a second should use an *alternative* movement — a
split squat rather than another squat. So a three-session block microcycle has
no grounding, and a two-session one is already past what the sources cover,
since the light session runs the primary at a share of the heavy load.

**That recommendation is already expressible**, which is worth knowing before
anybody builds for it. `check_primary` is applied on the *gating* role alone, in
both `Linear` and `Block`, so the primary slot may alternate:

    [fills.knee_dominant]
    light = "bulgarian-split-squat-barbell"   # the alternative movement
    heavy = "front-squat"                     # the primary the ladder climbs

Nothing needs building for the second session to be a different movement. What
is missing is a scheme for a *third*, and only for a block.

### Allocation is a pin, alternation, and the spacing rule

There is no priority order between disciplines, and gym-first is not a rule.
What decides the week is:

1. a **pin** — a slot nailed to a discipline, because Sunday morning is a ride
   for reasons the tool does not model and should not overrule;
2. **alternation** through the remaining slots, its phase fixed by the pin;
3. the **spacing rule**, which places the heavy session within what the gym got.

Run against the operator's week that reproduces it exactly, deriving every part
of what he states by hand today:

    pinned    sunday morning -> cycling

    monday    evening   gym        <- alternation
    wednesday evening   cycling
    friday    evening   gym
    sunday    morning   cycling    <- the pin

    gym microcycle = 2 sessions

    heavy: monday   day before is sunday    (ride + padel) -> no
           friday   day before is thursday  (clear)        -> yes

The Friday falls out. So does the Monday/Friday split, and so does the pairing
with cycling on Wednesday and Sunday.

**A pin is a stated fact and refuses like any other.** If pinning a slot makes
the rest unsatisfiable, the scheduler says so rather than quietly unpinning it.

**And a pin is overridable, like every other scheduling fact.** An alteration
already restates the slots for a run of days; it restates the pins the same way.
A week where the Sunday ride is not happening is a week whose pin lifts, and
that needs no new mechanism — only that alterations carry pins as well as slots.

### The training microcycle is seven days

Stated, and deliberately not generalised: non-weekly microcycles are out of
scope. The container repeats on a week, which is what makes "four slots and one
commitment" a complete description of it.

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

A second round on 2026-08-25 answered three more: a discipline's microcycle
follows the slots, a slot may be pinned, and there is no priority order because
the pin and the spacing rule between them decide the phase.

A third round on 2026-08-25 answered the rest: three gym sessions is two light
and one heavy, a pin is overridable through an alteration like anything else,
and the training microcycle is seven days.

What remains open:

- **A block's scheme for a third session.** Linear has one; block does not, and
  the sources do not offer one because they programme a single squat session a
  week. Two light and one heavy says how many of each, not what the second light
  session *does*. This is the question that arrives with the first extra slot,
  and the answer may well be that the extra session takes an alternative
  movement rather than a third dose of the primary — which the fills already
  allow.
- **Whether "two light and one heavy" survives contact.** It is explicitly a
  placeholder. Recording it as one means nobody later mistakes it for a
  considered position.
