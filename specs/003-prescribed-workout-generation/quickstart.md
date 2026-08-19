# Phase 1: Quickstart — validating prescribed workout generation

Every scenario in the spec, as something runnable. Integration tests at port
boundaries are the primary suite (§ 29), and the corpus is the fixture — 165
landing records already extracted, 164 normalised workouts.

**Placement.** The generation suites need the Hevy translator to build history from
raw, and `application` may not depend on `infrastructure`. So they live in
`crates/infrastructure/tests/`, exactly as the normalisation suites do and for the
same reason: the use case is generic over its ports, so the test supplying real ones
belongs where real ones live.

**Fixtures.** The authored programme is a `.toml` document in
`crates/infrastructure/tests/fixtures/`. That directory is already named in
`flake.nix`'s fileset — `commonCargoSources` takes `.rs` and `Cargo.toml` and
nothing else, so a fixture anywhere else is empty inside the nix sandbox while
passing on your machine.

---

## Prerequisites

```bash
nix develop
export FITNESS_TRACKER_DATABASE=local.db
export FITNESS_TRACKER_TIMEZONE=Europe/London
```

The store must be current before anything below means much:

```bash
fitness extract hevy.workouts
fitness normalise hevy.workouts
```

Then author the programme. **This will fail until the ladder's span is supplied**
(research D8) — the document rejects a remaining `TODO` rather than defaulting:

```bash
fitness programme author crates/infrastructure/tests/fixtures/programme.toml
fitness programme show
```

---

## The fast loop

```bash
cargo nextest run                 # inside nix develop
nix flake check                   # the gate
cargo sqlx prepare --workspace    # after changing any query
```

`nix flake check` enumerates the flake's `checks`, so `architecture`,
`use-case-isolation`, `document-format-is-an-adapters` and `workspace-members` run
without a workflow edit. Two of them matter especially here.

**`document-format-is-an-adapters` is what keeps `toml` out of `domain`**, and it
exists because this guide used to say `architecture` did that. It does not and
cannot: `architecture` reads *path* dependencies, which is what makes it a ring
check, and `toml` is a registry crate and therefore invisible to it. Verified by
adding `toml` to `crates/domain/Cargo.toml` and watching the new check fail.

**`use-case-isolation` is what stops a store adapter calling the generation it is
supposed to be driven by.** It named `extract` and `status` only until 2026-08-19,
so `prescribe` was unguarded for as long as it existed; it now names all four use
cases and is scoped to `src`, because an integration test at this ring does drive a
use case and legitimately.

---

## Scenario group 1 — a workout is issued (US1)

```bash
fitness prescribe --date 2026-08-17
```

| Scenario | What it asserts | Where |
| --- | --- | --- |
| US1-1 | One workout, items in fatigue order, every set pinning an axis | `prescribe_issues_a_complete_workout` |
| US1-2 | The primary's sets are a function of anchor and role; no performed value on the path | `the_primary_reads_only_programme_state` |
| US1-3 | Back-off is 85% of the *top set*, not of the anchor | `back_off_follows_the_top_set` |
| US1-4 | 80kg top set → 67.5 back-off; an exact tie resolves down | `quantisation_is_nearest_ties_down` |
| US1-5 | Non-primary slots derive from their own last performance | `accessories_read_observed_history` |
| US1-6 | An alternating fill reads two sessions back, not one | `an_alternating_fill_reaches_past_the_last_session` |
| US1-7 | A never-performed exercise is reported, not given a guessed load | `a_never_performed_slot_is_reported` |
| US1-8 | The issued prescription is stored in full | `an_issued_prescription_is_stored` |
| US1-9 | No performed query returns prescribed data | `prescription_never_answers_a_performed_query` |

**US1-4 is a property test as well as a case.** `quantise` over arbitrary loads and
increments: the result is always a multiple of the increment, always within half an
increment of the input, and always the lower of two equidistant candidates. The four
worked cases from [data-model.md](./data-model.md) — 68 → 67.5, 78.75 → 77.5,
74.375 → 75, 72.25 → 72.5 — are the named anchors alongside it.

**US1-6 is the one that would pass by accident.** The hip-dominant slot alternates
back-extension machine on light days with Nordic curls on heavy days, so a bounded
"last session" lookback returns the wrong exercise's history and still produces a
plausible number. The assertion is on the *date* the history came from, not only on
the load.

**US1-9 is a negative test and needs to stay one.** It asserts that
`ExerciseHistory` and `PerformedWorkoutReader` return nothing derived from
`prescribed_*` tables. There is no positive assertion that would catch a regression
here.

---

## Scenario group 2 — a failed attempt (US2)

```bash
fitness normalise hevy.workouts
fitness refusals
```

| Scenario | What it asserts | Where |
| --- | --- | --- |
| US2-1 | The 95kg set of 2026-07-03 is a failed attempt, not a refusal | `zero_reps_becomes_a_failed_attempt` |
| US2-2 | 77 `failure`-typed sets, 76 of them completed working sets | `the_failure_set_type_is_not_the_discriminator` |
| US2-3 | A failure contributes nothing to any total, count or estimate | `a_failure_is_not_a_quantity` |
| US2-4 | A failure and an absence are distinguishable | `a_failure_is_not_an_absence` |
| US2-5 | Re-derivation over unchanged raw is identical | `re_derivation_is_unaffected` |

**Expected after this group** — the SC-006 numbers, and they are exact:

```text
refusals           2      (was 3)
  non-contiguous-grouping   1   landing record 122
  single-member-grouping    1   landing record 127
failed attempts    1      landing record 10, front-squat, 95kg
```

**US2-2 is the guard against the tempting shortcut.** Keying the outcome on Hevy's
`failure` set type instead of on `reps == 0` would misfile 76 completed sets, and
every one of those sets currently contributes to a volume total. The assertion is on
the count: 77 sets of that type, 1 failed attempt.

**US2-3 must be checked as a diff, not as a value.** SC-007 says no total, count or
maximum estimate changes as a result of the attempt becoming visible. So the test
computes the totals before the change and after it and asserts equality — a
hard-coded expected total would pass even if the failure were being counted and the
constant had been updated to match.

---

## Scenario group 3 — the plan, and failure (US3)

Split as the spec splits them: the plan is a pure function of two inputs and is
tested as one; the failure mechanism needs misses the record does not contain and is
driven through fakes.

**The plan.** No performed record involved at all — these are property and table
tests over `Ladder`.

| Scenario | What it asserts | Where |
| --- | --- | --- |
| US3-1 | Weeks + 1RM produce every week's loading; last week is a test | `two_inputs_generate_the_block` |
| US3-2 | The anchor is identical in every week of a block | `the_anchor_does_not_move_within_a_block` |
| US3-3 | Step = span ÷ climbing weeks; changing duration changes the step | `the_step_derives_from_the_endpoint` |
| US3-4 | Any effort report → the load is exactly what the ladder says | `nothing_performed_climbs_it_faster` |
| — | A one-climbing-week block does not divide by zero | `a_degenerate_ladder_has_one_position` |

**Failure.** Fakes supplying misses.

| Scenario | What it asserts | Where |
| --- | --- | --- |
| US3-5 | A failed top set re-issues the week, no advance | `a_miss_holds_the_ladder` |
| US3-6 | Second failure at the same load → reset 1 from the failed load | `a_second_miss_suspends_the_ladder` |
| US3-7 | Re-climb reaching the failed load resumes at the suspended week | `a_completed_re_climb_resumes_the_ladder` |
| US3-8 | A stall during reset 1 → reset 2, −5% at +2.5kg/week | `the_second_stall_is_the_slower_reset` |
| US3-9 | A test anchors the next block, above or below the endpoint | `a_test_anchors_the_next_block` |
| US3-10 | A non-gating session's miss does not touch the ladder | `only_the_gating_role_gates` |
| — | **The anchor is unchanged across every reset** | `a_reset_never_touches_the_anchor` |

**US3-3 is the test that keeps the endpoint authoritative.** It generates the same
start and end percentages over 8 weeks and over 12, and asserts the endpoints match
while the steps differ. If someone later authors a step and derives the endpoint,
this fails.

**The unnumbered anchor test is the one to write first.** FR-021 is the whole point
of separating the plan from the failure mechanism, and it is the invariant most
likely to be quietly broken by an implementation that finds it convenient to move
the anchor during a reset.

**SC-005 is one table, asserted load for load** — the worked example from
`primary-lift-progression.md`. Note the anchor column: it is the same value in all
eleven rows, which is FR-021 read off the fixture.

| Week | Load | Result | State after | Anchor |
| --- | --- | --- | --- | --- |
| 1 | 90 | miss | ladder held | unchanged |
| 2 | 90 | miss | ladder suspended, reset 1 begins | unchanged |
| 3 | 80 | pass | re-climbing, +5 | unchanged |
| 4 | 85 | pass | re-climbing, +5 | unchanged |
| 5 | 90 | miss | at the failed load, missed again | unchanged |
| 6 | 90 | miss | reset 2 begins | unchanged |
| 7 | 80 | pass | re-climbing, +2.5 | unchanged |
| 8 | 82.5 | pass | re-climbing, +2.5 | unchanged |
| 9 | 85 | pass | re-climbing, +2.5 | unchanged |
| 10 | 87.5 | pass | re-climbing, +2.5 | unchanged |
| 11 | 90 | miss | at the failed load, missed again | unchanged |

**US3-2 is the test that keeps the gate negative.** It feeds a completed top set
carrying every `Rir` the vocabulary has and asserts the advance is identical in
every case. If someone later wires effort into the derivation, this is what fails.

**FR-010, from two directions.** `asking_twice_does_not_double_advance` runs
`prescribe` for one date twice and asserts the ladder position is unchanged and one
prescription exists. This should be impossible to fail given that the position is
derived (research D2), which is the point — it is a regression test against someone
reintroducing stored state.

---

## Scenario group 4 — the round trip (SC-010, SC-012)

**The projection is a model invariant; the comparison against the corpus is a
diagnostic.** Nothing in the corpus was issued from a prescription, so a
divergence between what generation produces for a past date and what was actually
prescribed is information rather than a failure — it locates a parameter nobody
has stated, a template that has since changed, or arithmetic done wrong by hand.
The criterion that does assert agreement is SC-012, and it cannot run until
generation has issued something and that session has been trained.

```rust
// Sketch. For each session in the corpus:
let performed = reader.between(date, date).await?;
let projection = project(&performed[0]);
let prescribed = prescriber.prescribe(date).await?;
let divergences = satisfies(&projection.shape, &prescribed.workout.shape());
```

| Scenario | What it asserts | Where |
| --- | --- | --- |
| SC-010a | All fifteen sessions since 15 June project without panicking | `every_session_projects` |
| SC-010b | Structure agrees: blocks, order, grouping, slots | `generation_reproduces_the_structure_of_the_record` |
| SC-010c | Loads are compared to 2026-08-07 onward, and every divergence is attributable | `divergences_from_the_record_are_attributable` |
| SC-010d | A projected `Exactly(6)` satisfies a prescribed `Range { 4, 6 }` | `satisfaction_is_direction_aware` |
| SC-010e | A projected shape cannot be issued | *compile-fail test* |
| SC-010f | The 95kg failure projects with `IntendedMeasureUnknown` | `a_failed_attempt_projects_a_gap` |

**SC-010c asserts attribution, not agreement.** Each divergence must fall into one
of three named buckets — an unstated parameter, a template change, or hand
arithmetic — and a divergence outside them is a defect in generation. That is
strictly stronger than a date cutoff with three named exclusions, which is what an
earlier draft had: a cutoff hides every divergence before it, where attribution
makes each one say what it is.

**SC-010e is a compile-fail test, not a runtime one.** FR-034 is held by the types —
only generation can build a `PrescribedWorkout`, because only generation has an
anchor, a cycle and a programme — so the assertion is that
`store.issue(projection.shape)` does not compile. A runtime assertion here would be
testing a rule that should not be expressible.

**What SC-010b will legitimately diverge on**, and the test records rather than
fails: the 2026-08-14 mobility grouping (five in one superset, where the three
sessions before grouped three and left two single) and the 2026-08-14 triceps
substitution. Both are recorded in
[contracts/programme.md](./contracts/programme.md) as record variance the template
does not model. The assertion is that these are the *only* divergences.

---

## Scenario group 5 — authoring and the CLI

| Scenario | What it asserts | Where |
| --- | --- | --- |
| A-1 | A document with a remaining `TODO` is rejected, naming the field | `a_placeholder_does_not_author` |
| A-2 | Authoring supersedes by date; the previous version survives | `authoring_supersedes_and_retains` |
| A-3 | Gating on a role the programme never runs is rejected | `an_unrunnable_gating_role_is_rejected` |
| A-4 | A primary exercise that is not the primary slot's fill is rejected | `the_primary_must_fill_its_slot` |
| A-5 | A duration exercise as primary is rejected | `the_primary_must_be_a_reps_exercise` |
| A-6 | `--date` on no programmed weekday names the programmed days | `a_wednesday_is_declined_with_the_programmed_days` |
| A-7 | `--date` omitted resolves to the next programmed day and prints it | `the_default_date_resolves_forward_and_says_so` |
| A-8 | No `toml` type appears in `domain` | *`architecture` check* |
| A-9 | Composed defaults are pinned in a unit test | `the_default_date_is_not_today_on_a_rest_day` |

**A-9 exists because of 001's scar.** A stub cannot catch a wrong default: the
contract tests point at a mock, so anything wrong with the *default configuration*
is invisible to them — a base URL already ending in `/v1` produced `/v1/v1/...` and
only a live run found it. The composed default here is `--date`, which resolves
forward through the calendar in the operator's zone. It gets its own unit test.

---

## What is not tested here, and why

- **Supersession against real data.** The corpus has 165 distinct source ids and no
  re-serve, so D3's § 10 handling is exercised synthetically — as 001 and 002 both
  left it. Recorded rather than argued away.
- **Correspondence between a prescription and the performance that satisfied it.**
  Out of scope. SC-010 compares a projection against a generation for the same
  date, which is deliberately *not* a claim that one motivated the other.
- **Fragmentation.** One session across four landing records stays four workouts.
  Harmless for "the most recent performance of this exercise", and the first figure
  that needs it right — a session count, a frequency, a streak — is the trigger to
  build the canonical layer properly.
- **The web adapter.** Dormant, as in 001 and 002.

---

## The one thing that blocks all of this

Research D8, now narrowed to a single value: **the ladder's start and end
percentages**. The anchor is known (90kg, tested 2026-07-03), the rep counts are
known, the light-of-heavy percentage is evidenced against three weeks, and the
back-off and warm-up percentages are evidenced against six and three sessions
respectively.

No test above that asserts a real primary load can be written until the span is
stated. SC-010c's expected table and the fixture document are both written with the
values absent so that neither compiles — a placeholder that runs green is worse than
one that will not build.

Everything else in this guide is writable now: the plan's property tests (US3-1 to
US3-4) hold for any span, the whole failure group is span-independent, group 2 needs
no span at all, and group 4's structural comparison (SC-010b) is about blocks,
order, grouping and slots rather than loads.
