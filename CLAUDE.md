# CLAUDE.md

## The constitution

`docs/constitution.md` governs this project. Read it before any
non-trivial change. It is binding rather than aspirational, and it is short
enough to read in full.

Nothing here restates its rules — a rule stated twice is a rule that drifts.
This file covers only the way of working, which the constitution deliberately
says nothing about.

If an instruction, a spec or a plan conflicts with it, say so and settle the
conflict explicitly. Three outcomes are legitimate: amend the constitution,
revise the artifact, or withdraw it. Do not quietly pick one and proceed.

## Where the plan and the reasoning live

Read these before planning anything. Neither is mentioned by the constitution,
and a session that skips them re-derives decisions that were already settled —
or worse, quietly contradicts one.

- **`docs/roadmap.md`** — what is planned, in what order, and why. It carries
  the current state at the top, the order of work, what each decision changed
  about that order, and the questions nobody has answered. It is a living
  document: revise it when the plan moves rather than leaving it to be
  discovered stale.
- **`docs/decisions/`** — why the model is the shape it is. Numbered, dated, and
  amended in place rather than superseded silently. A decision that turns out
  wrong gets amended and says so.

  **Write one only when there was a decision to make.** The operator's advice,
  2026-09-05, after this agent produced three records in a day: a decision record
  earns its place when there was genuine disagreement about approach — between
  contributors, or with a position someone held and changed. *"if there was only
  ever one answer, there was no decision to make."* Being wrong and then being
  corrected is not a disagreement; it is one party catching up, and it belongs in
  a commit message rather than in a numbered record.

  **The same test governs open questions**, wherever they are parked: a question
  is only open if resolving it unblocks something. A thing nobody needs an answer
  to before shipping is not an open question, and a choice that is the operator's
  to make whenever he likes is not one either — it is a programming choice.

**Write things down as they are settled, not at the end.** A long session holds
a great deal of reasoning that exists nowhere else, and the remedy is not a
better memory — it is a commit. If something was decided in conversation and is
not in one of these two places or in the code, it will be lost.

**What does not belong here**: anything about how a particular person likes to
work. That is not a fact about the project.

## Way of working

- **Agree the types, then build to something runnable.** Spec Kit was the way
  of working until 2026-08-20 and is retired. It was heavy, and the two things
  that actually moved the work were neither of its artefacts: iterating on the
  data types before writing them, and running the thing against the real store
  and the real record. A spec document described the calendar's interruptions in
  prose and got them wrong; one list of four dates from the operator showed the
  type was wrong, which no amount of specifying would have.

  So: settle the shape of the types first, out loud, because that is where the
  disagreements are. Then work toward a deliverable the operator can run and
  give feedback on — a command with an expected output, not a milestone. State
  the acceptance lines before building, so "done" is checkable rather than
  claimed.

  **`specs/` is not a source of truth and must not be consulted as one.**
  Retired 2026-08-20, withdrawn as authority 2026-09-01 after a session quoted
  `specs/003`'s out-of-scope list — "Cycling, nutrition phases and the
  constraint calendar" — back at the operator as a reason cycling was not on the
  path. Concurrent gym and cycling programming is the *point of the tool*, and a
  scoping line written by an agent in August was being read as a constraint on
  what the operator is allowed to ask for in September.

  That is the failure mode of the whole directory: 8,244 lines of agent-authored
  prose that the operator has never read, cited back at him as though it
  constrained him. Everything load-bearing in it is in the code (630fa02 moved
  the seed, the candidates and the vocabulary into `domain`). What governs is
  the constitution, `docs/decisions/`, `docs/roadmap.md` and the code.

  **Deleted on 2026-09-01**, along with `.specify/` and the `speckit-*` skills;
  the constitution moved to `docs/constitution.md` on its way out. All of it is
  in git history if a provenance note ever needs chasing. Do not restore it, do
  not cite it, and do not write its like again.
- **Branch, then pull request.** Human sign-off before merge (§ 40). Do not
  merge your own work. Dependency bumps are exempt and merge on green.
- **Conventional Commits.** release-please derives versions and changelogs from
  them, so a mislabelled commit produces a wrong release.
- **`nix flake check` is the gate.** `cargo nextest run` inside `nix develop`
  is the fast inner loop. CI enumerates `checks` from the flake, so adding a
  check there adds a CI job with no workflow edit.

## Layout

`{cli, web} → infrastructure → application → domain`, inward only. Ports are
declared in `application` — the standard hexagonal position, because a port is
defined by what the core needs rather than by what an adapter offers.

Two driving adapters, both operator entry points, both composition roots, both
at ring 3 and neither depending on the other. `cli` is being built first; the
two are meant to reach feature parity, which is both a convenience and the
demonstration that the hexagonal split is real. A capability that only one of
them can invoke is a sign the capability has been built into a transport.

`application` re-exports its ports and errors at the crate root but keeps its
use cases behind `extract`, `normalise` and `status`. That is not tidiness: it
makes `application::extract::…` greppable, and the `use-case-isolation` check
uses it to stop a driven adapter calling the application that is supposed to be
driving it.

**A source's identifiers live with that source's adapter, never in `domain`.**
The exercise mapping is keyed on Hevy's `exercise_template_id` and so sits in
`infrastructure/src/hevy/mapping.rs`; what `domain` owns is the vocabulary it
points at. A domain holding a vendor's identifiers is a domain shaped by a
source, which is the one thing § II.3 rules out — and the second source's
mapping is then a second adapter-side table rather than a `domain` that knows
about two vendors.

**Integration tests that need an adapter live at the adapter's ring.** The
normalisation suites drive the *use case* but need the Hevy translator, and
`application` may not depend on the ring above it — so they are in
`crates/infrastructure/tests/`. That is the hexagon working rather than a
compromise: the use case is generic over its ports, so the test supplying real
ones belongs where real ones live.

Adding a crate takes three edits, and the third is easy to forget:

1. `[workspace.dependencies]` in `Cargo.toml`
2. `workspaceSrc` in `flake.nix` — omit this and cargo is happy while nix
   silently ignores the sources
3. a ring in `crateRings` in `flake.nix`

The `workspace-members` and `architecture` checks catch 2 and 3.

**A test fixture that is not `.rs` needs a fourth edit.** `commonCargoSources`
takes Rust sources and `Cargo.toml` and nothing else, so a `.jsonl` corpus has
to be named in the flake's fileset explicitly — omit it and the tests find an
empty file inside the sandbox while passing on your machine.

## Conventions

- **Standard traits over bespoke methods.** A validated newtype implements
  `TryFrom<String>` and gets the rest — `as_str`, `AsRef`, `Display`,
  `TryFrom<&str>`, `FromStr` — from the macros in `domain::landing::newtype`.
  `FromStr` is not optional alongside `TryFrom<&str>`: `str::parse`, clap's
  value parsers and serde's string forms all go through it. See
  <https://rust-lang.github.io/api-guidelines/checklist.html>.

  The exception is `RawPayload::digest`, which is SHA-256 and deliberately not
  `Hash`: `Hash` hands its bytes to the caller's `Hasher`, produces 64 bits and
  guarantees nothing across builds, and this digest is persisted and compared
  against rows written by earlier versions.
- **A trait names a thing, not an act.** `WorkoutExtractor`, not
  `ExtractWorkouts`. The act is the method.
- **One source's shape is never the only shape.** `LandingRecord` carries what
  every record has whatever served it; anything true only of the transport is a
  `Provenance` variant. Pagination is the same rule applied to the port: the
  source hands back a batch and an opaque resume token, and `PageNumber` lives
  in the Hevy adapter where it means something.
- **A run's identity is derived, never passed.** No use case or driving port
  takes a `LandingStream`: the landing store is bound to a table, declares its
  stream (`HevyWorkoutLandingStore::STREAM`, beside the SQL naming the table),
  and everything else — lock, run log, resumption point, record tags — reads it
  from there. Two streams that must agree are one stream or a silent data loss;
  see `contracts/ports.md`.
- **What this build can collect lives in `cli::catalogue`.** One entry per
  stream — `hevy.workouts`, not `hevy` — and everything a source needs from the
  environment is derived from its name (`HEVY_API_KEY`, `HEVY_API_BASE_URL`), so
  a second source adds an entry and an arm in `cli::wiring` rather than a
  constant and a flag.

  **Three tables, and the third is the porcelain's.** Sources are systems,
  streams are what they serve, and a `KnownDiscipline` is a kind of training with
  the one source and one sink it has. Only `<discipline> next` reads the third:
  the plumbing commands stay flat and take a stream name, because collecting is
  not a discipline-shaped act — body weight has a source, no sink and no session
  to prescribe. A second discipline is an entry and an arm, not a second command.

## Easy to get wrong

- **Panics are `forbid`, not `deny`.** `#[allow(clippy::unwrap_used)]` is a
  compile error (E0453). Fix the error handling; do not reach for the
  attribute, and do not edit `Cargo.toml` to get around it.

  Two macros generate that attribute for you, so they cannot be used at all:

  - **`#[tokio::test]`** on a test returning `Result`. Build the runtime by
    hand — `tokio::runtime::Builder::new_current_thread()` — from a `#[test]`
    function returning `()`.
  - **clap's derive macros.** Use the builder API, which is plain function
    calls.

  Tests therefore return `()` and assert by panicking, which is also what
  `clippy.toml` is configured for: `allow-panic-in-tests` and its siblings
  exist precisely so a test can assert that way. `panic_in_result_fn` is
  forbidden too, so an `assert!` inside a function returning `Result` fails.
- **The test exemptions do not reach free functions.** `expect` is allowed in a
  `#[test]` function and in an `async` block inside one, but not in a helper
  defined alongside them in the same file. Fixture builders return `Result` and
  the test unwraps at the call site.
- **§ II is a data model, not a suggestion.** The common mistakes are storing
  something the analytical layer should derive, resampling component
  observations to a convenient resolution, and treating a match between two
  sources as permission to combine their series.
- **A stub cannot catch a wrong default.** The contract tests point the source
  at a mock server, so anything wrong with the *default* configuration is
  invisible to them — a base URL that already ended in `/v1` produced
  `/v1/v1/workouts/events` and only a live run found it. Pin composed defaults
  in their own unit test.
- **Regenerate `.sqlx` after changing a query**: `cargo sqlx prepare
  --workspace`. Builds read it offline, so a stale directory is a compile
  error rather than a silent fallback.
