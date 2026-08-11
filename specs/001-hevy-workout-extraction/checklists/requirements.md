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

- [ ] No [NEEDS CLARIFICATION] markers remain
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

### Outstanding

**`No [NEEDS CLARIFICATION] markers remain` — fails deliberately, and does not block planning.**

Two markers remain under Open Questions. Neither is a decision the author can make by choosing between options; both are empirical facts about an external system, answerable only by observing it:

1. **Completeness of the events feed** — whether requesting from the epoch reconstructs full history.
2. **Rate limits** — whether the source throttles and how it signals it.

Answering these by picking a plausible option would record a guess as a decision. They are left standing because the spec already guards them: SC-001 fails loudly if (1) is wrong, and (2) affects how extraction paces requests, not what it guarantees. Both resolve during `/speckit-plan`, against the live account or the source's documentation.

The third Open Question (exercise type metadata) carries no marker — it is a stated dependency for the future normalisation feature, not an unknown in this one.

### Resolved during validation

- **Edge cases had no expected behaviour.** Four cases were listed as bare conditions. Each now states expected behaviour and a reason. These are reasoned defaults, not source-confirmed facts, and are marked as such in the spec.
- **Concurrent invocation was unspecified.** It touches the resumption point, so an unstated answer was a silent correctness gap rather than a detail. Added as FR-010.
- **Success and empty-result were conflatable.** A run that lands nothing because nothing changed reads identically to a run that failed, which defeats § 38. Added as FR-011.
- **Assumptions section was absent.** Six assumptions extracted from the requirements — most consequentially, that a workout's source identifier is stable across edits, on which FR-005 and acceptance scenario 3 both rest.

### Borderline, accepted

- **Content Quality, item 1.** The spec names source-interface concepts: endpoint, payload, pagination, events feed, a `since` parameter. These describe the external system being integrated rather than a chosen implementation, and the feature cannot be specified without them. No language, framework, storage engine or internal structure appears.
- **Content Quality, item 3.** "Non-technical stakeholder" is a weak test on a single-operator project (§ I) where the stakeholder and the engineer are the same person. Read as: no knowledge of *this repository* is needed to review it. It passes on that reading.
