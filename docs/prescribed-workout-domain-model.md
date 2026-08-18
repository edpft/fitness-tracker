# Prescribed Workout Domain Model

*The prescribed side of § 11. Companion to `gym-workout-domain-model.md`, to
`primary-lift-progression.md`, and to the constitution, none of which it
overrides.*

*The primary strength slot is governed by `primary-lift-progression.md`. Where
this document once described a scored top set against an affirmative gate, that
was a competing design and it has been withdrawn — the progression schema is
authoritative and this document has been brought into line with it. The type
sketches have not been compiled.*

*The generative test — **would this model, given the current programme, produce
the routines currently in use?** — has been run in part, by hand, against the
164 normalised workouts. What it found is recorded where it bears: the
hypertrophy block was missing a slot, and the back-off rule did not reproduce
the record until it was restated as a percentage of the top set.*

---

## Two entities and two rules

| | Kind | What it is |
|---|---|---|
| **Workout template** | Code | A way of *building* a prescribed workout: which slots exist, in what order, under what constraints. |
| **Programme** | Code + § 12 data | A rule for generating a series of prescribed workouts, plus its authored inputs. |
| **Prescribed workout** | § 12 data | The concrete issued prescription. The only prescribed entity stored. |

The template is a builder, so there is no template value anywhere. `V1` is a
module, and selecting a variant is selecting among programme types:

```rust
pub enum Programme { V1(v1::Programme), V2(v2::Programme) }
```

Variants coexist and are append-only: a programme written against V1 keeps
generating against V1 after V2 exists, so a variant is never edited or removed,
only added.

§ 14 governs a narrow subset — warm-up percentages, loading tables, plate
quantisation — where only the current value is required, because what they
generated is recorded concretely in the prescribed workout. Open question 7
enumerates the ones a generated session actually needs; none is stored today,
and that is the nearest thing to a hard blocker in this document.

---

## The template

Eugene Teo's hybrid bodybuilding template, minus cardio (a separate Peloton
session). The rep schemes already diverge from his.

Five blocks in fatigue order: plyometric, power, strength, hypertrophy,
mobility.

```rust
pub mod v1 {
    /// Exactly one primary. Two and zero are both unconstructible.
    pub enum Primary { KneeDominant, HipDominant, UpperPush, UpperPull }

    pub struct StrengthBlock {
        pub knee_dominant: Exercise,
        pub hip_dominant: Exercise,
        pub upper_push: Exercise,
        pub upper_pull: Exercise,
        pub primary: Primary,
    }

    pub struct HypertrophyBlock {
        pub arms: Superset,      // biceps + triceps
        pub forearms: Superset,
        pub core: Exercise,      // single, never supersetted
    }
}
```

**Quality is a property of the block, not of the exercise.** A front squat in
the strength block and an explosive jump squat in the power block are the same
movement serving different purposes, so the exercise vocabulary cannot carry it.
It is not a property of the item either: what makes four slots one block is a
shared purpose, and the composition constraints are block-level rules that need
a block to attach to.

**`Quality` is not a type.** A workout is at most one block of each quality, in
fatigue order, and the blocks are differently shaped. That encodes as a struct
with five fields: names carry the quality, order carries the ordering. The
`Vec<Block>` alternative needs runtime checks for at-most-one-of-each and
correct ordering, which is what § 24 says not to do.

**Order is derived, not authored.** Ordering is a property of the quality —
cheapest first, then most CNS-demanding, then most technical, nothing that
pre-fatigues what follows — so nothing carries an index.

**Quality does not dictate the loading.** Teo's strength block is 3–6 reps; this
one opens with a top set of 1–3. His hypertrophy block is high-rep; this one is
3 × 4–6. The quality names the slot's purpose and nothing more.

**Blocks hold slots, not exercises.** A slot is a selection constraint; the
filler is a programme input and rotates under it. The evidence is direct: the
snatch was replaced by box jumps because its technical demand limited
progression and its warm-up made the session too long — the power slot survived
unchanged, the filler failed. And the hip-dominant slot is *the lower-body
pattern the primary is not*, a constraint referencing another slot's filler,
inexpressible if the block held exercises directly.

### Structural facts

These bound what is constructible. They are not configuration.

- **Two sessions per cycle**, a light variant and a heavy variant. Both are
  builds of the same template, differing in fill and in the primary's loading
  table.
- **The strength block requires four patterns** — knee-dominant, hip-dominant,
  upper push, upper pull — and admits multi-joint compound movements. A block
  missing a hip-dominant slot should not be constructible.
- **The upper strength pair is supersetted; the lower pair is not.** Not a
  preference. The antagonist-pairing requirement needs no separate expression,
  since the required pattern set already delivers a push against a pull.
- **The hypertrophy block is two supersets and a single slot** — biceps with
  triceps, forearms, and core — admitting single-joint or single-muscle-group
  movements. That is Teo's "address specific weaknesses", and where
  physio-prescribed forearm work belongs. The physio constrains *selection*, not
  prescription: their sets, reps and progression are not adopted, so the work
  progresses by double progression like anything else. "Physio-prescribed" is
  therefore not a property the model carries.

  **The core slot is not supersetted, and that is a structural fact rather than
  an accident of the fill.** It sits last in the block, after the forearms
  superset and before mobility, in all fifteen sessions since 15 June — always
  one exercise, never paired. Typing it as a third `Superset` would make the
  block unable to express what is actually run. Its filler has been the
  up-to-down cable twist throughout, which is loaded and progresses by double
  progression like the rest of the block.
- **Exactly one strength slot is primary, and everything asymmetric derives from
  it.** The primary gets a warm-up ramp and a top-set/back-off scheme; the other
  lower-body slot becomes accessory-style precisely because it is not primary.
  The upper pair are symmetric only because neither is primary.

**Plyometric** and **power** have no stated composition invariant. Open question
3.

`Pattern` does not appear in the template. If the strength block names its four
slots, the field name *is* the pattern; a `pattern:` field beside it is a second
source of truth that can disagree. Pattern is exercise vocabulary, used to
validate a fill.

---

## Two relations over exercises

An exercise belongs to an **exercise family** and has a **movement pattern**.
Different vocabularies, different consumers.

**Pattern** is prescribing vocabulary: knee-dominant, hip-dominant, upper push,
upper pull. Coarse by design — a slot needs to know only that the primary is
knee-dominant so the accessory lower slot must be hip-dominant.

**Family** is analytical vocabulary: front squat, back squat and Zercher squat
are one family. Charting a family over time is meaningful; charting a pattern is
not, since it would put a front squat and a leg press on one axis.

The two are hierarchical: every family has one pattern, but a pattern contains
more than its families. The squat family is knee-dominant; knee-dominant also
contains leg extensions, which are no part of it.

Pattern *can* therefore be derived from family, but need not be. Declaring
pattern directly on the exercise is sufficient for prescription, and when
families arrive the hierarchy becomes a consistency check rather than a rewrite.
**This keeps the performed model's open question 4 off the critical path.**

---

## The programme

A programme generates a series of prescribed workouts. Its purpose is always to
increase the primary exercise's 1RM.

### Inputs

**Two of these carry the plan and the rest are furniture.** The primary lift's
whole loading series is a function of duration and entry anchor, which is what
makes "a number of weeks and a starting 1RM" a complete statement of the
generator's job.

- **Duration** — how many weeks the block runs for. The last is the test.
- **An entry anchor** — the starting 1RM, with its provenance.
- **The ladder's endpoint**, as a percentage of the anchor. Authored, because it
  is a claim about achievable gain; the weekly step is derived from it and the
  duration.
- **The primary exercise**, and which strength slot is primary.
- **Slot fills**, including the variations alternated across the two sessions in
  a cycle (Nordic curls one day, back extension the other).

**Why that one alternates, stated by the operator on 2026-08-18: there is no
single hinge accessory.** The pattern splits into hamstring-focused work and
lower-back-focused work, and one exercise does not cover both — so the slot
takes two fills and shows one per session. It is a fact about the vocabulary,
not a loading device, and nothing about periodisation follows from it. The
primary slot does not alternate for the same reason inverted: one lift does
cover it, and **both sessions of the week run that lift.**

Fills are inputs, not choices the programme makes. Generation produces the
loading series, not the exercise selection.

### Anchor entry and exit

**A programme ends with a test.** It starts from a test, a previous test, a
previous e1RM, or an asserted value.

Because the exit test of one programme is the entry anchor of the next, those
four sources are not four independent cases. Once a series of programmes is
running there is one source; the others are bootstraps, used for the first
programme or after a gap. They differ in provenance rather than in kind — a test
is measured, an e1RM derived, an asserted value neither — so the anchor carries
its provenance.

**The anchor is fixed for the block's duration.** It is the starting 1RM, and
every prescribed load derives from it. What climbs across the block is the
ladder's position — a percentage of the anchor — not the anchor itself. An
earlier version of this section had the anchor advancing weekly, which describes
the same load sequence from the wrong end and leaves the block with no endpoint.

**The anchor of the block that produced the current record is 90kg, tested
2026-07-03**, landing record 10. That session ramped to a single at 90 and then
failed 95, which is what makes 90 a measured anchor rather than an asserted one:
it is the heaviest single completed under test, and the failure above it is the
evidence bounding it. The failed 95 is the corpus's only zero-rep set, and
reading it is what the sentinel below exists for.

**Only a test replaces the anchor, and a test ends a block**, so within a block
it is a constant. A stall does not touch it: a stall suspends the ladder and is
evidence that the plan was too ambitious, not evidence about where the block
started. **There is no mechanism by which a good session raises it**, which is
the whole of what separates this from the scored design withdrawn from this
document.

**Do not read an e1RM off a submaximal set.** A set left with repetitions in
reserve says nothing about a maximum, whatever a formula returns. Only a set
taken to failure or a genuine single supports an estimate. The arithmetic is
available on all 3,755 sets in the record and is meaningless on nearly all of
them.

### The plan, and the rate

**The programme is a linear block**: intensity ascends across the duration and
the block ends in a test. `primary-lift-progression.md` § The plan is
authoritative; what matters here is that it takes two inputs — a number of weeks
and a starting 1RM — and that the endpoint is authored while the weekly step is
derived from it and the duration.

That direction is deliberate. An endpoint is a claim about how much can be
gained in the time available, and personal history and a reference programme can
both inform it. A weekly step is a number with nothing behind it, and
multiplying it by a duration produces an endpoint nobody chose.

**A standard template answers the total-gain question by embedding one**, which
is why adopting a template is a real answer where deriving a rate from first
principles is not. 5/3/1 advances a training max 5kg every four-week cycle, so
roughly 1.25kg per week for a lower-body lift. A classic linear block finishes
near 102.5–105% of the entry 1RM. Personal history bounds it further, and the
boundary that matters is **regain versus new ground** — ground already covered
comes back fast and ground never covered does not.

**Only the primary uses this.** Every other slot in the session runs double
progression against observed history, which works and is not being replaced. The
primary needs something else because only the primary has a 1RM the programme is
trying to move.

### Failure is a separate mechanism

The plan and the response to the plan failing are two different things, and
conflating them is a mistake this document has already made once.

A stall **suspends the ladder** rather than altering the anchor: the drop is
taken from the failed load, the re-climb runs at the reset's own rate, and the
ladder resumes where it left off when the sequence returns to the failed load.
Two resets exist — −10% at +5kg weekly, then −5% at +2.5kg weekly — each costing
four weeks, so a stall has a fixed price whichever is in play.

In the older vocabulary of this section, that is **restart** rather than
**interruption**: the load sequence falls and re-climbs ground already covered,
where an interruption would only ever pause a rising sequence. The distinction
still matters, because a falling sequence is something the model has to be able
to express.

**One consequence bounds the endpoint.** A reset costs four of the block's 7 or
11 climbing weeks, so a ladder leaving no room for a single reset cannot survive
a stall inside the block. That is a real constraint on how ambitious an endpoint
can be, and it is the useful half of what open question 5 was asking.

### Slack

Slack is the gap between the load prescribed and the true maximum. It is what
makes a prescribed set something to complete rather than something to attempt.

**The ladder spends it, deliberately.** The anchor is fixed and the percentage
climbs, so early weeks sit well below the real number and late weeks — past 100%
— sit above it. The margin narrows week by week by design, and the stall, when
it comes, is the ladder finding the ceiling.

That is not a defect to be designed out; it is how a negative gate discovers
anything. The ladder's endpoint is an intention and the test says how much of it
was real.

**5/3/1 makes the same trade explicitly, and further down.** Its percentages run
off a *training max* set at 90% of the tested 1RM, so its whole ladder is
shifted below the real number and the slack is never spent to zero. That is a
legitimate alternative to anchoring on the tested value and running past 100%,
and it is worth recording as the road not taken: it buys a lower stall rate at
the cost of a lower endpoint. What is not available is 5/3/1's *feedback* — the
AMRAP top set — which this programme rejected on resolution grounds.

### What the evidence supports

- Volume-equated meta-analysis finds periodised training produces greater 1RM
  gains than non-periodised, and undulating greater than linear — but only in
  trained individuals. (Williams et al., *Sports Med* 2022, PMID 35044672.)
- Other systematic reviews find no difference between linear and undulating for
  upper or lower body strength. (Harries, Lubans & Callister, *JSCR* 2015.)
- Scheduled deloads are convention, not a finding. A one-week cessation at the
  midpoint of a nine-week programme produced *worse* lower-body strength than
  continuous training, with no difference in hypertrophy, power or endurance
  (Coleman et al., *PeerJ* 2024). That study used full cessation rather than a
  reduced-load week, so it is not a direct refutation, but nothing found
  supports a fixed every-fourth-week deload.

Taken together: having a structure matters more than which structure. The choice
of shape is not the load-bearing decision.

### Scheme

Scheme is derived, not stored — a total function of `(slot, primacy, session
role)`. At present: the primary gets top-set/back-off, all other strength and
hypertrophy slots get double progression, plyometric, power and mobility get
static. A slot therefore collapses to just an exercise, and a primary-style
scheme on a non-primary slot becomes unwritable.

**Session role is the third input.** The light and heavy sessions share
template, primacy and slots; they differ in fill and in the primary's loading.
Both loadings are anchor-relative, so this is one scheme variant with different
percentage tables, not two rules.

Scheme selection is programme-level, not template-level. Programme v1 gave the
primary an RPE cap; v2.1 gave it a percentage anchor. The structure was
untouched, so both are programmes against `V1`. Warm-ups follow scheme for the
same reason: the template says only that exactly one strength slot is primary,
never which, and never what that earns it.

**Where a slot's numbers come from depends on primacy.** The primary draws from
programme state; every other slot draws from observed history. Prescription may
read the performed layer, and § 11 forbids the reverse.

#### The back-off is a percentage of the top set, not of the anchor

**Back-off load is 85% of the prescribed top set**, quantised to the plate grid.
Not a percentage of the anchor, and not a series of its own.

This matters because the two readings diverge, and the record says which is
right. Checked against the corpus:

| Session | Top set | ×0.85 | Prescribed | |
|---|---|---|---|---|
| Fri 14 Aug | 87.5 | 74.4 | 75 | ✓ |
| Mon 10 Aug | 77.5 | 65.9 | 65 | ✓ |
| Fri 7 Aug | 85 | 72.3 | 72.5 | ✓ |
| Mon 3 Aug | 75 | 63.8 | 67.5 | ✗ |
| Fri 17 Jul | 82.5 | 70.1 | 70 | ✓ |
| Mon 13 Jul | 72.5 | 61.6 | 67.5 | ✗ |

The two misses are light-session Mondays before 10 August, and the operator has
confirmed them as operator error rather than a different rule — the back-off was
held at 67.5 across three sessions while the top set moved. That confirmation
matters: it is what makes 85% a stated intention rather than a percentage fitted
to five of six sessions with the sixth explained away.

It is also why the earlier reading of this document could not reproduce the
record. On 3 August the top set rose and the back-off did not, which no function
of the anchor produces — and generation must not reproduce it.

Quantisation is **nearest, ties down**: 85% of 80 is 68, which quantises to
67.5. Settled in `specs/003-prescribed-workout-generation`. The increment is a
generation parameter; the rounding rule is not, because § 9 puts deterministic
derivation in code rather than in data.

The rule is not a property of the back-off. It is a total function of a load and
an increment, so it governs every derived load that meets the grid — warm-up
ramp steps and reset drops as well. A −10% reset from 87.5 is 78.75, exactly
halfway between two increments, and resolves down to 77.5.

### The gate

**Governed by `primary-lift-progression.md`.** What follows is what the rest of
this model has to hold for that schema to work, not a restatement of it.

The primary's top set is prescribed as a **fixed rep count** and executed as
written. It is pass or fail, not scored. The record bears this out: the count is
fixed by session role — heavy singles, light triples — across every session
since the July test, and never varies within a role.

**The gate is negative: the plan proceeds by schedule and the ladder retreats on
evidence.** A miss holds the anchor and re-issues the same loads; a second miss
at the same load is a stall and triggers a reset. Nothing a good session
contains can advance the anchor faster, and no effort report is consulted.

This is the reverse of the affirmative gate this document previously described,
and three consequences fall out of the change:

- **A miss must be detectable**, where the affirmative gate never needed to
  detect one. This is what the zero-rep sentinel below is for.
- **A miss and an absence are no longer equivalent.** Under the affirmative gate
  both simply produced no evidence and both re-issued. Here a miss counts toward
  a stall and an absence does not, so the model must tell them apart. A
  prescribed workout with no corresponding performance is absence; one whose top
  set came back at zero reps is a miss.
- **`Rir` is captured and consulted by nothing.** It stays on the set as an
  observation, retained against a retrospective check that may never be wanted.
  It is not an input to any derivation, and a derivation that reads it is a
  defect.

**One session role gates the anchor** — currently the heavy session. Which role
gates is programme configuration. Session roles within a cycle are an ordering,
not state.

**Non-primary slots keep the affirmative gate and need no missed-workout
concept.** Double progression reads the last performance of its exercise and
asks whether it reached the top of the range. A skipped session changes nothing,
so the next prescription re-issues the same numbers. Mobility and plyometric
slots do not progress.

That the two halves of one session now gate in opposite directions is deliberate
and not an inconsistency. The primary is prescribed against an authored anchor
and can outrun the lifter, so it needs a retreat. Every other slot is prescribed
against its own observed history and cannot outrun anything, so it has nothing
to retreat from.

**Step-back** is the one operation requiring absence to be recognised, and it
keys on accumulated absence rather than any single session. Detraining work has
strength largely retained across one to two weeks off with measurable loss past
that, which gives "more than a couple of sessions, step back" an evidence base,
while what to do about a single missed session is coaching convention. *(Mujika
& Padilla is the standard reference; recalled, not verified.)*

---

## The prescribed workout

```rust
pub enum Target<M> { Exactly(M), Range { low: M, high: M } }

/// Volume, intensity, density — where intensity has three currencies and
/// exactly one is pinned. The "prescribes nothing" case is absent by
/// construction: every variant pins at least one axis.
pub enum Prescribed<M> {
    /// Load and measure pinned; effort is guidance. Warm-ups, back-offs,
    /// the primary top set, double-progression sets 1 and 2.
    Fixed { load: Load, measure: Target<M>, effort: Option<Rir> },

    /// Measure open; effort binds. The third set of the upper superset.
    /// `predicted` is typed apart from `Target`, because a prediction the
    /// set overshoots is not a prescription the set exceeded.
    ToEffort { load: Load, effort: Rir, predicted: Option<Target<M>> },

    /// Load open; effort binds; measure pinned. Programme v1's RPE cap.
    Autoregulated { measure: Target<M>, effort: Rir },
}

pub struct PrescribedSet<M> {
    pub prescription: Prescribed<M>,
    pub rest_after: Option<Target<Duration>>,
    pub warmup: bool,
}
```

**Sets within a slot are heterogeneous, and effort is per-set.** The upper
superset is one to two in reserve on sets one and two, and the third open.

**The primary top set is `Fixed` with an `Exactly` measure and no effort.** Load
pinned, count pinned, nothing open — which is what makes it pass or fail. It was
`Range` with the effort carrying a stopping instruction while the gate was
affirmative; the range was the thing being scored, and with the score gone the
range has nothing left to do.

`Target::Range` survives regardless, because double progression on every
non-primary slot is expressed with it. `Prescribed::Autoregulated` also survives
and is now unreachable from any current programme: it was v1's RPE cap, and no
programme against the present schema issues one. Variants are append-only, so a
v1 programme still generating still needs it — but nothing new produces one, and
that is worth knowing before someone reads the enum as a menu of live options.

**Rest inverts.** Performed `rest_after` is optional because nobody recorded it.
Prescribed rest is the density axis: it is the instruction, naturally a range,
and its absence means no instruction was given rather than unknown.

**Laterality needs no field.** The set boundary is the rest boundary (the NSCA
definition the performed model adopts), so ten each leg then rest is one set of
twenty. "Per leg" is recoverable from the exercise being unilateral.

### Structure

Supersetting pairs slots at execution time, so what is issued is an ordered
sequence with grouping — primary, then push and pull together, then the
accessory — and the named-slot structure is gone. That grouping is structurally
what the performed model calls `WorkoutItem`.

Blocks and slots are construction-time scaffolding and do not survive into what
is issued. One thing must, though: **items are slot-tagged**, or "same slot,
different cycle" stops being answerable, and that comparability was the argument
for slots. Block is derivable from slot.

### Generation

**Event-driven, and the event is a match rather than an arrival.** "A
performance was logged" cannot be the trigger — an unplanned Saturday session
would fire it. The event is *this performance satisfied the outstanding
prescribed workout*, so prescribed↔performed correspondence must exist before
generation can run. Open question 1.

Bounding generation to one session ahead bounds staleness to one session.

**The generation query is per-exercise and unbounded.** Not "the last two
sessions" — double progression needs the most recent performance *of each
exercise in the slot*, whenever it last appeared. The alternating lower
accessory already reaches back further than the previous session.

Because issue is just-in-time, a prescribed workout is placed on a date by
construction. There is no undated reusable prescription: that is the programme.

### What it does not have

**No `Session`.** That container exists in the performed model because sources
fragment one training session into several records. A reconciliation artefact of
observation. Fragmentation is not prescribed, so prescription tops out at the
workout.

**No attempt entity.** A failed attempt is a performed-side fact: Hevy encodes
it as zero reps, which is a sentinel rather than a count. `RepCount` stays
non-zero, and translation reads the sentinel into a distinct outcome so a
failure can never flow into a volume sum or an e1RM as if it were a number. This
resolves the performed model's open question 3.

**The sentinel is `reps == 0` and nothing else.** Hevy also has a `failure` set
*type*, and it is not the discriminator: it appears 77 times across the corpus
and means "taken to failure", which the translator already reads correctly as
zero in reserve. Exactly one of those 77 carries zero reps — the 95kg front
squat of 2026-07-03 — and that is the one genuine failed attempt in 165 records.
Keying on the type would file 76 completed sets as failures.

This is now load-bearing rather than tidy. Under the negative gate a miss is
what triggers a stall, so a failed attempt that the normalised layer will not
represent is a stall the programme cannot see. **Today the domain refuses it:**
`RefusalReason::ZeroReps`, kind `unmodelled` — the corpus's single failed
attempt is currently sitting in `normalisation_refusal` rather than in a
workout. That refusal has to become an outcome before the gate can be built, and
the change is in the Hevy translator and the performed set model, not here.

A failed attempt and an absent one are **not** equivalent under this gate, which
is the reverse of what this document said when the gate was affirmative. A miss
counts toward a stall; an absence holds the anchor and re-issues. They were
never equivalent for anything estimating a maximum either — a failed 95 is
informative and a missed session is not.

---

## Open questions

1. **Prescribed↔performed correspondence.** Generation depends on it.

   Hevy attaches a routine id to a workout started from a routine. The
   historical rate is low — 8 of 164 — but that reflects routines having been
   deleted and workouts started ad hoc, not unreliability in the source. So the
   id is a candidate key rather than a discounted one. What it cannot be is the
   whole answer: it identifies the routine, not the issue. If one routine is
   rewritten in place each cycle, every workout carries the same id.
   Correspondence remains a claim with its own provenance; the id is evidence
   toward it.

   The negative gate raises the stakes here. Distinguishing a miss from an
   absence needs an issued prescription to point at: a session that never
   happened is only recognisable as absence if something says it was due. Under
   the affirmative gate this was a convenience; it is now a precondition.

2. **What "step back" concretely means**, and the absence threshold that
   triggers it. Distinct from the reset protocol, which keys on stalls rather
   than absence — a lifter who trains and misses gets a reset, one who does not
   train gets a step-back, and only the second needs absence measured. Whether
   the two mechanisms should stay separate is itself undecided.

3. **Composition of the plyometric and power blocks.** Neither has a stated
   invariant. The plyometric slot admits both a reps filler (pogos) and a
   duration filler (jump rope), so a slot cannot be typed by measure — a
   consequence rather than a constraint.

4. **Load on mobility work.** `Prescribed` requires a load in two of its three
   variants. A couch stretch has none. `Load::Relative(0)` is the performed
   model's answer and works, but it is an encoding rather than a fact.

5. **What total gain is reasonable for a given duration.** Narrowed rather than
   resolved. The shape is settled — the endpoint is authored as a percentage of
   the anchor and the weekly step derives from it — and two bounds now exist: a
   standard template embeds a rate (5/3/1 gives roughly 1.25kg per week for a
   lower-body lift), and a ladder must leave room for one reset or it cannot
   survive a stall inside its own block. What remains open is the number itself
   for a specific lifter, where the best available evidence is personal history
   and the regain-versus-new-ground boundary within it.

6. **Whether progress is expected during a nutrition deficit.** Bears directly
   on question 5, since the same duration buys different gains under different
   phases. Currently untested; the present block resolves it empirically.

7. **The generation parameter set.** § 14 governs these and requires only the
   current value, because what they produced is recorded concretely in the
   issued prescription. What is known to be needed:

   - **Warm-up ramp** — stated: 4 at 40%, 3 at 60%, 2 at 80%, 1 at 90%, all of
     the session's own top set.
   - **Back-off percentage** — 85%. The rounding is settled and is code, not a
     parameter: nearest, ties down.
   - **Top-set reps by session role** — heavy 1, light 3, read off the record
     rather than stated. Constant within a block; descending reps across the
     block is the textbook variant and is deferred.
   - **The light session's top set as a percentage of the heavy one** — 85%,
     stated by the operator on 2026-08-18. It was 88.5%, solved from three weeks
     of the record; every one of those light and heavy pairs is a flat −10kg
     apart, so the percentage was a ratio fitted to an offset and drifted across
     the three (87.9%, 88.2%, 88.6%) where the offset did not drift at all. A
     percentage is still the right shape — an offset is a far larger relative
     drop at a 60kg anchor — but the number in it has to be chosen rather than
     solved for.
   - **The ladder's start and end percentages** of the anchor. The weekly step
     is
     derived from these and the duration, not authored.
   - **Plate increment** — 2.5kg. The rounding rule that consumes it is code.
   - **Reset drops and re-climb rates** — −10%/+5kg and −5%/+2.5kg, from
     `primary-lift-progression.md`.

   **Duration belongs to the programme, not here.** It is supplied per block,
   and the ladder's step is the span divided by it — so it is an input rather
   than a value to be settled once.

   None of these is stored today. They are the authored inputs a generated
   workout is a function of, and the reason a programme cannot yet run is that
   they live in prose rather than in the store.

---

## Not modelled here

- The Hevy adapter, and how any of this renders into a routine. Routine
  proliferation — whether one routine is rewritten in place or one issued per
  prescribed workout — is not purely an adapter tidiness question, because
  rewrite-in-place is what destroys the routine id's discriminating power in
  open question 1.
- Cycling and nutrition phases, which a full programme should coordinate with.
  These are calendar-driven where strength progression is evidence-driven, so
  they sit above this model rather than inside it.
- The constraint calendar. A holiday with no training is absence: the anchor
  holds, and no prescription is issued, because those sessions are never
  programmed. An unprogrammed slot and an abandoned prescription are different
  facts.
- The exercise vocabulary, the pattern vocabulary, and the family relation.
