# Phase 0: Research — Prescribed workout generation

Nine entries. Each records what was chosen, why, and what was rejected.

The spec's one question — plate quantisation — was resolved before planning and
is not re-litigated here; D5 records only what generalised during design. D8 is
not a decision but a gap, recorded so it cannot be mistaken for one. D9 was raised
by the operator during planning and changed the type factoring, so it is last
rather than in dependency order.

Facts quoted below were taken from the store on 2026-08-17: 165 landing records,
164 normalised workouts, 1 retraction, 3 refusals, 1,122 exercise entries, 3,755
sets, 165 distinct source record ids.

---

## D1: A failed attempt is a set outcome

**Decision**: `Set<M>` stops holding `measure: M` and holds
`outcome: Performed<M>`, where `Performed<M>` is `Completed(M) | Failed`.
`RepCount` keeps its `NonZeroU32`.

**Rationale**: The performed model already established that zero is not a rep
count — `RepCount` is a `NonZeroU32` and `InvalidQuantity::ZeroReps` exists
because "a set of zero reps is not a set". That reasoning was right and this
decision does not weaken it. What changes is where the case goes: 002 sent it to
a refusal, and a refusal is for what the domain cannot express. The domain can
express a failed attempt; it simply had no variant for one, because the prescribed
side did not exist to give it meaning. 002's own comment on
`RefusalReason::ZeroReps` says so — the case "needs an *attempt*, which belongs
with prescribed-versus-performed".

Putting the outcome on the set rather than beside it is what makes FR-029
structural. A volume sum matches on `Completed(reps)` and there is no arm in which
a failure contributes a number, so a failure cannot reach a total by arithmetic.
An `Option<RepCount>` would have compiled and would have relied on every caller
remembering which `None` meant what.

**Alternatives considered**:

- **`SetKind::Failed`.** Rejected: `SetKind` answers one question — should volume
  metrics count this? — and a failed attempt is a working set that happens to have
  failed. The measure would still need a value, so this solves nothing and
  overloads a type that currently has exactly two states for a reason.
- **A sentinel `RepCount`, zero or otherwise.** Rejected outright. It reintroduces
  the representable-zero § 24 spent 002 removing, and the sentinel then has to be
  filtered everywhere by discipline.
- **`Option<M>` on the set.** Rejected: the corpus already has sets with no
  recorded intensity and no recorded rest, where absence means "nothing recorded
  it". A second `None` meaning "attempted and failed" makes the two
  indistinguishable, which is precisely FR-030.
- **Leave it a refusal, and read the refusal table when gating.** Rejected: it
  makes the progression rule depend on a diagnostic table, and a refusal is
  explicitly a thing to *act on and fix*. The gate would be reading the repair
  queue.

**Evidence this is the right discriminator**: across the corpus, Hevy's `failure`
set *type* appears 77 times and exactly one of those carries zero reps. The type
means "taken to failure" and the existing translator already reads it correctly as
zero in reserve. Keying the new outcome on the type would misfile 76 completed
sets; keying it on `reps == 0` fires once, on the 95kg front squat of 2026-07-03
that is the genuine failed attempt.

---

## D2: The anchor is fixed and authored; the ladder position is derived

**Decision**: The anchor is the block's starting 1RM, **authored and constant for
the block's duration**. What is derived is the block's *plan* — a percentage ladder
over the climbing weeks — and the *position on it*, which the performed record can
suspend and resume. Nothing stores a mutable current anchor, a stall count or a
reset stage.

**This decision was revised during planning.** The first version had the anchor
itself climbing +2.5kg per week and derived from the performed record. That
describes the same load sequence from the wrong end: a value that climbs
indefinitely gives the block no endpoint, so there is nothing for a duration to be
the duration *of*. The operator's framing settled it — the generator takes a number
of weeks and a starting 1RM and produces a plan, which requires the starting value
to hold still.

**The plan, concretely**. Given `W` weeks and anchor `A`:

```text
climbing weeks  = W - 1              the last week is the test
step            = (end - start) / (climbing weeks - 1)
heavy(w)        = quantise(A × (start + step × (w - 1)))
light(w)        = quantise(heavy(w) × light_of_heavy)
warmups(w)      = percentages of that session's own top set
back_off(w)     = 85% of that session's own top set
```

**The endpoint is authored and the step derived**, not the reverse. An endpoint is a
claim about achievable gain that history and a reference programme can both inform;
a weekly step is a number with nothing behind it, and multiplying it by a duration
yields an endpoint nobody chose. This also makes duration meaningful: the same
endpoint over 8 or 12 weeks is two different plans, where a fixed step over two
durations is the same plan run for different lengths.

**What survives from the first version.** The three properties that made a derived
anchor attractive still hold, because the ladder position is derived even though the
anchor is not:

- **FR-010** — asking twice cannot double-advance, because there is no counter. The
  week's load is a function of the date and the record, computed fresh.
- **§ 7** — re-derivation end to end: raw → normalised → history → ladder position →
  prescription, with the authored programme and parameters as the other inputs.
- **SC-008** — regenerate identically, trivially, with no state to have drifted.

**What it costs**: a cycle calendar, unchanged from the first version. Locating a
date on the ladder needs the programme's start date and weekday-to-role mapping, and
calendar arithmetic resolves through the operator's IANA zone rather than by adding
multiples of 24 hours (§ II.3). `jiff` already carries what it needs.

**Alternatives considered**:

- **A climbing anchor** — the first version. Rejected above.
- **Store the ladder position, mutate on advance.** Rejected: needs a rebuild path,
  a double-advance guard, and gives the operator a number with no way to check it
  against the rules. The two arithmetic errors this feature exists to prevent were
  exactly that failure in a human medium.
- **Read the ladder position off the last issued prescription.** Rejected: makes
  generation depend on its own output, so a missed session leaves the chain reading
  a prescription that was never performed.

**Consequences worth naming.**

- **A reset does not touch the anchor** (FR-021). It suspends the ladder, drops from
  the *failed load*, re-climbs at the reset's rate, and resumes the ladder when it
  returns to that load. A stall is evidence the plan was too ambitious, not evidence
  about where the block started.
- **A test replaces the anchor for the *next* block**, and a test ends a block, so
  within a block the anchor is a constant. What marks a session as a test is
  programme information — the programme knows one is scheduled — rather than
  anything inferable from the sets alone.

---

## D3: The history projection resolves supersession

**Decision**: The projection answering "the most recent performance of exercise
X" reads the normalised layer and, where two landing records share a source record
id, takes the later-served one. Cross-source correspondence is not attempted.

**Rationale**: The spec defers the canonical layer on the operator's decision that
one source needs no reconciliation. That holds for *matching* and not for
*supersession*, and the two are different rules in § 10: records from different
sources are co-observations, while two records sharing a source identity are one
source contradicting itself, where the later supersedes.

Deferring matching is safe because there is no second source. Deferring
supersession is safe only until the first re-serve, at which point the projection
sees two versions of one performance and has no basis to prefer between them. The
failure mode is a prescription derived from a performance the source has
withdrawn — silently wrong, not visibly broken.

The corpus does not contain the case: all 165 landing records carry distinct
source ids. So this is defensive, and cheap enough to be worth it — one `WHERE`
clause over `serve_ordinal`, which the landing table already carries for exactly
this purpose. It is tested synthetically, as 001 and 002 both tested supersession.

**What this deliberately does not do**: resolve fragmentation. One training
session spread across four landing records stays four normalised workouts. That is
harmless for "the most recent performance of this exercise" and would not be
harmless for a session count, a frequency or a streak — § 10's counting rule. No
such figure is computed here, and the first one that is needed is the trigger to
build the canonical layer properly.

**Alternatives considered**:

- **Read normalised naively, latest `started_at` wins.** Rejected: that is the
  silent-wrong-answer path above.
- **Build the canonical layer.** Rejected on the operator's scope decision, and it
  is the right deferral: canonicalisation for one source is mostly passthrough
  plus fragmentation reconciliation, and fragmentation needs a session concept the
  performed model deliberately does not have yet.
- **Put the supersession rule in the use case.** Rejected: it is expressible as a
  predicate in the query the adapter already runs, and lifting it into
  `application` means loading every set to filter in memory.

---

## D4: What the projection returns, and the never-performed case

**Decision**: The projection returns, per exercise asked about, either the most
recent performance — its date, its sets, each set's load and outcome — or an
explicit "never performed" answer. Not `Option`.

**Rationale**: FR-011 requires a slot that cannot be derived to say which slot and
why, and FR-007's double progression needs a *previous* performance to progress
from. Three states matter and they are different: performed before, never
performed, and performed but the record is unusable. An `Option<Performance>`
collapses the first two at the call site and is exactly the shape that invites a
`None` to become a default load.

A named case also makes the acceptance scenario writable: US1 scenario 7 asks that
a never-performed exercise be prescribable from the authored programme rather than
failing. That is only a distinct behaviour if the type distinguishes it.

**Query shape**: per-exercise and unbounded — not "the last two sessions". The
alternating lower accessory proves the need. Across the corpus the hip-dominant
slot alternates Nordic hamstring curls with the back-extension machine, so on any
given session the exercise being prescribed was last performed two sessions back,
not one. A bounded lookback would silently re-issue opening numbers.

**Alternatives considered**:

- **Return the last *N* performances.** Rejected: double progression needs one,
  and *N* is a number nobody can justify. The unbounded single answer is both
  simpler and correct.
- **Return every performance of the exercise and let the use case pick.** Rejected:
  moves the ordering rule out of the adapter that can express it in SQL, and hands
  the use case 3,755 sets to sift.

---

## D5: Quantisation is one function

**Decision**: Nearest multiple of the plate increment, ties resolving down. A
`domain` function over a load and an increment, applied wherever a derived load
meets the grid.

The rule itself was settled in the spec. What design added is the scope: it is
**not** a back-off rule. Three derivations produce off-grid loads and all three go
through the same function.

- **Back-offs.** 85% of an 80kg top set is 68 → 67.5.
- **Warm-up ramp steps.** Percentages of the top set, and there are four of them
  per session.
- **Reset drops.** −10% from 87.5 is 78.75, which is an exact tie between 77.5 and
  80, and resolves down to 77.5. This is the case that made the generalisation
  necessary: had quantisation stayed attached to the back-off, the reset drop
  would have needed its own rule and the two could disagree.

**Why the increment is data and the rule is code**: § 9. Deterministic derivation
is code; the increment is a § 14 generation parameter because the gym's plates
could change and because `primary-lift-progression.md` treats 2.5kg as a fact
about the equipment. The rounding direction is not a fact about equipment.

**Alternatives considered**: recorded in the spec. Rejected there were *always up*
(overshoots by up to 3% on a 2.5kg grid, and makes a reset drop land above what the
reset intends) and *always down* (the same rule as the chosen one except on exact
ties, and a weaker statement of it).

---

## D6: Where the authored programme comes from

**Decision**: A TOML document, read once by `fitness programme author <path>`,
converted into `domain` types at the adapter boundary, and stored durably with the
date it was authored. Supersession by authored-at date, never overwrite (§ 12).

**Rationale**: FR-023 and FR-024 require the parameters and the programme to be
stored, so something has to put them there. The programme carries roughly thirty
values — eleven slot fills, a four-step warm-up ramp, per-role top-set reps, the
back-off percentage, the increment, the two reset protocols, the gating role, the
start date and the weekday mapping.

§ 21 is the rule in tension, and it exempts "interface languages confined to their
adapter — SQL at the store, query or template syntaxes at their respective ports".
A document read at one boundary and converted immediately is that exemption's
case. The test it must pass is that no `toml` type reaches `domain`, which the
placement in `infrastructure/programme/document.rs` enforces and the
`architecture` check verifies by ring.

**Alternatives considered**:

- **CLI flags.** Rejected: thirty values is an unusable command line, and an
  unreviewable one. It also loses the thing a document gives for free — the
  operator can read the whole programme at once and see that it is the programme
  they meant.
- **Seed through a migration.** Rejected twice over: authored data is not schema,
  and § 12 requires history that a migration cannot express. It would also make
  changing a percentage a schema change.
- **JSON.** Rejected on the document's only purpose. It is edited by hand, and
  JSON has no comments — so the *why* behind a percentage would have nowhere to
  live except a commit message.
- **Store parameters as rows and edit them with `fitness parameters set`.**
  Rejected as scope: it is a second authoring surface for the same data, and § 32
  asks for one capability right rather than two half-built.

---

## D7: How a date becomes a session role

**Decision**: The programme declares a start date and a weekday-to-role mapping.
A requested date resolves to a cycle index and a session role by walking the
calendar in the operator's zone.

**Rationale**: The prescribed model says session roles within a cycle are an
ordering, not state, and that the light and heavy sessions differ in fill and in
the primary's loading. Something has to say which a given date is, and there are
only two candidates: derive it from the calendar, or read it from what was last
performed.

Reading it from performance is what the spec's deferral of correspondence rules
out — and it would be wrong anyway, since a missed session must not shift the role
of the next one. The calendar is authored, so the answer is deterministic and
available for a date in the future, which is what "issue the next session" needs.

The corpus supports the mapping directly: sessions land on Mondays and Fridays,
and the primary's top set is a triple on Mondays and a single on Fridays across
every session since the July test. That is the mapping, and it is authored rather
than inferred.

**Alternatives considered**:

- **Alternate roles by counting performed sessions.** Rejected: a missed session
  flips every subsequent role, so one absence desynchronises the programme
  permanently.
- **Let the operator name the role.** Rejected as a footgun — the role determines
  the loading, so a mistyped role silently prescribes the wrong session. Deriving
  it and *printing* it is strictly better. The operator names the date, which they
  cannot get wrong without noticing.

**Not decided here**: what happens when a date falls on no programmed weekday.
Treated as an error naming the programmed days rather than as an implicit nearest
match; recorded in [contracts/cli.md](./contracts/cli.md).

---

## D8: One authored value remains unknown — a gap, not a decision

**Substantially narrowed by D2's revision.** The first version of this entry listed
two unknowns and reverse-engineered a candidate anchor from four sessions. With the
anchor fixed and authored, most of that dissolves.

**Stated by the operator**, and so not open at all:

| Value | Value | |
| --- | --- | --- |
| Anchor | **90kg**, tested 2026-07-03 | A completed single with a failed 95 above it — the one measurement of this lift's 1RM in the record. The operator named the session; the record supplied the number |
| Back-off | **85%** of the session's top set | Where the record disagrees — three light sessions before 10 August — that is operator error, confirmed as such |
| Warm-up ramp | **4 at 40%, 3 at 60%, 2 at 80%, 1 at 90%** of the top set | |
| Plate increment | **2.5kg** | Equipment |
| Reset protocols | −10%/+5kg, −5%/+2.5kg | `primary-lift-progression.md` |

**An operator input rather than a parameter:**

| Value | |
| --- | --- |
| Duration | Supplied per block. Taking a duration and a starting 1RM is the whole point of the generator, and the ladder's step is the span divided by it — so a different duration is a different plan rather than the same plan run longer |

**Inferred from the performed record and not confirmed.** These are marked
`INFERRED` in the authored document. They are probably right; none of them was
stated, and the distinction is what stops a fitted value passing as intent:

| Value | Value | How it was arrived at |
| --- | --- | --- |
| Light of heavy | **85%** | Stated by the operator 2026-08-18. Was 88.5%, solved from three weeks of light/heavy pairs that are a flat −10kg apart — a ratio fitted to an offset, drifting across the three where the offset does not. The percentage is still the right shape; the number in it had to be chosen |
| Top-set reps | heavy **1**, light **3** | Read off every session since the July test. Well evidenced; they have not varied within a role |
| Accessory scheme | **4–6 × 3 sets** | Eyeballed across pull-ups, curls and wrist work. Both the range and the decision to use one range for ten slots are unconfirmed |

**Why the distinction is not pedantry.** The back-off held at 67.5kg for three
consecutive sessions while the top set climbed 75 → 77.5. Fitting the back-off the
way these three were fitted would have yielded "the back-off holds while the top
set moves" — a mistake encoded as a rule. It was avoided because the operator
stated the value, not because the method was sound.

**What remains unknown: the ladder's start and end percentages.** One number in
substance, since the start follows from the endpoint and the duration once a rate is
implied.

This is open question 5 in the prescribed model and it is not resolvable by research
into this repository, because it is a claim about what a specific lifter can gain in
a specific number of weeks. What research *can* supply is bounds, and three now
exist:

- **A standard template embeds a rate.** 5/3/1 advances a training max 5kg per
  four-week cycle for a lower-body lift — roughly 1.25kg per week. A classic linear
  block finishes near 102.5–105% of the entry 1RM. Recalled from training
  literature rather than a source in this repository; worth verifying before
  authoring.
- **Personal history bounds it.** The record holds one other defensible estimate: 28
  April 2025, 90×3 at zero in reserve, which Epley puts near 99kg. So a block
  anchored at 90 has roughly 9kg of regain available before it reaches new ground,
  and regain comes back faster than new ground does.
- **The reset must fit.** A stall costs four of the block's 7 or 11 climbing weeks,
  so a ladder leaving no room for one reset cannot survive a stall inside its own
  block. This is a hard constraint on ambition and it comes from the mechanism
  rather than from physiology.

**Why the earlier fitted reading is not adopted.** The first version solved for "the
heavy top set is 100% of a climbing anchor" from two consecutive sessions. It was
arithmetically consistent and it was fitting, not authoring — the same move that put
the back-off error into the record. § 12 makes these authored intent, and intent is
stated rather than inferred. The current model reaches the same loads from a stated
plan instead.

**What this blocks and what it does not.** It blocks SC-001 — a real workout — and
demonstrating SC-001. It does not block Phase 1 design or the
implementation of anything: the parameter store, the ladder derivation, the
projection and the CLI are all indifferent to the value.

**How the gap is held open safely.** The authored document carries `TODO` where the
span belongs and authoring refuses it, so no prescription can be issued from a
guessed span. A placeholder that authors successfully is worse than one that
fails.

---

## D9: A performance projects into a prescription shape, and the shape is a separate type

**Decision**: Split the instructional content of a session from the facts that make
a prescription *issued*.

```rust
/// What to do. Ordered items, groupings, slots, sets.
pub struct WorkoutShape { items: NonEmpty<PrescribedItem> }

/// A shape that was issued, and everything that makes that claim true.
pub struct PrescribedWorkout {
    shape: WorkoutShape,
    issued_for: Date,
    session_role: SessionRole,
    week: WeekKind,
    anchor: Anchor,
    parameters: GenerationParameters,
    programme: ProgrammeId,
    issued_at: Timestamp,
}

/// Total, and `domain`. Not a port — it reads no store and makes no request.
pub fn project(workout: &GymWorkout) -> Projection;

pub struct Projection { pub shape: WorkoutShape, pub gaps: Vec<ProjectionGap> }
```

**Rationale**: A performed workout and a prescribed one have the same structure —
ordered items, groupings, exercises, sets carrying a load and a measure. That is
not a coincidence; it is why the prescribed model says the issued grouping "is
structurally what the performed model calls `WorkoutItem`". So a total function
from performance to prescription shape exists, and writing it down buys two things.

1. **A divergence names itself.** Comparing a projection against a generated
   prescription reports what differed, in the domain's own vocabulary, instead of
   a human reading a printout against a database.
2. **It separates what a prescription *is* from what makes it issued.** That
   factoring was latent and wrong in the first draft of this plan, where
   `PrescribedWorkout` bundled the items with the anchor and the date.

**This is a forward invariant, and the corpus does not satisfy it.** The property
is that a session performed under an issued prescription projects back to
something that prescription is satisfied by. Nothing in the corpus was issued: it
records a programme run by hand, whose template changed while it ran and whose
arithmetic was sometimes wrong. Comparing a projection against a *regenerated*
prescription for a past date is therefore a diagnostic — it says where the model
and the history part company, and each parting has a cause worth naming — and it
is not a test the model should be expected to pass. An earlier draft of this
document had SC-002 and SC-003 asserting reproduction, which would have made
reproducing human error a requirement.

The distinction matters for what the comparison is *for*. Against the corpus it
locates unstated parameters, template changes and hand-arithmetic mistakes.
Against sessions performed after generation starts issuing, it locates defects.

**The § 11 hazard this closes, which is the reason for the split.** § 11 makes the
separation one-directional: prescribed data never satisfies a query about what
happened. A projection runs the *permitted* direction — it reads performance — but
its output is prescription-shaped, and if that output were the same type generation
produces, it could be handed to `PrescribedWorkoutStore::issue`. The store would
then hold a prescription that was never issued, reverse-engineered from the
performance it is supposed to be compared against. Expectation versus reality
becomes unrecoverable, which is precisely what § 11 exists to protect.

Making `WorkoutShape` the common type and `PrescribedWorkout` the issued one closes
this by construction (§ 24): only generation can build a `PrescribedWorkout`,
because only generation has an anchor, a cycle and a programme to build it from. A
projected shape has nowhere to be stored as a prescription. FR-034 is therefore not
a rule anyone has to follow.

**What the round trip is not.** It is not correspondence, and it does not touch
open question 1. A projected shape is not the prescription that motivated the
performance and cannot be: a session can swap an exercise, reorder items, or
abandon sets, and the projection faithfully describes the result rather than
recovering the intent. Comparing a projection against a generated prescription
reports divergences and asserts nothing about which one is right.

**The round trip is lossy, and the losses are the interesting part.** Each is a
`ProjectionGap` rather than an invented value (FR-035):

- **A failed attempt carries no intended count.** The performed record has the load
  and the failure; the repetitions the operator was trying for are not recorded
  anywhere. This is the sharpest gap and it is new information: it says the
  performed model cannot fully describe a missed set, which nothing had noticed
  before the projection was written down.
- **An exact count cannot distinguish `Exactly` from a satisfied `Range`.** A
  performed six reps projects to `Exactly(6)`, where the prescription may have said
  4–6. So comparison must treat a performed count falling inside a prescribed range
  as agreement rather than divergence — the comparison is direction-aware, and that
  asymmetry is a property of the domain rather than a weakness of the test.
- **Observed effort is not prescribed effort.** A recorded RIR is what happened; a
  prescribed effort is guidance. The projection carries the observation as
  `predicted` on `ToEffort` only where the prescription's own shape calls for it,
  and otherwise drops it rather than promoting an observation into an instruction.
- **Slot identity is not in the performed record.** A performed workout has items
  and no slots. The projection assigns slots by position against the template where
  the structure matches, and records a gap where it does not — which is exactly the
  signal that a session was restructured at the gym.

**Alternatives considered**:

- **One type, with the issuance fields optional.** Rejected: it makes an
  unissued prescription representable, so `issue` has to validate what the type
  should have guaranteed, and the § 11 hazard above stays open.
- **Compare printed output as text.** Rejected: brittle against formatting, and it
  cannot express "a performed 6 satisfies a prescribed 4–6".
- **Skip the projection and assert against hand-written expected loads.**
  Rejected: hand-written expectations are what the back-off error came from, and
  an expectation copied out of the record would make the record the
  specification.

**Scope held**: no CLI surface for this. `fitness prescribe --from-performed` is a
plausible command and is not in this feature; the projection exists as a `domain`
function and a test mechanism. Adding a command would mean deciding how a projected
shape is displayed so that nobody mistakes it for a prescription, which is a
question worth its own answer.

---

## D10: The load comes from a reps-and-RIR table, not from a guessed opener

**Decision**: A working load is `anchor × %1RM(reps, RIR)`, where the percentage
comes from the RPE/RIR table published by Reactive Training Systems
(Tuchscherer), and the programme states an RIR per phase rather than an opening
proximity.

**Why the earlier design was wrong.** The linear ladder needed a total gain to
assert; replacing it with an "opening proximity" only moved the guess. As the
operator put it: a load you can do five-by-five without grinding is itself a
five-rep-at-two-in-reserve test, and *the whole point of an entry test is not
having to guess*. An opening proximity is a second guess wearing the first one's
clothes.

An RIR per phase is not a guess. It is the thing periodisation actually
prescribes — accumulate further from failure, intensify closer to it, test at
zero — and combined with the rep ladder it determines the load outright.

**The table.** Published as a grid of RPE against repetitions; it reduces exactly
to one expression, which is what makes it code rather than data:

```text
%1RM = 100 − 2.5 × (reps − 1) − 5 × RIR
```

Verified against every cell consulted: 1 rep at RIR 0 is 100%, 5 at RIR 2 is 80%,
3 at RIR 1 is 90%, 10 at RIR 0 is 77.5%. RPE and RIR are the same scale inverted
(`RIR = 10 − RPE`), so the domain holds RIR, which is what the performed record
already carries.

**A caveat worth carrying.** The published grid is uniform — every extra
repetition costs 2.5% and every repetition in reserve 5%. The underlying RTS data
is not perfectly linear, and this is a rounded presentation of it. It is accurate
enough to prescribe from and should not be mistaken for a measurement.

**What it says about the 2025 block, and what it does not.** Comparing that
block's loads against the table shows them 4 to 12 points below what its recorded
efforts imply. That is *not* evidence the block was under-loaded, because the
comparison is confounded: the only anchor available is derived from the block's
own exit test, and moving that number was the block's whole purpose. Its entry
1RM is unrecorded. So the table is adopted on its own authority rather than
because it reproduces the record — which is the right way round, and the opposite
of how the back-off percentage was nearly arrived at.

**What is still not settled**: the RIR per phase. Accumulation, intensification
and the test each need one, and they are programme parameters rather than
anything to be recovered from a record whose efforts do not agree with its loads.

**Sources**: the chart as published at <https://fitnesscalcs.com/rpe-chart/>,
attributed to Reactive Training Systems and to Zourdos et al. (2016).

---

## D11: The block's percentages are Prilepin's, and the span is derived

**The question D8 left open**: the percentage table for an
accumulation-into-intensification block, and how it scales when duration changes
the rung count. It is answered here, with two corrections to what was recorded
as settled and one question that has to go back to the operator.

### The relation, and where each number comes from

One relation does both phases — the table D10 already adopted, read at RIR 0,
which is what a rep max is:

```text
rm(reps) = 100 − 2.5 × (reps − 1)          % of 1RM for a true reps-rep max
```

**Intensification** runs one set, so the set can be a rep max, and the phase is
a ladder of them: repetitions descend to the target and the load climbs to
`rm(target)`. A 3RM block terminates at 95% because the table says a 3RM *is*
95% — the endpoint D10 made a fact rather than an ambition.

**Accumulation** runs many sets, so no set can be a rep max, and the distance
below it is what needed a source. **Prilepin's chart is that source**, because
it pins the total number of lifts admissible in each intensity band:

| %1RM | reps/set | total lifts | optimal |
| --- | --- | --- | --- |
| < 70% | 3–6 | 18–30 | 24 |
| 70–79% | 3–6 | 12–24 | 18 |
| 80–89% | 2–4 | 10–20 | 15 |
| 90%+ | 1–2 | 4–10 | 7 |

Holding accumulation a constant three repetitions in reserve — `rm(reps) − 15` —
lands every rung inside its band, and lands the three-rep rung exactly on
Prilepin's optimum. Nothing was tuned to make that happen: the proximity is the
table's own step, and the bands are Prilepin's.

**Three in reserve is a template constant, not an autoregulation gate.**
`primary-lift-progression.md` forbids RIR as an input to a derivation, and this
does not breach it: what that rule excludes is a *recorded* effort feeding a
decision, which introduces a decision point resolved by mood. A planning
constant chosen once, in code, from a published table is § 9's "deterministic
derivation is code" — the same standing the endpoint already has. The handover
lists "an RIR per phase" among the parameters the operator rejected; it was
rejected as *an authored parameter*, and it is not one here.

### The tables this produces

The phase split is the one already recorded: week 1 is the entry test, and the
remainder splits with intensification dropping first. Accumulation's repetitions
descend to 2 over its rungs; intensification's descend to the target over its
own; sets are 5 across in accumulation and 1 in intensification.

```text
7 weeks — 1 test, 3 accumulation, 3 intensification, target 3RM
  wk 2  accum  5×4  77.5%  20 lifts       wk 5  intens 1×5  82.5%
  wk 3  accum  5×3  80.0%  15 lifts       wk 6  intens 1×4  88.75%
  wk 4  accum  5×2  82.5%  10 lifts       wk 7  intens 1×3  (test)

11 weeks — 1 test, 5 accumulation, 5 intensification, target 3RM
  wk 2  accum  5×6  72.5%  30 lifts       wk  7  intens 1×7  82.5%
  wk 3  accum  5×5  75.0%  25 lifts       wk  8  intens 1×6  85.6%
  wk 4  accum  5×4  77.5%  20 lifts       wk  9  intens 1×5  88.75%
  wk 5  accum  5×3  80.0%  15 lifts       wk 10  intens 1×4  91.9%
  wk 6  accum  5×2  82.5%  10 lifts       wk 11  intens 1×3  (test)
```

**Duration changes where the block starts and never where it finishes**, which
is the property the linear ladder already has and the reason its step is derived
rather than authored. A 7-week block climbs 6.25 points a week through
intensification; an 11-week block climbs 3.1.

**The 8-week case reproduces the 2025 block's structure exactly** — 5×5, 5×4,
5×3, 5×2, then 1×5, 1×4, 1×3. That is corroboration rather than fitting: the
repetitions fall out of the phase split and the target, and were compared
against the record afterwards. The *loads* do not agree, and should not — that
block opened at 63% of its own estimated 1RM, which the operator has said was a
guess.

**Two rungs exceed Prilepin and want one set fewer.** 5×6 is 30 lifts and 5×5 is
25, against a band admitting 24. Four sets on both — 24 and 20 — brings them
inside, and the chart's weightlifting provenance argues the same way: a squat
has far more time under tension than a snatch, so its totals should run lower
rather than higher.

### Correction 1: the wave is not a drop in load

The handover records that "the second phase restarts at higher reps and lower
load than the first ended", from the 2025 block's 5×2@80 into 1×5@77.5. Under
this derivation intensification opens at *the same* load accumulation left off
at, with the repetitions jumping from 2 to 5 and the sets collapsing from 5 to
1.

The drop in the record is an artefact of that block's accumulation ramp being
unusually steep — it climbed from 63% to 84% in four weeks because it started
too light, not because it was meant to overshoot. What is real in the
observation is that **the set gets much harder while the session gets much
easier**, and that survives here intact. A load that has to fall is a constraint
nothing in the literature asks for.

### Correction 2: the ladder span is derived at both ends, and D8 closes

`ladder_start` and `ladder_end` have been `TODO` since the feature began,
because neither could be chosen without guessing. Under `v2` neither is chosen:
intensification's endpoint is `rm(target)` from the table, and its start is
where accumulation finished, which Prilepin fixes. The `TODO` stays in the
document only for as long as `v1` does.

Correction 3 below moves that endpoint again — from the entry anchor to what has
actually been lifted since — but not back into anyone's hands. It stays derived.

### Correction 3: the block plans a gain, and the literature says how much

**Written after two wrong turns, both mine.** The first draft prescribed every
intensification week against the entry test's 1RM, which made the implied 1RM
climb 97.1 → 97.9 → 98.6 → 99.3 → 100.0% and arrive at exactly what was tested
at the start — a maintenance block wearing a periodisation costume. The second
draft concluded that planning a gain needs a number and there was nowhere honest
to get one, and proposed deriving the load from performed top sets instead.

That was wrong, and the operator named the error precisely: what they rejected
was *themselves* picking a percentage or a weight out of the air, not a number
the literature supplies. Three sources of expertise are available — the
published literature, the performed record, and what the operator states — and
refusing the first one because the third had declined to guess is how this went
round in circles.

**The literature is unanimous, and the number is 105% of the entry 1RM.**

| Source | What it plans |
| --- | --- |
| Russian Squat Routine, 6 weeks | Ends with a single at **105%** of the starting max; 5–10% expected |
| Arbic, 17-week block periodisation | Tests at **105%** of the *original* 1RM; built so the lifter can double it |
| Meet attempt convention | Third attempt 103% standard, **105%** aggressive; a PR attempt is 102–107% of the previous best |
| Peaking-block main work | 87–95% of the entry max, arriving above 100% only at the end |

**It is a convention, not a measurement, and it should be recorded as one.**
105% is suspiciously round, and it is round in every source, which is what a
shared convention looks like rather than what a finding looks like. Prilepin's
bands came out of thousands of training logs and the repetitions-in-reserve
table is a rounded presentation of real data; this is a number everyone repeats.
As the operator put it: if everyone else is inventing it, we can invent it too.

That is not a shrug. A shared convention is worth more here than a private
invention would be, for two reasons that have nothing to do with it being
correct. It makes this block comparable with published ones — a programme
finishing at 105% can be read against every other programme finishing at 105%.
And it is **falsifiable against the operator's own record**, which a number
picked in this room would not be: each block ends in a test, so after two or
three of them the exit results say whether 105% was optimistic, pessimistic or
about right for this lifter.

**Revising it then would be legitimate where fitting it now would not**, and the
difference is worth stating precisely, because this repository has already made
the mistake once with `light_of_heavy`. Fitting a parameter to the record
in advance produces a number that reproduces the record and predicts nothing.
Checking a stated convention against outcomes it did not see is how a convention
earns or loses its place. The first is circular; the second is a measurement.

Until then it stands, and asking the operator for a replacement would be the
mistake — not using it.

**A note on what this vindicates.** `v1`'s ladder ended at 105%, and the
handover dismissed that as "the linear model's invented 105% endpoint". It was
not invented. It is the standard figure, and it was discarded on the grounds
that nobody could justify it — at which point nobody looked.

**In our terms**, where the exit test is a rep max rather than a single:

```text
endpoint = 105% × rm(target)
         = 1.05 × 95%        for a 3RM target
         ≈ 100% of the entry 1RM
```

A 3RM block plans to exit with a **triple at about the entry test's one-rep
max**, which is a claim a person can hold in their head, and it falls out of two
literature facts rather than out of anyone's optimism.

### The tables, with the endpoint applied

```text
11 weeks — 1 test + 5 accumulation + 5 intensification, target 3RM
  wk  2  accum   5×6  72.5%  30 lifts
  wk  3  accum   5×5  75.0%  25 lifts
  wk  4  accum   5×4  77.5%  20 lifts
  wk  5  accum   5×3  80.0%  15 lifts
  wk  6  accum   5×2  82.5%  10 lifts
  wk  7  intens  1×7  82.5%   → implies a 1RM of  97.1% of entry
  wk  8  intens  1×6  86.8%   → implies a 1RM of  99.2%
  wk  9  intens  1×5  91.1%   → implies a 1RM of 101.2%
  wk 10  intens  1×4  95.4%   → implies a 1RM of 103.2%
  wk 11  intens  1×3    —      exit test: no load prescribed, 99.8% is the
                               warm-up target and 105% is what it is for

7 weeks — 1 test + 3 accumulation + 3 intensification, target 3RM
  wk  5  intens  1×5  82.5%   → implies a 1RM of  91.7% of entry
  wk  6  intens  1×4  91.1%   → implies a 1RM of  98.5%
  wk  7  intens  1×3    —      exit test; the same 105% endpoint, reached in
                               three rungs rather than five
```

**The implied 1RM climbing past 100% is the point**, and it is what the operator
described: the intensification weeks land above what the entry test predicts,
and the exit test confirms the gain rather than discovering it. A shorter block
climbs the same span in fewer, larger steps, which is the property duration has
had since D2.

**No load is prescribed for a test week.** `WeekKind::Test` already carries no
percentage, which is why the type is a variant rather than a flag. The endpoint
is the warm-up ramp's target and is presented as an expectation. The tables in
the first draft of this decision showed it in the test row as though it were a
prescription.

**The second draft's measured-anchor mechanism is dropped.** Re-deriving each
intensification week from the block's performed top sets would work, but it
solves a problem that does not exist once the endpoint is known, and `v1`'s
stall and reset protocol already covers a block that turns out to have been too
ambitious.

### What is left authored

Duration, the target repetition count, and the entry test. Every load in the
block comes from those three plus three literature constants: the
repetitions-in-reserve table, Prilepin's bands, and the 105% endpoint. **D8
closes.**

**Sources**: Prilepin's chart as published at
<https://www.precisionpointtraining.com/strength-training-articles/prilepins-chart/>
and at <https://70sbig.com/blog/2012/05/prilepins-chart/>, which carries the
weightlifting-provenance caveat; block-phase intensities from eliteFTS, *Block
Periodization for Powerlifting: Revisited and Revised*; the
descending-repetition peaking convention from
<https://www.sugdenbarbell.co.uk/routines/Tokars-5x3-System>; the
repetitions-in-reserve table from D10.

**Sources for the 105% endpoint**: the Russian Squat Routine as published at
<https://liftvault.com/programs/powerlifting/russian-squat-routine-spreadsheet/>
and <https://www.castironstrength.com/russian-squat-routine/>; Brad Arbic's
17-week block periodisation programme at
<https://liftvault.com/programs/powerlifting/17-week-block-periodization-powerlifting-peaking-program-by-brad-arbic/>;
meet-attempt convention from
<https://www.castiron-lift.com/blogs/news/powerlifting-meet-prep-peaking-advanced-uk>;
Smolov's explicitly planned weekly load increases at
<https://www.powerliftingtowin.com/smolov/>.

## D12: Three phases, and a plan with no repetitions in reserve

**The operator brought research to this, and it changed three things D11 had
settled**: how short a block can be, how its weeks divide, and — after a
correction they made directly — where accumulation's loads come from.

### The split is stated, and it is a rotation rather than a table

```text
8 weeks   3 accumulation, 3 intensification, 2 realisation
9 weeks   4, 3, 2
10 weeks  4, 4, 2
11 weeks  4, 4, 3
12 weeks  5, 4, 3
```

The first four rows are the operator's. The fifth is not authored: **each week
beyond the eighth goes to accumulation, then intensification, then realisation,
in rotation**, which reproduces all four stated rows and answers every duration
they did not state. A rule was worth having because the calendar hands over
whatever it hands over, and a table would refuse a window nobody had tabulated.

**One correction, settled with the operator on 2026-08-18.** The research as
stated gave the fourth row as twelve weeks at 4-4-3, and 4-4-3 sums to eleven.
The first three rows sum exactly, and the increments walk accumulation →
intensification → realisation in order, so eleven weeks is 4-4-3 and twelve is
5-4-3. That matters concretely: the operator's autumn window is the eleven-week
one.

### Realisation is a third phase, and the literature has it

D11 had two phases with the exit test as the last intensification week. Every
source consulted has three, and gives the third one a length:

| Source | Accumulation | Intensification | Realisation |
| --- | --- | --- | --- |
| Bartolomei et al. (PMC4637911), Table 5 | 2–6 weeks | 2–4 weeks | 2 weeks |
| Hevy Coach, *Block Periodization* | 3–6 weeks | 3–6 weeks | 1–3 weeks (7–10 days short taper, 3 weeks long) |
| Kilo, *Principles of Periodization* | 3-week mesocycles | 3-week mesocycles | — |

So 3-3-2 as a floor is not merely the operator's preference; it is the shortest
arrangement that gives all three phases the length their sources ask for. The
intensities agree too: accumulation submaximal, intensification 75–90%,
**realisation 90% and up with volume reduced** — which is what the block's last
rungs come out at without being told to.

### The duration counts phase weeks, and the calendar carries one more

3 + 3 + 2 is eight and leaves no week for the entry test, so the entry test is
not inside the count. It is taken the week before the block opens, which is also
how the operator's calendar reads it: a 3RM on Friday 18 September, then the
phases from the week of 21 September to 29 November — ten weeks, which is 4-4-2.

`Block::duration_weeks` is therefore what the table above counts and
`Block::total_weeks` is one longer. `MINIMUM_WEEKS` is 8, not 7.

### The block exits on a single, and that is where the 1RM belongs

**The operator asked whether two lifting days a week should chase a 3RM on one
and a 1RM on the other, instead of one being lighter.** Settled on 2026-08-18:
no — the second session stays the same week's rung at 85%, and the single arrives
as the last rungs of realisation.

Three reasons, in the order they matter:

- **A second target measures nothing new.** Every load in this block is derived
  through `rm(reps)`, in which a 3RM *is* 95% of a 1RM. A 1RM day tells you
  something the 3RM day does not only if that table is wrong, and in that case
  the table is what wants revising rather than the week.
- **Two ladders both ending near-maximal double the top-end exposure on one
  lift**, and this design has deliberately given up RIR as an input, so there is
  nothing left to absorb a bad week.
- **Realisation is where the literature puts singles**, and there is now a
  two-to-three week phase to hold them. The descending-repetition peaking
  convention is a sequence of rep maxes ending in one.

So the repetition ladder runs from intensification straight through realisation
to a single, and **the exit test is a single whatever the entry test was**.
Entering on a triple and exiting on a single is deliberate: a cold maximal single
measures technique as much as strength, and a peaked one is what the realisation
weeks were for. The operator's own record already does this — the March 2025 back
squat block entered at `5×1@95` and exited at `1×1@110`.

**The endpoint therefore simplifies.** D11 had `105% × rm(target)`, which for a
3RM target came to about 100% of the entry one-rep maximum and needed explaining
every time. With a single at the end it is 105% of the entry one-rep maximum
flat, which is what the literature actually says, and the block plans a 5% gain
measured in the unit it was planned in.

**The third input changes meaning, and there are still three.** It was "the
repetition maximum the block is for"; it is now the repetition count of the entry
test, and nothing else in the block reads it. Every load comes from the duration
and the literature constants.

### Correction 4: there is no RIR in a percentage-based plan

**The operator's correction, 2026-08-18, and it is the important one in this
decision.** D11 placed accumulation three repetitions in reserve below the
maximum for its repetition count — `rm(reps) − 15` — and argued at length that a
planning constant taken from a published table was not the autoregulation
`primary-lift-progression.md` forbids. That argument was about the wrong thing.
**A percentage-based plan states percentages.** Reaching one by subtracting a
number of repetitions in reserve from a maximum puts an RIR parameter in the
plan whatever the arithmetic is called, and `5 × RIR` is a coefficient of the
RTS grid rather than anything Prilepin published.

**Prilepin's chart places the phase without it, and the column that does it was
sitting unread.** The chart's repetitions-per-set column says where a set of a
given size belongs: threes to sixes are admissible at any intensity, a double
first appears at 80%, a single at 90%. Accumulation descends to a double, so its
heaviest rung is one, and 80% is the lightest load the chart will put a double
at. Every earlier rung is one repetition more and **2.5 points lighter, which is
the repetition-maximum table's own slope along the repetitions axis** — the one
coefficient of that grid which is not about effort.

```text
accumulation(reps) = 80% − 2.5 × (reps − 2)
```

Both numbers are published, neither is chosen here, and no reserve appears in
either. The loads it produces are the same shape 2.5 points lower:

| reps | 7 | 6 | 5 | 4 | 3 | 2 |
| --- | --- | --- | --- | --- | --- | --- |
| load | 67.5% | 70% | 72.5% | 75% | 77.5% | 80% |
| sets | 4 | 4 | 4 | 5 | 5 | 5 |
| lifts | 28 | 24 | 20 | 20 | 15 | 10 |

Seven is as far up as it goes: the longest block the next section admits gives
accumulation six weeks, and its first rung is one repetition above that.

Every one of those pairings still falls inside the band Prilepin admits for its
load, which is the property the reserve constant was introduced to obtain.
`PER_REPETITION_IN_RESERVE` is deleted rather than left unused: a public constant
whose only purpose was this is an invitation to make the same mistake again.

**Scope, as the operator stated it**: the primary lift's block periodisation.
RIR as an *observation* on performed sets is untouched, the accessory schemes are
untouched, and the test week's open single is still `Rir::Zero`, which is how
that type says "a maximum attempt" rather than a reserve.

### The tables

```text
10 weeks — entry test the week before, then 4 accumulation, 4 intensification,
2 realisation. The operator's autumn window. Eleven weeks of calendar, ten of
plan.

  wk  1  test    3RM             the anchor; no load prescribed
  wk  2  accum   4×5   72.5%     20 lifts
  wk  3  accum   5×4   75.0%     20 lifts
  wk  4  accum   5×3   77.5%     15 lifts
  wk  5  accum   5×2   80.0%     10 lifts   ← accumulation exits at 80%, always
  wk  6  intens  1×6   80.0%     → implies a 1RM of  91.4% of entry
  wk  7  intens  1×5   85.0%     → implies  94.4%
  wk  8  intens  1×4   90.0%     → implies  97.2%
  wk  9  intens  1×3   95.0%     → implies 100.0%
  wk 10  realis  1×2  100.0%     → implies 102.5%
  wk 11  realis  1×1    —         exit test: 105% is what the ramp is for

8 weeks — 3, 3, 2. The floor, in nine weeks of calendar.

  wk  1  test    3RM
  wk  2  accum   5×4   75.0%
  wk  3  accum   5×3   77.5%
  wk  4  accum   5×2   80.0%
  wk  5  intens  1×5   80.0%     → implies  88.8%
  wk  6  intens  1×4   86.25%    → implies  93.2%
  wk  7  intens  1×3   92.5%     → implies  97.3%
  wk  8  realis  1×2   98.75%    → implies 101.2%
  wk  9  realis  1×1    —         exit test, the same 105% in fewer rungs
```

**Two things worth noticing, neither of them designed in.** The implied maximum
crosses 100% at the intensification/realisation boundary in the ten-week case and
inside realisation's first rung in the eight-week one — the phase boundary and the
point where the plan passes the entry maximum arrive at the same place without
being made to. And realisation's rungs come out at 100% and above, which is the
90%-and-up zone every source assigns that phase.

**One ladder runs through intensification and realisation**, repetitions
descending and load climbing without a break at the boundary. A discontinuity
there would be a number somebody chose, and there is none available. What
realisation contributes is the last rungs; what makes it a taper is that its
weeks are the ones to strip volume from, which is a question for the second
session and the accessory schemes rather than for the top set.

### The upper bound is derived too

The top set opens at the load accumulation finished on, at a repetition count
equal to the number of weeks the two phases it spans hold. At fifteen weeks that
is a set of nine at 80%, and 80% is exactly a nine-repetition maximum. At sixteen
it is a set of ten at 80%, which is heavier than a ten-repetition maximum — not a
hard set but an impossible one. **So the longest block is fifteen weeks, and
nobody authored that number.** It also lands about where the literature stops
describing one block and starts describing two: 6 + 4 + 3 is thirteen.

### What is left authored

Duration, the repetition count of the entry test, and the entry test itself. Every
load comes from those plus three published constants: Prilepin's chart, the
repetition-maximum table's repetitions axis, and the 105% endpoint.

**Sources**: the operator's research and its citations —
<https://doclachjames.substack.com/p/making-sense-of-strength-periodisation>,
Bartolomei et al. at <https://pmc.ncbi.nlm.nih.gov/articles/PMC4637911/>,
<https://barbend.com/different-types-of-training-periodization/>,
<https://trainkilo.com/blogs/inside-the-system/principles-of-periodization-the-foundation-of-long-term-progress>
and <https://hevycoach.com/glossary/block-periodization/>. Prilepin's chart and
the 105% endpoint keep the sources listed under D11.
