<!--
Sync Impact Report
- Version: 1.0.0 (draft, unratified). Pre-push revision is drafting, not amendment, so this
  stays at 1.0.0 until the first push to the remote regardless of how much it changes. The
  version line does not move again until feature implementation has begun.
- Content: operator-authored constitution, sections I-X plus Governance. Supersedes the
  repository-derived draft written earlier in the same session, which was auto-generated
  from repo context and carried no lineage worth preserving.
- Sections: I. Purpose; II. Data architecture; III. Non-observation data; IV. Architecture;
  V. Languages; VI. Type discipline; VII. Testing; VIII. Quality; IX. Operational
  constraints; X. Enforcement; Governance.
- Not governed here: toolchain pinning, licence allowlist and dependency policy. Those stay
  enforced by rust-toolchain.toml, deny.toml and the flake checks.
- The per-rule `[tool]` / `[review]` enforcement tags are gone (46 of them). They applied a
  general preference — prefer a deterministic tool to an LLM's judgement — to each rule
  individually, which turned it into forty claims to keep current and invited bespoke checks
  written to make an unimplemented tag look satisfied. The preference now appears once, in
  § X, where it belongs.
- Deferred items:
  - TODO(RATIFICATION_DATE): ratified once preparation is complete and feature
    implementation can begin.
-->

# Constitution

Governing rules for this project. Non-negotiable unless amended (§ Governance).

How each rule is enforced is a question for § X, and is not recorded rule by rule.

---

## I. Purpose

A single system for ingesting, storing and analysing personal health and fitness data across all platforms in use. The analytical layer is deliberately open-ended: the system's job is to make future analysis possible, not to serve a fixed set of metrics.

Single user, single operator. No multi-tenancy; no auth model beyond credential handling (§ VIII).

## II. Data architecture

**§ II governs observation data: records of what happened, received from external sources.** Within that scope the model is structural and non-negotiable. It has three inputs and three derivations. An input is stored, never computed. A derivation is defined by what it is a function of, not by how it is stored, and is never mutated in place: after any input changes, a derivation is identical to a full re-derivation of it.

Data that is not an observation is outside this model. It acquires no raw, normalised or canonical layer, and "which layer does this belong to?" is not a question that applies to it — the answer is that § II does not reach it. Do not invent a layer, category or slot to accommodate it. Authored records of intent and the parameters we consult are governed by § III. Reconstructible state is governed by nothing at all: an extraction watermark is not an input, because losing it costs a re-fetch rather than a fact.

### Inputs

**1. Raw landing.** Source responses persisted as received, before interpretation. Unrecognised or unparseable fields are retained, never discarded. Raw is append-only: landing records are never mutated, compacted or deleted. A source re-serving changed data lands as a new record, and supersession is resolved at the canonical layer, never by rewriting raw.

**2. Operator overlays.** Operator input is stored separately from every derivation and applied only during rebuild — never patched into derived rows directly — so removing an overlay record and rebuilding restores the underlying deterministic result. Two overlays exist.

- *Edit overlay* — operator corrections to individual observations, any field. An input to the normalised layer.
- *Match overlay* — operator assertions that two entities are, or are not, the same real-world event. An input to the canonical layer.

Overlays are peers of raw, not derivations of it. Unlike raw they are not append-only: retraction is the point of an override you can undo.

Obligations, common to both:

- **Anchored to source identity**, never to a surrogate key, so overlay records survive a rebuild.
- **Both directions are assertable.** For matching, a negative assertion is a first-class record and not merely the absence of a positive one — without it, a deterministic rule re-fires the same wrong match on every rebuild.
- **Bulk application is supported.** Many observations or correspondences overridden in one action.
- **Dated and given a reason.** An unexplained override is unreadable six months later.
- **Overlay provenance is queryable** — what was overridden, when and why. Available for inspection; series logic does not consult it.
- **Overrides do not propagate.** New data matching an old override does not inherit it, which keeps a capture gap visible rather than papering over it indefinitely.

### Derivations

**3. Normalised layer.** A function of raw, deterministic translation, and the edit overlay. Per-source and derived per-record: each normalised entity is a function of exactly one raw landing record, never of two, and never of another source. This layer says what each source said, in our terms — including where a source has said the same thing twice, or later contradicted itself.

It models domain entities — a strength workout of ordered exercises and sets; a cycling workout of summary and samples; a body measurement — whose definitions are declared, version-controlled and owned here, extending § 8 from identity to structure. Sources are translated into these entities, never the reverse: no source's format shapes the domain, and a new or historical source is an adapter question, not a modelling one. A standalone reading is the degenerate entity. Component observations keep the source's native temporal resolution — never resampled, aggregated or interpolated — and belong to their parent entity. Two sources recording one real-world event produce two entities here, and that is correct.

- **Provenance is mandatory:** the source that produced the observation, whatever version or algorithm identifier the source exposes, and the identifier by which the source names this record — which is what makes same-source supersession mechanically detectable at § 4. Provenance records what a source actually tells us; it is not inferred or invented.
- **Units canonicalised** (kg, metres, seconds, bpm, watts).
- **Timestamps are local wall-clock time plus IANA timezone identifier.** 8pm stays 8pm. The UTC instant is derivable and is never the stored primary; an offset is not a substitute for a zone. Where a source supplies only a UTC instant, the zone is taken from declared operator configuration — a versioned input to deterministic translation (§ 9), not an inference about the source. Exceptions (e.g. travel) are corrected through the edit overlay.

**4. Canonical layer.** A function of the normalised layer, deterministic matching, and the match overlay. Cross-source: one entry per real-world event, whatever number of sources recorded it, and whatever number of times each recorded it. This is the clean layer the analytical layer reads.

Provenance survives reconciliation: a canonical entity always names the normalised entities it stands for. Identity, provenance and supersession apply at entity and observation level alike. Metrics are not stored here — e1RM, relative strength and their kind are analytical-layer functions over entities (§ 5).

**5. Analytical layer.** Built from functions over the canonical layer. Nothing here is a system of record.

### Rules following from the above

**6.** **Every metric declares a comparability class.**

- *Source-independent* — the observation is the same fact however it was recorded. Load, reps, duration, distance. Series combine sources freely.
- *Method-dependent* — the value is inseparable from the sensor and algorithm that produced it. Body fat %, HRV, RHR, FTP. Series never stitch across sources; a body fat % from Withings and one from InBody are different series, not noisy versions of the same one.

Edits interact with the class. Excluding a method-dependent observation leaves a gap in its series — gaps stay visible (§ 37). Substituting its value replaces the source's algorithm with operator judgement: a different method, hence a different series. In practice, method-dependent values are excluded, never corrected — there is no true value to restore.

The declaration duty extends to the analytical layer. A derived series' method is the tuple of its inputs' methods and its own derivation choices (formula, parameters); changing any element starts a new series, never a silent splice into the old one. Derived series over method-dependent inputs never mix input sources the inputs themselves could not.

**7.** Re-derivation must be possible from the inputs — raw, translations, edits and matches — without refetching. Each derivation re-derives from the derivation below it plus its own inputs, and the chain runs end to end. If it isn't possible, the ingestion is incomplete.

**8.** **Entity identity is ours, not the source's.** Entity vocabulary is declared, version-controlled and owned here, and applied at the normalised layer. Mapping defaults to passthrough; deterministic translation merges where a source over-separates; edits reassign.

Assistance load is a property of a pull-up, not a different exercise — a source that separates them is translated, not obeyed.

**9.** **Deterministic derivation is code; operator override is data.**

- *Deterministic translation* — source identity plus recorded values resolve to a normalised entity with no further input.
- *Deterministic matching* — source identities plus recorded values resolve to a correspondence with no further input. Authoritative when it fires; the match overlay covers what it cannot reach, and both directions of assertion are available because a rule that fires wrongly would otherwise re-fire on every rebuild.

Both are re-runnable at any time. Neither consults an overlay.

**10.** **Correspondence is ordered within a source and unordered across sources.**

- Two records sharing a source identity are the same source contradicting itself. The later supersedes; the earlier remains in raw and normalised but is not current.
- Records from different sources are co-observations. Neither supersedes the other, both stand, and disagreement between them is evidence rather than error. Choosing which to read is an analytical-layer decision (§ 5), never resolved by discarding one.

Consequences:

- **Absence of a correspondence means unknown, not distinct.** Only a negative assertion establishes that two entities are different events.
- **Matching does not merge series.** A correspondence says two entities describe one event, not that their observations share a scale. Method-dependent observations (§ 6) stay distinct across a matched pair exactly as they would unmatched.
- **Counting respects correspondence.** Any function whose result depends on how many events occurred — session counts, volume totals, frequency, streaks — operates over correspondences, not over the normalised entity list. An unmatched duplicate silently inflates every such figure.

**11.** **Prescribed and performed are stored separately and joinably.** Assumed anchors, prescribed loads and planned sessions are recorded — expectation against reality must be recoverable — but they are not observations. The separation is one-directional: prescribed data never satisfies a query about what happened, and never feeds a derived series that claims to measure.

## III. Non-observation data

§ II governs what happened. This section governs the rest of what the system stores: what we intend, and what we consult. Neither is derived, neither passes through raw, and neither is subject to § II's layering.

**12.** **Authored data.** Records of intent, authored by us: prescriptions, planned sessions, assumed anchors. It is a primary input — nothing regenerates it if lost — so it is stored durably and keeps its history. No raw layer applies: raw exists to guard against fallible translation of a format we do not control, and we control this one entirely. § 11 governs its relationship to observations and is not weakened here.

**13.** **Interpretive parameters.** Values consulted in order to interpret observations: heart-rate and power zones, FTP, the default timezone. Effect-dated and retained — the value in force at the time of the observation is the one that applies, and a superseded value is never overwritten or deleted. § 7 requires this: analytical results are re-derived on demand rather than stored (§ 5), so a lost past value makes past analysis unreproducible. Changing one does not rewrite an existing series; § 6 governs.

**14.** **Generation parameters.** Values consulted when authoring a program: warmup set percentages, scheduling constraints, the family calendar. Only the current value is required. What was generated is recorded concretely in the authored record (§ 12), so the parameter that produced it needs no history and a superseded value answers no question. Where such a value is received from an external system it arrives through an adapter behind a port (§ 16), and need not be persisted at all.

## IV. Architecture

**15.** Hexagonal. Domain logic has no knowledge of adapters, transports, storage engines or vendors.

**16.** Every external system is a driven adapter behind a port — data sources, the store, and any model. No vendor type crosses a port boundary.

**17.** Deterministic first. Any capability derivable from an algorithm is implemented as one; progression schemes, load derivation and scheduling are algorithmic problems, not generative ones. A capability may depend on an LLM only where no deterministic derivation exists.

**18.** Where an LLM is used it sits behind a port like any other external system, and the model is swappable without touching domain logic.

**19.** **The frontend contains no domain logic.** It requests, renders and interacts; derivation, aggregation and business rules live behind the API. A frontend needing to compute something the API doesn't serve is evidence of a missing endpoint, not a place for logic.

## V. Languages

**20.** Backend: Rust. Frontend: server-rendered HTML from Rust templates, HTMX for client interactivity, server-rendered SVG for visualisation. No authored application JavaScript or TypeScript. Vendored libraries driven entirely by markup or data attributes (htmx itself) are exempt as tooling; a library requiring authored client code is not vendorable under this rule — it is an amendment.

**21.** No third language for application code. Build, CI and operational tooling are exempt. Interface languages confined to their adapter — SQL at the store, query or template syntaxes at their respective ports — are the adapter's vendor surface, not application languages. Logic expressible in the application language does not migrate into them.

**22.** Analysis worth doing is a platform capability, not a script. Ad hoc scripts outside this repository are legacy pending absorption; their capabilities are re-specified and rebuilt here, never ported or extended.

## VI. Type discipline

**23.** No raw types at domain boundaries. Primitives are wrapped in newtypes; alternatives are sum types.

**24.** Illegal states are unrepresentable. Validation happens at construction; downstream code does not re-check invariants the type already guarantees.

**25.** Types document. A type name requiring a comment to explain what it holds is the wrong type name.

**26.** Errors are typed. No panics and no unhandled errors in the domain. Adapters translate vendor failures into domain error types at the port — no vendor error crosses a boundary.

## VII. Testing

**27.** Types first. Model the domain before writing behaviour. A test the type system makes impossible to fail is not written.

**28.** A randomly generated instance of a type must be valid. If an arbitrary instance can violate an invariant, the type is wrong — not the generator. Property-based tests assert this.

**29.** Integration tests at port boundaries are the primary suite and the agent's steering signal. They exercise the public API through real ports with test adapters.

**30.** The public API is fully tested. Coverage of internals is a consequence of testing through the public interface, not a target in itself — per-function tests are not required. Substantial code unreachable from any API test is either dead or evidence of a missing API test.

**31.** Red-green-refactor applies at the port-boundary level, not per function.

## VIII. Quality

Quality is never traded for delivery speed.

**32.** Minimalism applies to scope, not to quality. One capability right end to end before the next — "right", not "minimal".

**33.** No proof-of-concept-grade code in the repository. If it is committed, it is built to the standard of this document.

## IX. Operational constraints

**34.** Deployment-agnostic. No hardcoded paths, hosts, ports or environment assumptions; all of it configuration. Moving between hosts requires no code change.

**35.** Credentials never enter version control. They are supplied by environment or local config; refreshable tokens are persisted outside the repository and rewritten in place on refresh.

**36.** A source being unavailable degrades the system; it never fails it. Capabilities not depending on that source continue to work.

**37.** Partial data is recorded as partial. No silent interpolation, no carrying values forward, no gap-filling on write.

**38.** Staleness is observable. The newest observation per source is queryable, so a broken ingestion is visible rather than silent.

## X. Enforcement

Where an **established** tool can check a rule — a compiler lint, a linter, a scanner — it checks it. Deterministic, cheap, and a better thing for an agent to code against than a reviewer's attention. Prefer the mechanism that cannot be talked around: a `forbid` lint is a compile error no attribute can override, where a denied one is an invitation to add an exception.

Where no established tool exists, review covers it. That is a resting place, not a debt. A bespoke check written to close the gap is a tool nobody maintains, a rule stated twice, and a false sense that the gap is closed — the greater risk is a check whose only real effect is that it can be edited to pass.

No rule above records which of the two applies to it. Enforceability changes as tooling does, and a per-rule claim would be forty promises to keep current. Where checks and review prompts live is a repository-layout question. This document names no path.

**39.** Review covers design, naming, invariant modelling and architectural fit. Agent review surfaces candidates for attention; it does not gate, because it shares blind spots with the agent that wrote the code.

**40.** Human sign-off before merge is a real gate and is not delegated.

## Governance

- **Conflicts are surfaced, not silently resolved.** Where this document conflicts with a spec, a plan, or a current instruction, the conflict is raised and settled explicitly. Three outcomes are legitimate: amend this document, revise the artifact, or withdraw the artifact. A written artifact never outranks current intent by virtue of being written down, and precedence is never applied as an automatic tiebreak.
- **This document is a draft until preparation is complete.** It is ratified at the point feature implementation can genuinely begin, not before. A draft still governs; it is simply expected to move.
- **The version stays at 1.0.0 until the first push to the remote**, however many times this document is edited before then. Revision before that point is drafting, not amendment, and does not bump. Once feature implementation has begun, every change bumps the version: MAJOR for a rule removed or redefined incompatibly, MINOR for a rule added or materially widened, PATCH for clarification.
- **`docs/decisions/` records genuine changes of direction**, and decisions where more than one option was legitimately available. It is not a changelog for edits to this document. Nothing is owed to it until implementation has started — before then there is no direction to have changed.
- A rule that is repeatedly violated is evidence to either automate it or drop it — not to restate it.

**Version**: 1.0.0 (draft, unratified) | **Ratified**: TODO(RATIFICATION_DATE): on completion of preparation, when feature implementation can begin | **Last Amended**: 2026-08-08
