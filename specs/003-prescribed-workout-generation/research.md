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
| Light of heavy | **88.5%** | Solved from three weeks of light/heavy pairs. A flat −10kg offset fits those three equally well and was rejected for not being portable to a different anchor — itself an unconfirmed judgement |
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
