# Feature Specification: Hevy workout extraction

**Feature Branch**: `001-hevy-workout-extraction`

**Created**: 2026-08-11

**Status**: Draft

**Scope**: Extract and Load only. No normalisation, canonicalisation or analysis.

## Why

Every derivation the platform will perform depends on observation data being landed and permanently re-derivable from. Hevy is the first source. Until performed strength workouts are in raw, nothing downstream can be built or tested against real data.

This feature ends at the raw landing area. It produces no domain entities and answers no training question.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Land the full Hevy history (Priority: P1)

As the operator, I run extraction and every workout Hevy holds is landed in raw, so that derivations can be built and rebuilt indefinitely without contacting Hevy again.

**Why this priority**: It is the only story. The feature is a single capability — landing one source's observations — and it does not decompose into independently valuable slices: a partial history in raw satisfies no downstream need, because § 7 requires the whole chain to re-derive from what has been landed. Splitting it would produce fragments that deliver value only once all are present, which is the opposite of what prioritisation is for.

**Independent Test**: Run extraction against an account of known size and confirm the landed workout count matches what the source independently reports (SC-001), then run it again and confirm no records are added (SC-002). Delivers the guarantee that raw is a complete and stable substrate for everything built after it.

**Acceptance Scenarios**:

1. **First run.** **Given** no extraction has run before, **When** extraction runs to completion, **Then** every workout in the account is landed.
2. **Repeat run, nothing changed.** **Given** a completed extraction and no subsequent change in Hevy, **When** extraction runs again, **Then** no new landing records are created.
3. **Workout edited in Hevy.** **Given** a workout was previously landed, **When** it is edited in Hevy and extraction runs, **Then** a new landing record is created for it and the earlier record remains unchanged and retrievable.
4. **Workout deleted in Hevy.** **Given** a workout was previously landed, **When** it is deleted in Hevy and extraction runs, **Then** a landing record is created recording the deletion, and no existing record is removed or altered.
5. **Interrupted run.** **Given** extraction fails partway through, **When** it is run again, **Then** no workout that would have been collected by the failed run is missed.
6. **Operator-requested full re-fetch.** **Given** a completed extraction, **When** the operator resets the resumption point and extraction runs, **Then** landing records are created only for workouts whose payload now differs from the most recent record for that workout.
7. **Source unavailable.** **Given** Hevy is unreachable, **When** extraction runs, **Then** it fails visibly, raw is unchanged, and capabilities not depending on Hevy continue to work.

### Edge Cases

Expected behaviour below is a reasoned default, not a source-confirmed fact; each is testable as written.

- **A delete event arrives for a workout that was never landed.** The deletion is landed anyway. Raw records what the source asserted, and suppressing it would require raw to consult its own history — an interpretation FR-002 forbids. A deletion standing alone is a fact about the source, and is resolved at the canonical layer, not here.
- **A workout is edited more than once between two extraction runs.** Every distinct payload the source serves is landed, in the order served. If the source collapses repeat edits into a single current state, one record results and no edit is lost that the source still holds — raw cannot land what it was never served.
- **The account contains zero workouts.** The run completes successfully, lands nothing, and advances the resumption point. An empty account is not a failure and must not be reported as one.
- **Extraction is run twice concurrently.** The second invocation fails fast rather than proceeding (FR-010). Two runs sharing one resumption point can advance it past records neither has landed, which breaks FR-006 silently.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: One landing record MUST correspond to one workout as served by the source. Pagination boundaries are an artefact of the request and are not preserved.
- **FR-002**: A landing record MUST store the payload as received. No field is parsed, validated, renamed, defaulted or interpreted.
- **FR-003**: Every landing record MUST carry provenance sufficient to answer what it came from and when: source, endpoint, the time of the fetch that produced it, the source's own identifier for the workout, and the kind of event that produced it.
- **FR-004**: Deletion asserted by the source MUST be recorded as a landing record. Nothing in raw is ever removed, mutated or compacted.
- **FR-005**: Re-running extraction over unchanged source data MUST add no landing records.
- **FR-006**: Extraction MUST resume rather than restart. The resumption point advances only when a run has collected everything available to it; a run that fails partway leaves it unchanged.
- **FR-007**: The resumption point is reconstructible state, not an input (§ II). Losing it costs a re-fetch, never a fact. The operator MUST be able to reset it to collect the full history again.
- **FR-008**: The most recent successful extraction MUST be queryable, so a silently broken extraction is visible (§ 38).
- **FR-009**: Credentials MUST be supplied by environment or local configuration and never committed (§ 35).
- **FR-010**: Only one extraction run MUST be in progress at a time. A second concurrent invocation fails without landing records and without advancing the resumption point.
- **FR-011**: A failed run MUST be distinguishable from a run that completed having found nothing. FR-008 answers when extraction last succeeded; a run that lands nothing because the source held nothing new is a success, not silence.

### Key Entities

- **Landing record** — one workout payload as the source served it, plus its provenance. Immutable.
- **Extraction run** — one invocation: when it started, what it collected, whether it completed.
- **Resumption point** — the position extraction continues from. Reconstructible; not a system of record.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After a first full extraction, the number of distinct workouts landed equals the count the source independently reports for the account.
- **SC-002**: Running extraction twice in succession with no intervening change in Hevy produces the same number of landing records as running it once.
- **SC-003**: Every workout ever landed remains retrievable in its original form after any number of subsequent extractions.
- **SC-004**: A run interrupted at any point, followed by a successful run, lands the same set of workouts as a single uninterrupted run.
- **SC-005**: Rebuilding every derivation downstream of raw requires no request to Hevy.

## Assumptions

- **The source serves a workout events feed that reconstructs full history.** Requested from the epoch, it is assumed to surface a workout's creation as an update carrying its full body. Inferred from the feed's default `since` value and its stated purpose, not from documentation. See Open Questions.
- **A workout has a stable source identifier across edits.** FR-005 and acceptance scenario 3 both depend on it: without it, an edit is indistinguishable from a new workout and supersession cannot be detected downstream (§ 10).
- **Change detection compares payloads, not timestamps.** Scenario 6 requires deciding whether a re-fetched workout differs from what was last landed; the comparison is against the stored payload, so a source that re-serves an identical body after a reset adds nothing.
- **The account is a single operator's own account** (§ I). No multi-account or delegated-access handling.
- **Extraction is invoked manually or by external scheduling.** Nothing in this feature triggers itself.
- **Volume is small.** A full history is a few dozen requests and a few thousand workouts; no bulk-loading or streaming strategy is assumed necessary.

## Out of Scope

Normalisation, canonicalisation and analysis. Exercise templates, routines, routine folders and exercise history. Webhook subscriptions. Write-back to Hevy. Scheduling and freshness policy — extraction is invoked, not self-triggering. Any other source.

## Open Questions

- **Completeness of the events feed.** The design assumes the workout events feed, requested from the epoch, reconstructs the entire history — that a workout's creation surfaces as an update carrying its full body. This is inferred from the feed's default `since` value and its stated purpose, not from documentation. SC-001 is the guard: if the assumption is wrong, acceptance fails rather than data being silently lost. [NEEDS CLARIFICATION: confirm against the live account before implementation]
- **Rate limits.** Page size is capped low enough that a full history is a few dozen requests. Whether the source throttles, and how it signals throttling, is unknown. [NEEDS CLARIFICATION: confirm against the live account or documentation before implementation]
- **Exercise type metadata.** Whether an exercise is weight-and-reps, bodyweight, duration or distance is carried by the exercise template, not the workout. Translation will likely need it. It is refetchable and out of scope here, but it is a dependency the normalisation feature inherits.
