# Feature Specification: Prescribed workout generation

**Feature Branch**: `003-prescribed-workout-generation`

**Created**: 2026-08-16

**Status**: Planned. One question was resolved before planning (plate quantisation) and the primary's progression model was revised *during* planning, from a climbing anchor to a fixed anchor with a climbing percentage ladder. See "Questions resolved" and [research.md](./research.md) D2.

**Input**: Derived from the working session of 2026-08-16 rather than from a command argument. `/speckit-specify` was invoked with no description, and the feature taken is the one settled in that session: issue the next prescribed workout from an authored programme and observed history.

**Scope**: The prescribed side of § 11 for one programme against one primary lift, ending at a workout printed to the operator. No canonical layer, no correspondence between prescription and performance, no routine written back to any source.

**Models of record**:

- [`docs/primary-lift-progression.md`](../../docs/primary-lift-progression.md) — governs the primary strength slot. Authoritative on what the generator is for, the anchor, the ladder, the gate, stalls and resets.
- [`docs/prescribed-workout-domain-model.md`](../../docs/prescribed-workout-domain-model.md) — governs everything else: the template, the blocks, the slots, and the shape of what is issued.

Where this spec and either document conflict, the conflict is settled explicitly — the document is amended, or this spec is revised (§ Governance).

## Why

The system holds 164 normalised workouts and cannot yet answer the question it was built to answer: *what should I lift tonight?*

Everything built so far runs one way. Hevy is extracted into raw, raw is derived into a normalised gym workout, and there it stops. That chain is a record of the past with no output an operator can act on. The programme it serves is currently run by hand — loads worked out on paper, an anchor carried in the operator's head, a routine retyped into a phone each week — and the two arithmetic errors found in the August back-off loads are what running it by hand costs.

This feature closes the loop. It is the first capability in the repository that produces something rather than storing something, and the first time authored data (§ III) exists at all: until now every row in the store has been an observation or a derivation of one.

It is deliberately narrow. One programme, one primary lift, one template, printed to a terminal. It does not reconcile sources, does not decide whether a prescription was followed, and does not write to Hevy. Those are real problems and none of them stands between here and a workout on Monday night.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Issue the next prescribed workout (Priority: P1)

As the operator, I ask for the next session on a given date and receive a complete prescription — every block in fatigue order, every slot filled, every set carrying its load and its target — derived from the authored programme and from what I have actually been lifting, so that I can train from it without doing any arithmetic myself.

**Why this priority**: It is the feature's reason to exist and the only story that produces a workout. The other two make the workout *correct over time*; this one makes it exist. Built alone against a hand-set anchor it already replaces the paper.

**Independent Test**: With a programme and an anchor authored, ask for the session of 2026-08-17 and confirm the printed workout reproduces the structure of the fifteen sessions since 15 June — five blocks in order, the strength block's four patterns, the upper pair supersetted and the lower pair not, the hypertrophy block's two supersets and single core slot — with the primary's loads derived from the anchor and every other slot's derived from its own last performance.

**Acceptance Scenarios**:

1. **A workout is issued.** **Given** an authored programme with its slot fills and an anchor, **When** the operator asks for the next session on a date, **Then** one prescribed workout is produced, its items in fatigue order, each set carrying a load and a target.
2. **The primary draws from programme state.** **Given** an anchor and a ladder position, **When** the primary's sets are derived, **Then** its warm-up ramp, top set and back-off sets are all functions of the anchor and the session role, and of nothing read from the performed record.
3. **The back-off follows the top set.** **Given** a prescribed top set, **When** the back-off load is derived, **Then** it is the authored percentage of that top set — not of the anchor — quantised to the plate grid.
4. **An off-grid load quantises to the nearest increment.** **Given** a top set of 80kg and a back-off percentage of 85, **When** the back-off load is derived, **Then** it is 67.5kg: 68 is nearer 67.5 than 70. **And given** a derived load falling exactly halfway between two increments, **Then** it resolves to the lower.
5. **Every other slot draws from observed history.** **Given** a non-primary slot whose exercise was last performed at some earlier session, **When** that slot's sets are derived, **Then** they are a function of that last performance under double progression, whenever it occurred.
6. **History reaches back past the previous session.** **Given** a slot whose fill alternates between sessions — Nordic curls one day, back extension the other — **When** the slot is derived, **Then** it reads the last performance *of the exercise being prescribed*, which is two sessions back, not the last session.
7. **A slot with no history is prescribable.** **Given** a slot filled with an exercise that has never been performed, **When** the slot is derived, **Then** the prescription comes from the authored programme rather than failing, and says so.
8. **The issued workout is stored.** **Given** a workout has been issued, **When** the store is read, **Then** the prescription is there in full and durably (§ 12), because expectation must remain recoverable against reality (§ 11).
9. **Prescription never becomes observation.** **Given** an issued prescribed workout, **When** any query about what was performed is answered, **Then** the prescription contributes nothing to it (§ 11).

---

### User Story 2 - A failed attempt is recorded, not refused (Priority: P2)

As the operator, when I load a bar and fail the lift, that failure is part of my training record — distinguishable both from a set I completed and from a session I never did — so that the programme can tell a lift I could not make from a night I did not train.

**Why this priority**: It is a precondition of Story 3 and it is worth having on its own. The domain currently refuses a zero-rep set as unmodelled, so the corpus's single genuine failed attempt — 95kg front squat, 2026-07-03 — sits in `normalisation_refusal` rather than in a workout, and the test that established the anchor is therefore only half recorded.

**Independent Test**: Re-derive the normalised layer over the landed corpus and confirm the 95kg attempt appears as a failed attempt against the front squat of 2026-07-03, that refusals fall from three to two, and that the attempt contributes nothing to any volume total or maximum estimate.

**Acceptance Scenarios**:

1. **Zero reps is a failed attempt.** **Given** a landed set recording zero repetitions, **When** the normalised layer is derived, **Then** it becomes a failed attempt at that load against that exercise, not a refusal and not a set of zero reps.
2. **The set type is not the discriminator.** **Given** a landed set typed `failure` with a non-zero rep count, **When** derivation runs, **Then** it is an ordinary completed working set taken to zero in reserve. The corpus holds 77 such sets and exactly one of them is a genuine failure; keying on the type would misfile 76.
3. **A failure is not a quantity.** **Given** a failed attempt in the record, **When** any volume total, repetition count or maximum estimate is computed, **Then** the attempt contributes nothing to it — a failure is evidence about a load, not a number of repetitions.
4. **A failed attempt is not an absence.** **Given** one session where the primary's top set failed and another where no session occurred at all, **When** the two are read, **Then** they are distinguishable.
5. **Re-derivation is unaffected.** **Given** the change, **When** the normalised layer is derived twice over unchanged raw, **Then** the results are identical (§ 7).

---

### User Story 3 - A block is planned, and failure is handled (Priority: P3)

As the operator, I give the generator a number of weeks and my current 1RM and get back a block whose loads climb to a stated endpoint and finish in a test — and when the plan turns out to have been too ambitious, it drops back and re-climbs rather than stopping.

**Why this priority**: Without it Story 1 issues the same numbers every week, which is a working prescription and not a programme. It is last because it is the only story that needs Story 2 finished first, and because a hand-set ladder is a tolerable stopgap where a hand-computed workout is not.

**The plan and the failure mechanism are separate**, and the scenarios below are grouped accordingly. Scenarios 1 to 4 are the plan: a deterministic ladder from two inputs. Scenarios 5 to 8 are what happens when it fails. Neither is derived from the other.

**Independent Test**: Generate an 8-week block from a 90kg anchor and confirm the heavy top sets climb to the authored endpoint with the final week a test; then drive the same programme through the worked example in `primary-lift-progression.md` — 90kg missed twice, dropped to 80 at +5kg weekly, missed again at 90, dropped to 80 at +2.5kg weekly — and confirm the prescribed load each week matches the eleven-week table exactly.

**Acceptance Scenarios**:

1. **Two inputs generate the block.** **Given** a duration in weeks and a starting 1RM, **When** the block is generated, **Then** every week's primary loading follows, and the final week is a test.
2. **The anchor does not move within a block.** **Given** any sequence of completed sessions, **When** each week is issued, **Then** the anchor is the same value throughout. What changes is the ladder's percentage.
3. **The step is derived from the endpoint.** **Given** an authored start and end percentage and a duration, **When** the ladder is built, **Then** the weekly step is the span divided by the climbing weeks — and changing the duration changes the step, not the endpoint.
4. **Nothing performed climbs it faster.** **Given** a top set completed easily, with any effort report attached, **When** the next week is issued, **Then** the load is exactly what the ladder says. No effort report is consulted by any derivation.
5. **A miss holds the ladder.** **Given** a gating session whose top set was a failed attempt, **When** the next week is issued, **Then** the same loads are re-issued and the ladder has not advanced.
6. **A second miss at the same load suspends the ladder.** **Given** a second failed attempt at a load already failed, **When** the next week is issued, **Then** the load is the first reset's drop from the failed load, and the anchor is unchanged.
7. **A completed re-climb resumes the ladder.** **Given** a reset whose re-climb has reached the load that was failed, **When** the next week is issued, **Then** the ladder resumes from the position it was suspended at.
8. **The second stall is the slower reset.** **Given** a stall while the first reset is in play, **When** the next week is issued, **Then** the second reset's drop and rate apply.
9. **A test replaces the anchor for the next block.** **Given** a completed test, **When** the following block is generated, **Then** its anchor is the tested value, whether that is above or below the ladder's endpoint.
10. **Only the gating role gates.** **Given** a non-gating session in the same week, **When** its top set is missed, **Then** the ladder is unaffected.

---

### Edge Cases

- **A session was never trained.** Absence is not a miss: the ladder holds, no stall accrues, and the same week re-issues. Distinguishing the two is what Story 2 delivers.
- **A block ends mid-re-climb.** The test runs regardless. A reset costs four of the block's 7 or 11 climbing weeks, so a ladder leaving no room for one reset cannot survive a stall inside its own block — a real constraint on how ambitious an endpoint can be.
- **The ladder's endpoint is below where the lifter already is.** Permitted and sometimes correct: a block anchored on a recent test may deliberately spend early weeks below it. It is not the generator's business to refuse an unambitious plan.
- **A back-off percentage lands off the plate grid.** 85% of an 80kg top set is 68kg, which quantises to 67.5. An exact tie — a derived load landing halfway between two increments — resolves downward, so 68.75 quantises to 67.5 rather than 70.
- **The programme has never issued anything.** The first workout has no previous prescription to advance from; it derives from the authored entry anchor.
- **A slot's exercise has no performed history and no authored starting point.** The prescription cannot be derived and must say so rather than inventing a load.
- **Two sessions fall on one date, or a session is asked for twice.** Asking for the same date twice must not double-advance anything.
- **The performed record is stale.** A prescription derived from history that has not been extracted since the last session is wrong in a way the operator should be able to see (§ 38).
- **The primary's exercise is changed mid-programme.** Out of scope: the primary exercise is a programme input, and changing it is a new programme.
- **A performance diverges from what was prescribed.** Exercises swapped at the gym, an order changed, sets abandoned. The projection describes what happened; it does not judge it, and it does not claim to recover the prescription that motivated it. Comparing a projection against a generated prescription reports divergences and asserts nothing about which is correct.
- **A projection of a failed attempt.** The load is known and the intended repetitions are not, because the performed record does not carry them. The gap is recorded, not filled.

## Requirements *(mandatory)*

### Functional Requirements

**Generation**

- **FR-001**: The system MUST produce a complete prescribed workout for a named date on operator request.
- **FR-002**: The issued workout MUST carry its blocks in fatigue order — plyometric, power, strength, hypertrophy, mobility — with the strength block's four movement patterns present, the upper pair supersetted and the lower pair not, and the hypertrophy block's two supersets followed by its single unsupersetted core slot.
- **FR-003**: Every prescribed set MUST pin at least one of load, measure or effort. A set prescribing nothing MUST be unconstructible.
- **FR-004**: The primary slot's sets MUST derive from the anchor, that week's ladder position and the session role, and MUST NOT read the performed record.
- **FR-005**: The primary's top set MUST be prescribed as an exact repetition count, not a range.
- **FR-006**: The back-off load MUST be the authored percentage of the prescribed top set, quantised to the nearest multiple of the authored plate increment, with an exact tie resolving downward.
- **FR-007**: Every non-primary strength and hypertrophy slot MUST derive from the most recent performance of the exercise being prescribed, however far back that is, under double progression.
- **FR-008**: Plyometric, power and mobility slots MUST be issued without progression.
- **FR-009**: Each issued item MUST be tagged with the slot it fills, so that the same slot is comparable across cycles.
- **FR-010**: Asking for the same date more than once MUST NOT advance programme state more than once.
- **FR-011**: Where a slot cannot be derived, the system MUST report which slot and why, and MUST NOT substitute a guessed load.

**The plan**

- **FR-012**: The system MUST generate a block's whole primary loading series from two inputs — a duration in weeks and a starting 1RM — plus the authored ladder endpoint.
- **FR-013**: The anchor MUST be constant for the duration of a block. No performed value may change it, and effort reports MUST NOT be an input to any derivation.
- **FR-014**: The final week of a block MUST be a test. The ladder's climbing weeks are therefore one fewer than the duration.
- **FR-015**: The ladder's weekly step MUST be derived from the authored start and end percentages and the number of climbing weeks, not authored directly.
- **FR-016**: Each week's heavy top set MUST be the anchor scaled by that week's ladder percentage; the light session's top set MUST be a percentage of that week's heavy top set.
- **FR-017**: A recorded test MUST replace the anchor for the following block, whether the tested value is above or below the ladder's endpoint.

**Failure handling**

- **FR-018**: A failed top set MUST hold the ladder and cause the same loads to be re-issued.
- **FR-019**: A second failure at a load already failed MUST suspend the ladder and begin a reset: the authored drop taken from the failed load, re-climbing at that reset's rate.
- **FR-020**: When a reset's re-climb reaches the load that was failed, the ladder MUST resume from the position it was suspended at.
- **FR-021**: A reset MUST NOT alter the anchor. A stall is evidence that the plan was too ambitious, not evidence about where the block started.
- **FR-022**: Only the programme's gating session role may trigger a hold or a reset.

**Authored data**

- **FR-023**: The system MUST store generation parameters (§ 14): warm-up ramp percentages and repetitions, back-off percentage, top-set repetitions per session role, the ladder's start and end percentages, the light session's percentage of the heavy top set, plate increment, and the reset drops and re-climb rates.
- **FR-024**: The system MUST store the programme: its duration, primary exercise, slot fills including alternating variations, gating session role, and entry anchor.
- **FR-025**: The system MUST store every issued prescribed workout durably and in full, with the date it was issued for (§ 12).
- **FR-026**: Authored data MUST NOT pass through a raw or normalised layer, and MUST NOT be derivable from or overwritten by any observation (§ III).
- **FR-027**: Prescribed data MUST NOT satisfy any query about what was performed, and MUST NOT feed any series claiming to measure (§ 11).

**The performed record**

- **FR-028**: A landed set of zero repetitions MUST normalise to a failed attempt at its load, not to a refusal.
- **FR-029**: A failed attempt MUST contribute nothing to any volume total, repetition count or maximum estimate.
- **FR-030**: A failed attempt MUST be distinguishable from a completed set and from an absent session.
- **FR-031**: The source's `failure` set type MUST continue to mean a set taken to zero in reserve, and MUST NOT be read as a failed attempt.
- **FR-032**: The system MUST answer, for a given exercise, what its most recent performance was, without bound on how far back it looks.

**Projection of a performance into a prescription shape**

- **FR-033**: The system MUST be able to project any performed workout into the shape of a prescription — ordered items, groupings, slots where known, and each set's load and target.
- **FR-034**: A projected shape MUST NOT be storable as an issued prescription, and MUST NOT be readable as one. The projection describes what was done in prescription's vocabulary; it is not a claim that anything was prescribed.
- **FR-035**: Where a performance carries something a prescription cannot express, or omits something a prescription requires, the projection MUST record the gap rather than inventing a value. A failed attempt is the known case: it carries the load attempted and not the repetitions intended.

### Key Entities

- **Generation parameters**: The values consulted when authoring a prescription — percentages, repetition counts, increments, reset rates. Only the current value is required (§ 14), because what they produced is recorded concretely in the issued workout.
- **Programme**: A rule for generating a series of prescribed workouts, plus its authored inputs — duration, primary exercise, slot fills, gating role and entry anchor. Its purpose is to increase the primary exercise's maximum.
- **Anchor**: The starting 1RM every primary load derives from, carrying its provenance — measured by test, derived, or asserted. **Constant for the block's duration.** Replaced only by a test, which ends a block. A stall does not touch it.
- **Ladder**: The block's plan — a percentage of the anchor per climbing week, running from an authored start to an authored endpoint, with the weekly step derived from the span and the duration. The last week of a block is a test rather than a ladder position.
- **Prescribed workout**: The concrete issued prescription for one date. An ordered sequence of items with grouping, each item slot-tagged, each set pinning at least one axis. The only prescribed entity stored.
- **Prescribed set**: One instruction — a load, a target measure, an optional effort guide, and an optional rest instruction. Distinct in shape from a performed set: prescribed rest is an instruction whose absence means none was given, where performed rest is an observation whose absence means none was recorded.
- **Workout shape**: The instructional content of a session — its items, groupings and sets — separated from the facts that make a prescription *issued*: the date, the anchor, the parameters and the programme. A generated prescription is a shape plus those facts. A projection of a performance is a shape and nothing else, which is what makes it unstorable as a prescription.
- **Failed attempt**: A load that was attempted and not completed. A performed-side fact, distinct from a set and from an absence, and never a quantity.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The operator obtains a complete, trainable prescription for a named date in a single request, with no manual arithmetic.
- **SC-002**: Running generation against the programme in force reproduces the structure of every one of the fifteen sessions trained since 15 June 2026 — same blocks, same order, same grouping, same slots.
- **SC-003**: Regenerating a past session from the anchor and history in force at that date reproduces the loads actually prescribed on that date, for every session from 7 August 2026 onward. Sessions before that date are excluded knowingly: the back-off loads of 13 July, 20 July and 3 August were computed wrongly by hand and the correct rule does not reproduce them.
- **SC-004**: The two back-off errors visible in the corpus cannot recur, because no prescribed load is arrived at by hand.
- **SC-005**: The eleven-week worked example in `primary-lift-progression.md` is reproduced exactly, load for load.
- **SC-011**: A block generated from a duration and a starting 1RM alone — no other operator input beyond the ladder endpoint — produces a complete primary loading series for every week, ending in a test.
- **SC-006**: The failed 95kg attempt of 2026-07-03 appears in the training record as a failed attempt, and refusals over the landed corpus fall from three to two.
- **SC-007**: No total, count or maximum estimate anywhere in the system changes as a result of that attempt becoming visible.
- **SC-008**: Discarding all generated output and regenerating from the stored authored data reproduces it identically.
- **SC-009**: An operator reading a prescription issued six months earlier can see every value it was derived from without consulting anything outside the stored record.
- **SC-010**: Every one of the fifteen sessions trained since 15 June 2026 can be projected into a prescription shape, and each projection can be compared against what generation produces for that date — turning SC-002 and SC-003 from a reading of printed output into a comparison of two values. Where the two differ, the difference is reported as a list of specific divergences rather than a pass or fail.

## Assumptions

- **The analytical layer is a passthrough of the normalised layer.** One source is in use, so there is nothing to reconcile and no canonical layer is built here. This is a deliberate deferral and not a claim that canonicalisation is unnecessary; a second source makes it necessary.
- **The operator names the date.** Generation is invoked by hand rather than triggered by a performance arriving, so no correspondence between prescription and performance is required. The session role is derived from the date's position in the programme's cycle.
- **The prescription is printed.** No routine is written back to Hevy. How a prescription reaches the phone is left to the operator, and routine proliferation stays an open question in the domain model.
- **Movement pattern and exercise family are not modelled.** Slot fills are authored inputs, so nothing needs to validate that a fill matches its slot's pattern. This keeps the performed model's open question 4 off the critical path.
- **Step-back on accumulated absence is not built.** It is the one operation requiring absence to be measured, and measuring absence requires the correspondence this feature defers.
- **One programme is in force at a time**, against one primary lift.
- **The anchor is known: 90kg, tested 2026-07-03.** It is the one measurement of the primary's 1RM in the record — a completed single with a failed 95 above it. The block's remaining unknown is the ladder's start and end percentages, which are a claim about achievable gain rather than anything derivable from the record. This feature does not attempt to infer them.
- **No e1RM is read off a submaximal set.** A set left with repetitions in reserve says nothing about a maximum. Only a set taken to failure or a genuine single supports an estimate, which in this record means the 3 July test and the 28 April 2025 triple at zero in reserve.
- **Repetitions are constant per session role within a block** — one on the heavy session, three on the light one, as the record has run since July. Descending repetitions across the block is the textbook linear variant and is deferred, not rejected.
- **The performed record is extracted and normalised before generation runs.** Staleness is observable (§ 38) but this feature does not extract on the operator's behalf.

## Out of Scope

- The canonical layer, and any reconciliation across sources.
- Correspondence between an issued prescription and the performance that satisfied it.
- Writing routines back to Hevy or any other source.
- The movement-pattern and exercise-family relations.
- Step-back on accumulated absence.
- Cycling, nutrition phases and the constraint calendar.
- Choosing slot fills. Fills are programme inputs; generation produces the loading series, not the exercise selection.
- Any second driving adapter. The web adapter reaching parity with the CLI is a separate feature.

## Questions resolved

### Question 1: Plate quantisation — resolved 2026-08-16

**The question**: FR-006 requires the back-off load to be the authored percentage of the top set, quantised to the plate grid. When a derived load falls between two plate increments, which way does it go? 85% of an 80kg top set is 68kg, off the 2.5kg grid, and an 80kg top set is the next light session's prescription. The corpus could not settle it, because every back-off in the validated window happened to land within 0.6kg of the grid.

**Resolved**: **Nearest, ties down.** A derived load quantises to the closest multiple of the plate increment; a load falling exactly halfway resolves to the lower.

**What follows**: 68 → 67.5, so the next light session's back-off is 67.5kg. Quantisation is a total function of a load and an increment, so it applies wherever a derived load meets the grid — back-offs, warm-up ramp steps and reset drops alike — rather than being a back-off rule. A reset of −10% from 87.5 is 78.75, exactly halfway between 77.5 and 80, and resolves to 77.5.

Rejected: *always up*, which can overshoot the intended percentage by up to 3% on a 2.5kg grid, and which would make a reset drop land higher than the reset intends. *Always down* differs from the chosen rule only on exact ties and was the weaker statement of the same preference.
