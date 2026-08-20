# Specification Quality Checklist: Prescribed workout generation

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-16
**Feature**: [spec.md](../spec.md)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs)
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## Notes

**All items pass.** One question was raised and resolved before planning — plate
quantisation, bearing on FR-006 — and is recorded in "Questions resolved" rather
than as an inline `[NEEDS CLARIFICATION]` marker, following the convention
established in `002-hevy-workout-normalisation`.

**Revalidated after the progression model changed during planning.** The primary's
mechanism moved from a climbing anchor to a fixed anchor with a climbing percentage
ladder, which rewrote FR-012 to FR-022, split User Story 3 into a plan and a failure
mechanism, renumbered FR-018 to FR-030 as FR-023 to FR-035, and added SC-011. The
checklist was re-run against the revised spec rather than carried forward:

- **Requirements testable and unambiguous** — the plan half of User Story 3 is now
  testable as a pure function of two inputs, which it was not when the anchor
  depended on the performed record.
- **Scope clearly bounded** — descending repetitions across a block is newly named
  as deferred rather than left implicit.
- **Success criteria measurable** — SC-011 was added because "two inputs generate
  the block" is the requirement the operator actually stated and nothing asserted
  it.

One item that would have failed before the revision and does not now: an earlier
draft carried a fitted anchor value inferred from four sessions, which is not a
measurable criterion but a guess wearing one. It is gone.

The answer is nearest, ties down. FR-006 and edge case 2 now state it, and US1
scenario 4 tests it against the concrete case that raised it: an 80kg top set
back-offs to 67.5. The rule generalised past the back-off during resolution —
quantisation is a function of a load and an increment, so it also governs
warm-up steps and reset drops.

**On "no implementation details" and "non-technical stakeholders".** Both pass
against this repository's convention rather than against a general reading. The
spec uses constitutional vocabulary — raw, normalised, layer, adapter, slot,
anchor — because the constitution defines those terms and the operator is also
the architect. It names no language, framework, library, schema or API. The
single mention of the CLI and web adapters appears in Out of Scope, where the
point being made is which driving adapters this feature does *not* deliver.

**On SC-002, SC-003 and SC-012.** These were revised after the operator pointed out
that the round trip is a property of the model going forward, not of the corpus. An
earlier draft had SC-002 and SC-003 asserting that generation reproduces the
fifteen sessions since 15 June — which would have made the corpus the
specification, and so required the model to reproduce a template that changed while
it ran and arithmetic that was sometimes wrong.

They now assert *attribution*: every divergence between generation and the record
falls into a named bucket — an unstated parameter, a template change, or hand
arithmetic — and one that does not is a defect. SC-012 carries the agreement
assertion and applies only to sessions performed after generation began issuing,
because nothing in the corpus was issued.

This is stronger rather than weaker. A date cutoff with three named exclusions
hides every divergence before it; attribution makes each one say what it is.
