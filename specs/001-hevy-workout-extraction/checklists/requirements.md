# Specification Quality Checklist: Hevy workout extraction

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-11
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

### Cleared during planning

**`No [NEEDS CLARIFICATION] markers remain` — now passes.**

Two markers stood at specification time. Neither was a decision the author could make by choosing between options; both were empirical facts about an external system, answerable only by observing it. They were left standing rather than guessed at, on the grounds that the spec already guarded them, and both were resolved during `/speckit-plan` against the live account:

1. **Completeness of the events feed** — CONFIRMED. Requesting from the epoch reconstructs full history; the count matches the source's own exactly.
2. **Rate limits** — NONE OBSERVED, and none documented. Extraction backs off regardless, which costs nothing if the source never throttles.

Both are now recorded under Resolved Questions in the spec, with evidence in `research.md`. Neither was answered by picking a plausible option, which was the reason for deferring them in the first place.

**SC-001 was revised as a consequence.** Observing the account showed it failing on a correct run: one workout exists only as a deletion, so a first extraction lands 164 distinct identifiers against a reported 163. It now counts workouts whose most recent landing record is an update.

Two Open Questions remain, neither carrying a marker: exercise type metadata is a stated dependency for the future normalisation feature rather than an unknown in this one, and the deletion behaviour is a prediction with a deferred live check.

### Resolved during validation

- **Edge cases had no expected behaviour.** Four cases were listed as bare conditions. Each now states expected behaviour and a reason. These are reasoned defaults, not source-confirmed facts, and are marked as such in the spec.
- **Concurrent invocation was unspecified.** It touches the resumption point, so an unstated answer was a silent correctness gap rather than a detail. Added as FR-010.
- **Success and empty-result were conflatable.** A run that lands nothing because nothing changed reads identically to a run that failed, which defeats § 38. Added as FR-011.
- **Assumptions section was absent.** Six assumptions extracted from the requirements — most consequentially, that a workout's source identifier is stable across edits, on which FR-005 and acceptance scenario 3 both rest.

### Borderline, accepted

- **Content Quality, item 1.** The spec names source-interface concepts: endpoint, payload, pagination, events feed, a `since` parameter. These describe the external system being integrated rather than a chosen implementation, and the feature cannot be specified without them. No language, framework, storage engine or internal structure appears.
- **Content Quality, item 3.** "Non-technical stakeholder" is a weak test on a single-operator project (§ I) where the stakeholder and the engineer are the same person. Read as: no knowledge of *this repository* is needed to review it. It passes on that reading.
