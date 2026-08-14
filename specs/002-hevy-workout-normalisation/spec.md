# Feature Specification: Hevy workout normalisation

**Feature Branch**: `002-hevy-workout-normalisation`

**Created**: 2026-08-14

**Status**: Ready for planning. The three questions below are resolved; see "Questions resolved".

**Scope**: The normalised layer for the `hevy.workouts` stream (§ II.3). No canonicalisation, no analysis, no overlay.

**Model of record**: [`docs/gym-workout-domain-model.md`](../../docs/gym-workout-domain-model.md). It defines the gym-workout entity, the reasoning behind every type, and the counts a correct translation reproduces. Where this spec and that document conflict, the conflict is settled explicitly — the document is amended, or this spec is revised (§ Governance).

## Why

Raw holds 164 Hevy workout records and answers no training question. § II.3 is where the domain first exists as something other than prose: a workout of ordered items, exercises, sets, load, measure and intensity, in our vocabulary rather than the source's.

Nothing downstream can begin without it. The canonical layer is a function of this one, the analytical layer a function of that, and § 7's re-derivation chain has no second link until this link exists. The domain model has been written out as rules and run against every landed record on paper; this feature makes those rules executable and holds them to the figures that run produced.

It ends at the normalised layer. It produces no sessions, no correspondences and no metrics.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Normalise the landed history into gym workouts (Priority: P1)

As the operator, I derive the normalised layer from raw and every landed Hevy workout becomes a gym workout in our own vocabulary — ordered items, exercises, sets, load, measure, intensity and set kind — so that the canonical and analytical layers have something to be a function of.

**Why this priority**: It is the feature's reason to exist. The two stories below are refinements of what happens at its edges; neither is worth building against an empty normalised layer.

**Independent Test**: Derive over the 164 landed records and confirm the entity counts — 3,755 of 3,779 sets, 1,122 performed exercises from 1,135 landed entries, 328 supersets from the 334 well-formed groupings — then derive again over unchanged raw and confirm the result is identical. Delivers the domain entity that every later layer reads.

**Acceptance Scenarios**:

1. **A workout translates.** **Given** a landed record holding a well-formed workout, **When** the normalised layer is derived, **Then** one gym workout results, its items in the order the record gave them, each exercise carrying its sets with load, measure, intensity where recorded, and set kind.
2. **Identity comes from the mapping, not the label.** **Given** two landed exercise entries with different exercise template identifiers that the mapping sends to one exercise — an assisted and an unassisted pull-up — **When** derivation runs, **Then** both resolve to the same exercise, the assisted load negative and the unassisted `Relative(0)`.
3. **A renamed template still resolves.** **Given** a template identifier that has appeared under two titles across the history, **When** derivation runs, **Then** every entry bearing that identifier resolves to the same exercise regardless of the title recorded alongside it.
4. **Re-derivation is identical.** **Given** a completed derivation, **When** the normalised layer is discarded and derived again from unchanged raw, **Then** the result is identical (§ 7).
5. **Wall clock survives.** **Given** a workout the source timestamps as a UTC instant, **When** derivation runs, **Then** the workout's time carries an IANA zone from operator configuration and reads back as the wall-clock time that was trained at, across both British Summer Time and Greenwich Mean Time.
6. **An unmapped identifier stops the run.** **Given** an exercise template identifier the mapping does not cover, **When** derivation runs, **Then** it fails and names the identifier. The vocabulary is code (§ 9), so a gap in it is a defect to fix, not data to record around.
7. **A source re-serves a workout.** **Given** two landing records sharing one source identifier, **When** derivation runs, **Then** two normalised workouts result and both stand. Which is current is § 10's question and is answered at the canonical layer.

---

### User Story 2 - Refusal is recorded, never guessed (Priority: P2)

As the operator, when a landed record says something the domain cannot express, the grammatical part translates, the part that cannot is left out, and the omission is recorded with enough detail to act on — so that a wrong record is visible and diagnosable rather than silently repaired into a plausible lie.

**Why this priority**: § 37 is not satisfied by translating what fits. 24 sets and 2 supersets in the corpus do not translate, and each is either data to fix at source, a limitation to declare, or a gap in the model — telling them apart is the whole value, and it is unavailable if the refusal is a stack trace or a dropped row.

**Independent Test**: Derive over the corpus and confirm the refusals are exactly the named ones — 7 zero loads on absolute-load exercises, 16 band-resistance sets, 1 zero-rep set, 1 non-contiguous superset, 1 single-member superset — each naming the record, the position within it and the reason. Delivers a diagnosable account of everything raw holds that the domain will not accept.

**Acceptance Scenarios**:

1. **Zero on an absolute-load exercise.** **Given** a set recording zero load on an exercise whose implement has mass, **When** derivation runs, **Then** the set does not translate, the omission is recorded against its exercise and position, and the rest of the workout translates. No bar mass is assumed and no default is applied — 10, 15 and 20 kg bars are all in use, so every repair is a guess.
2. **Band resistance.** **Given** a set whose resistance is a band, **When** derivation runs, **Then** the set does not translate and the omission records band resistance as a declared limitation rather than a data error.
3. **A malformed superset.** **Given** a grouping with members either side of a non-member, or with a single member, **When** derivation runs, **Then** the grouping does not translate, the omission is recorded, and its member exercises translate as ordinary items in their recorded order — the workout is not lost to a bad grouping.
4. **An unknown set kind.** **Given** a set carrying a kind the domain does not recognise, **When** derivation runs, **Then** the set does not translate rather than defaulting to a working set.
5. **Nothing is skipped silently.** **Given** the full landed corpus, **When** derivation runs, **Then** every landing record has exactly one of three outcomes: a normalised entity, a retraction it applied, or a recorded refusal naming it. There is no fourth.
6. **Absence is absence.** **Given** a set with no recorded intensity and no recorded rest, **When** derivation runs, **Then** intensity and rest are absent — not zero, not carried forward from a neighbouring set, and not reconstructed from a linked routine (§ 11, § 37).

---

### User Story 3 - A withdrawn workout is absent, not marked (Priority: P3)

As the operator, a landed deletion leaves the workout it names with no normalised entity — including the tombstone already in raw for a workout created and deleted between two extraction runs, which names a workout that was never landed and so withdraws nothing.

**Why this priority**: One record in 164, and the corpus offers no second case. But it is landed already, it cannot be ignored without violating "nothing is skipped silently", and a body-less record is the one input shape the ratified constitution did not obviously answer. It is why § II.3 now says what a retraction does.

**Independent Test**: Derive over the corpus and confirm the single `deleted` record produces no entity, fails nothing, and is accounted for. Then derive over a synthetic pair — an `updated` record and a later `deleted` record sharing one identifier — and confirm no workout results for that identifier, while every other record's workout is untouched.

**Acceptance Scenarios**:

1. **A deletion for a workout previously landed.** **Given** an `updated` record and a later `deleted` record sharing one source identifier, **When** derivation runs, **Then** no normalised workout exists for that identifier. The `updated` record stays in raw and nothing else in the derivation changes.
2. **A tombstone for a workout never landed.** **Given** a `deleted` record with no body and no prior `updated` record for that identifier, **When** derivation runs, **Then** it withdraws nothing, derivation does not fail, and the record is accounted for as a retraction rather than as a refusal.
3. **Order within raw does not matter.** **Given** the same two records, **When** derivation runs over them in either landed order, **Then** the result is the same absence. A retraction is not "the latest record wins" — it is not overridden by an `updated` record landed after it, because the source withdrawing a record and then serving it again is a re-creation the corpus has never shown and § 10 would resolve at the canonical layer.
4. **A retraction is not a refusal.** **Given** a derivation over the corpus, **When** the refusals are read back, **Then** the `deleted` record is not among them. Nothing about it was rejected.

### Edge Cases

- **An exercise entry with no sets.** The entry does not translate — an exercise holds a non-empty sequence of sets by construction (§ 24) — and the omission is recorded. The rest of the workout translates.
- **A workout with no translatable items.** A workout holds a non-empty sequence of items, so a record whose every item is refused yields no workout, and the refusal is recorded against the record. This is not a run failure.
- **Sled Push recording thirty seconds and a zero distance.** It is a duration exercise here regardless of the category the source declares (nine such sets). Where the source's category and ours differ, ours wins, and the mapping is where that is decided.
- **A set at RPE the source records but the domain's eight positions do not cover.** The source's scale is `6, 7, 7.5, 8, 8.5, 9, 9.5, 10` and the mapping is total across it. An RPE outside that set is unrecognised and refused, like an unknown set kind.
- **The same exercise appearing twice in one workout.** Two separate entries in the ordered sequence, not merged. Three workouts in the corpus do this; whether it encodes rounds is open question 10 and is not answered here.
- **A source field the domain does not model** — a workout title, a description, an exercise note, a `routine_id`. It is not carried into the normalised entity and its absence is not a refusal. Raw retains it (§ II.1) and it is available if a later feature needs it.
- **Derivation runs while extraction is running.** Derivation reads raw and writes only the normalised layer; it neither takes the extraction lock nor advances the resumption point. A record landed after derivation began is picked up by the next derivation.

## Requirements *(mandatory)*

### Functional Requirements

**Shape of the derivation**

- **FR-001**: One normalised gym workout MUST be a function of exactly one landing record — never of two, and never of another source (§ II.3).
- **FR-002**: Translation MUST be deterministic: the source's identifiers and recorded values, plus the declared mapping and the declared operator configuration, resolve the entity with no further input (§ 9).
- **FR-003**: Translation MUST NOT consult any overlay (§ 9). No edit overlay exists yet; the prohibition is binding regardless.
- **FR-004**: Deriving twice over unchanged raw MUST produce an identical result, and the normalised layer MUST be re-derivable from raw without contacting the source (§ 7). It is never mutated in place.
- **FR-005**: Every landing record in the stream MUST have exactly one of three outcomes: a normalised entity, a retraction it applied, or a recorded refusal naming it. No record is silently skipped, and none has two outcomes.

**What a normalised workout carries**

- **FR-006**: A workout MUST be a non-empty ordered sequence of items, each item either a single performed exercise or a superset, in the order the record gave them.
- **FR-007**: A superset MUST hold two or more contiguous members. A grouping that fails either condition does not translate.
- **FR-008**: A performed exercise MUST hold a non-empty sequence of sets, and its measure — repetitions, elapsed time, ground covered, or ground covered in a time — MUST be fixed by which exercise it is, so a set and its exercise cannot disagree (§ 24). Ground covered and ground covered in a time are separate measures, not one measure with an optional duration: a carry is time under load and a run is pace, and a series must not be able to average across them.
- **FR-009**: Every set MUST carry a load. Load is absolute where the implement has mass and zero is therefore impossible, and relative — a signed delta against an unrecorded bodyweight — where an unloaded version of the movement exists. Assistance is negative on the same axis as added weight; the source's separately-named assisted exercises are translated, not obeyed (§ 8).
- **FR-010**: A zero load on an absolute-load exercise MUST NOT translate. There is no "unrecorded" load and no assumed implement mass.
- **FR-011**: Intensity MUST be optional, and where present MUST be reps in reserve on eight ordered positions that support comparison but not arithmetic. The source records RPE; the mapping from its scale to those positions is total, and an unrecognised value is refused. An absent value is absent, not a default.
- **FR-012**: Set kind MUST be working or warm-up and nothing else. A source kind that is neither — the source's `failure` and `dropset` are both working sets to the only question asked of the field — is mapped explicitly, and an unrecognised kind does not translate.
- **FR-013**: Rest after a set MUST be optional and MUST NOT be reconstructed from a linked routine. This source records none, so it is permanently absent here (§ 11, § 37).
- **FR-014**: Load MUST carry sub-kilo precision without floating-point representation error, because the value is persisted, digested and compared against rows written by earlier versions (§ 7).
- **FR-015**: Units MUST be canonicalised — kilograms, metres, seconds (§ II.3).

**Identity and provenance**

- **FR-016**: Exercise identity MUST resolve through a declared, version-controlled mapping keyed on the source's exercise template identifier, many-to-one onto our exercises. Per identifier it resolves both the exercise and its load interpretation. Titles inform the mapping when it is authored and never key it, because a title is not stable across the history.
- **FR-017**: An exercise template identifier the mapping does not cover MUST fail derivation loudly, naming the identifier. No passthrough, no fallback exercise, no silent omission.
- **FR-018**: Every normalised entity MUST carry provenance: the source, whatever version or algorithm identifier the source exposes, and the identifier by which the source names the record it came from (§ II.3). Provenance records what the source told us; nothing about it is inferred.

**Time**

- **FR-019**: Every timestamp MUST carry an IANA timezone identifier and MUST never be naive. An offset is not a substitute.
- **FR-020**: Where the source supplies only a UTC instant, the zone MUST be taken from declared operator configuration — a versioned input to deterministic translation, not an inference about the source or about the data. Travel is invisible in the payload and is an edit-overlay correction, out of scope here.

**Refusal**

- **FR-021**: What cannot be expressed MUST be rejected rather than coerced. The grammatical part of a record translates, the ungrammatical part does not, and translation never guesses which repair was meant (§ 37).
- **FR-022**: A refusal MUST record what was refused, where within the record it sat, and why, in terms specific enough to act on without re-reading the payload.
- **FR-023**: Refusals MUST be queryable after a derivation, so what the domain will not accept is visible rather than surfacing only in a log.
- **FR-024**: A refusal within a record MUST NOT prevent the rest of that record translating, and MUST NOT stop derivation of other records. FR-017 is the one exception, and it is a defect in our code rather than in the data.

**Deletion**

- **FR-025**: A landed deletion MUST leave the workout it names with no normalised entity (§ II.3, retraction). It produces no entity of its own — not a tombstone, not a body-less workout — because the layer's account of a withdrawn workout is its absence.
- **FR-026**: A deletion naming an identifier no `updated` record was ever landed for MUST withdraw nothing and MUST NOT fail derivation. It is the case already in the corpus.
- **FR-027**: A retraction MUST NOT be recorded as a refusal. Nothing was rejected, and conflating the two would put a working source event in the operator's list of things to fix.
- **FR-028**: Retraction MUST survive re-derivation: deriving again over unchanged raw MUST reproduce the same absence, and MUST NOT depend on the order in which records are read.

**Reach**

- **FR-029**: The capability MUST be invocable through a port in the same terms whichever driving adapter calls it, so that reaching parity with a second adapter later requires no change to the capability. A capability only one transport can invoke has been built into that transport.

### Key Entities

- **Gym workout** — a non-empty ordered sequence of items, with its start time and its provenance. The normalised layer's entity for this stream.
- **Workout item** — one position in that sequence: either a performed exercise or a superset.
- **Superset** — two or more exercises performed back to back. Contiguity and a minimum of two members are what the container asserts.
- **Performed exercise** — one exercise together with the non-empty sequence of sets performed of it. The measure is fixed by which exercise it is.
- **Set** — one performed set: its load, its measure, its intensity where recorded, its kind, and the rest that followed it where recorded.
- **Load** — absolute where the implement has mass, relative where an unloaded version of the movement exists. Signed in the relative case, so assistance and added weight sit on one axis.
- **Measure** — repetitions, elapsed time, ground covered, or ground covered in a time. The exercise vocabulary is partitioned by it.
- **Intensity** — reps in reserve, on an ordinal scale of eight named positions.
- **Set kind** — working or warm-up.
- **Exercise mapping** — the declared, version-controlled correspondence from a source's exercise template identifier to one of our exercises and its load interpretation. Many-to-one. Code, not data.
- **Refusal** — the record of something a landing record asserted that the domain will not express: what, where, and why.
- **Retraction** — a source event withdrawing a record it previously served. It produces no entity and is not a refusal; its effect is that the workout it names has none.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Deriving over the 164 landed records translates **3,755 of 3,779 sets**, reproducing the model of record's figure. The air bike's 32 sets and the sled's 9 are among them, carrying `Relative(0)` under decision 0003.
- **SC-001a**: Of the corpus's 336 groupings, **334 are well-formed** — the model of record's figure — and **328 reach the output as supersets**. The six that do not lose members to the band-resistance refusals, and a superset with fewer than two members is not one. Four keep a single member, which translates as an ordinary item rather than being lost with the grouping.
- **SC-001b**: **1,122 of the 1,135 landed exercise entries** become a performed exercise. The thirteen that do not lost every set they had — twelve band-resistance entries, and one `Snatch-Grip Behind The Neck Press` whose only two sets both record a zero load on a barbell — and an exercise holds a non-empty sequence of sets by construction.
- **SC-002**: The sets that do not translate are exactly the named cases and no others — 7 zero loads on absolute-load exercises, 16 band-resistance sets, 1 zero-rep set — and the supersets that do not translate are exactly one non-contiguous and one single-member grouping. A refusal outside that set is a regression, and a case in that set silently translating is a worse one.
- **SC-003**: All 1,135 landed exercise entries **resolve through the mapping**, and all 134 distinct exercise template identifiers in the corpus are covered by it. Zero unmapped identifiers. Distinct from SC-001b: resolving is what the mapping does, and an entry can resolve and still translate to nothing because every one of its sets refused.
- **SC-004**: Deriving twice over unchanged raw produces equal normalised entities and an equal set of refusals, and discarding the normalised layer entirely and re-deriving restores it identically, with no request to the source.
- **SC-005**: Every one of the 164 landed records is accounted for by exactly one of a normalised entity, a retraction it applied, or a refusal naming it — none in two, none in none.
- **SC-006**: A workout the source stamps at 18:00 UTC in July and one stamped at 19:00 UTC in December both read back as the same local wall-clock hour, and no normalised timestamp lacks a zone.
- **SC-007**: Every refusal identifies the landing record, the position within it, and a reason that distinguishes wrong data from a declared limitation from an unmodelled case, without the operator re-reading the payload.
- **SC-008**: An exercise template identifier absent from the mapping fails derivation and names the identifier, and no workout containing it translates around the gap.
- **SC-009**: The assisted and unassisted forms of the same movement resolve to one exercise across the corpus — the 97 `Pull Up` and 159 `Pull Up (Assisted)` sets form one series, as do the 84 `Chest Dip` and 277 `Chest Dip (Assisted)` sets — with assistance carried as negative load.
- **SC-010**: Deriving over the corpus yields **163 gym workouts**, one per `updated` record. The `deleted` record names a workout never landed, so it withdraws nothing and the count is unreduced; it appears in no refusal. Over a synthetic corpus of the same 164 records plus an `updated` record for the deleted identifier, the count stays 163 — the workout the retraction names is the one absent.
- **SC-011**: A carry and a run cannot be compared by construction: no single measure holds both `Farmers Walk`'s 15 sets and `Running`'s 19, and no set carries a duration field that is absent for every set of its exercise.

## Assumptions

- **The domain model document is the model of record.** Its entity, its reasoning and its figures are the specification's substance. This spec restates the obligations; it does not re-derive the model, and it does not re-argue decisions the document settled.
- **The landed corpus is the fixture.** 164 records — 163 `updated`, 1 `deleted`, November 2024 to August 2026 — are already in the store and are not re-fetched. Integration tests at port boundaries are the primary suite (§ 29) and run against them.
- **An unmapped exercise template identifier is a defect in our code, not in the data.** The mapping is code (§ 9), so a gap in it means the vocabulary is incomplete. Failing the run is therefore right where a data error is recorded and stepped over. This is the reasoning behind FR-017 and FR-024's exception.
- **Exercise template metadata is consulted when the mapping is authored, not when translation runs.** The source's declared type is the only published carrier of the assisted/bodyweight/absolute sign convention, and it is invisible in a workout payload — so it informs the mapping, which then carries the load interpretation per identifier. Translation makes no request for it. This closes the open question 001 left for this feature.
- **A missed attempt is parked, not modelled.** The one zero-rep set — 95 kg × 0 reps at zero reps in reserve — is a real event and is not a set. It belongs with prescribed-versus-performed, which is out of scope, so it is refused and recorded as an unmodelled case rather than coerced into a set of zero reps. It is one of SC-002's 24.
- **Bands are a declared limitation.** No scalar is honest for band tension, nothing records the mechanism, and the account's assisted loads cannot be told from a machine stack deterministically. The 16 band sets are refused as a limitation, not as an error.
- **The operator's zone is a single declared value for the whole history.** The corpus shows a clean one-hour seasonal shift consistent with one zone throughout. Travel is invisible in the payload and is an edit-overlay correction, out of scope.
- **The exercise vocabulary is only as large as the corpus requires.** 134 templates map onto the exercises they need; the vocabulary is not designed ahead of evidence, and the enums in the model document are explicitly illustrative.
- **Supersession is untestable against real data and is not this feature's concern.** 164 records carry 164 distinct workout identifiers, with no re-serve. FR-001 and the acceptance scenario for a re-served workout are exercised synthetically.
- **Derivation is invoked, not self-triggering.** Nothing here schedules itself, and freshness policy is out of scope as it was for extraction.

## Out of Scope

The edit overlay, in both directions — nothing is applied and nothing is recorded. The canonical layer entirely: matching, correspondence, supersession, and the Session that sits above the workout, including the corpus's 136 sessions from 163 records. The analytical layer and every metric — e1RM, relative strength, volume, frequency, streaks. Prescription, routines, and anything the source's `routine_id` names. The grouping layer over exercises. Every other source, and every other Hevy stream. Write-back to Hevy. Refetching exercise templates at translation time.

## Open Questions

Carried from the model of record. None blocks this feature; each is named so that a later feature does not have to rediscover it.

- **Overlay anchors below the workout** (open question 6). The source publishes no identity below the workout, so an overlay anchored to source identity is unsatisfiable as § II.2 words it. An anchor derived from content survives a rebuild, which is that rule's stated purpose. Settled when the overlay is built; this feature should avoid foreclosing it.
- **Block-level results** (open question 10). Three workouts repeat an exercise entry, most likely rounds encoded without supersets. Blocked on a second source.
- **Per-limb load** (open question 9) and **machine identity** (open question 8). Both resolved by naming or declared as limitations; neither changes what translates.
- **Is a day a container?** (open question 1). Bears on relative strength, which is analytical and out of scope.


---

## Questions resolved

The three markers the draft carried, settled 2026-08-14. Each is recorded in
`docs/decisions/` because more than one option was legitimately available, and
each changed a document that was already written down.

### Q1: What does a deletion normalise to?

**Answer**: nothing — and the workout it names has no normalised entity either.
A withdrawn workout is not something the source is still saying, so the layer's
account of it is its absence rather than a tombstone standing in for it.

This changed the constitution. The per-record rule in § II.3 reads, literally,
as forbidding it; the reading is wrong, because that rule is about *composition*
— never build one entity out of several records, since reconciling accounts is
the canonical layer's work — and a retraction composes nothing. A source's
`updated` and `deleted` records for one workout are versions of one entity, not
two entities. § II.3 and § 10 now say so. Version 1.0.0 → 1.0.1, PATCH: the
letter changed, the spirit is what got written down.

Reflected in FR-025 to FR-028, user story 3, SC-005 and SC-010.
[Decision 0001](../../docs/decisions/0001-retraction-at-the-normalised-layer.md).

### Q2: The air bike and the sled — 41 sets of unrecorded resistance

**Answer**: they translate, carrying `Relative(0)`, and that this load is not a
measurement of what was moved is declared rather than expressed by the value.
SC-001's 3,755 stands as written.

The cost is real and is accepted knowingly: `Relative(0)` means "plain
bodyweight" for every other exercise, so for these two it says something false.
It is bounded in code — the mapping is the only place an exercise's load
interpretation is decided, so the affected exercises are enumerable by reading
it — and the alternatives cost more, either a distinction in the vocabulary paid
for by two exercises out of 134, or 41 otherwise well-recorded sets refused to
protect a field nothing yet asks a question of.

[Decision 0003](../../docs/decisions/0003-unrecorded-resistance-translates-as-relative-zero.md),
which also says when to revisit it.

### Q3: Does `Distance` carry an optional duration?

**Answer**: no. Ground covered and ground covered in a time are separate
measures, so the vocabulary partitions four ways rather than three.

The corpus decided it: all 19 `Running` sets carry a duration and none of the 41
carry sets does, so there is no exercise the evidence leaves ambiguous — which
was the stated cost of splitting. And an always-absent option would have meant
"not captured" for a run and "does not apply" for a carry with nothing in the
type to tell them apart, which is the merge that got the `Unrecorded` load
removed, one field over.

The model of record declared the optional duration and has been amended.
Reflected in FR-008, the Measure entity and SC-011.
[Decision 0002](../../docs/decisions/0002-distance-and-distance-over-time-are-different-measures.md).
