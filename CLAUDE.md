# CLAUDE.md

## The constitution

`.specify/memory/constitution.md` governs this project. Read it before any
non-trivial change. It is binding rather than aspirational, and it is short
enough to read in full.

Nothing here restates its rules — a rule stated twice is a rule that drifts.
This file covers only the way of working, which the constitution deliberately
says nothing about.

If an instruction, a spec or a plan conflicts with it, say so and settle the
conflict explicitly. Three outcomes are legitimate: amend the constitution,
revise the artifact, or withdraw it. Do not quietly pick one and proceed.

## Way of working

- **Spec Kit.** `/speckit-specify` → `/speckit-plan` → `/speckit-tasks` →
  `/speckit-implement`. No implementation before a spec and plan exist.
- **Branch, then pull request.** Human sign-off before merge (§ 40). Do not
  merge your own work. Dependency bumps are exempt and merge on green.
- **Conventional Commits.** release-please derives versions and changelogs from
  them, so a mislabelled commit produces a wrong release.
- **`nix flake check` is the gate.** `cargo nextest run` inside `nix develop`
  is the fast inner loop. CI enumerates `checks` from the flake, so adding a
  check there adds a CI job with no workflow edit.

## Layout

`{cli, web} → infrastructure → application → domain`, inward only. Ports are
declared in `application`.

Two driving adapters, both composition roots, both at ring 3 and neither
depending on the other. `cli` is the operator's entry point — extraction is
invoked from a terminal or an external scheduler, never over HTTP. `web` is the
HTTP surface. A capability belongs to whichever transport invokes it; a batch
job behind a web binary is a name that misleads.

Adding a crate takes three edits, and the third is easy to forget:

1. `[workspace.dependencies]` in `Cargo.toml`
2. `workspaceSrc` in `flake.nix` — omit this and cargo is happy while nix
   silently ignores the sources
3. a ring in `crateRings` in `flake.nix`

The `workspace-members` and `architecture` checks catch 2 and 3.

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
