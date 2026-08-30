# Fitness Tracker

A single system for ingesting, storing and analysing personal health and
fitness data across every platform in use. Single user, single operator.

The rules governing it — the observation data model above all — are in
[`.specify/memory/constitution.md`](.specify/memory/constitution.md), and are
binding rather than aspirational. Read it before changing anything here.

A Rust workspace laid out for hexagonal architecture, built with
[crane](https://github.com/ipetkov/crane) over a
[fenix](https://github.com/nix-community/fenix) toolchain.

## Layout

Dependencies point inward only. Nothing in an inner ring may name anything in
an outer one:

```
cli ──┐
      ├──▶ infrastructure ──▶ application ──▶ domain
web ──┘
```

| Crate | Role | What belongs here |
| --- | --- | --- |
| `domain` | The core | Entities, value objects, and the rules that govern them. Depends on no workspace crate, and never on a framework, database, or transport. Data-type dependencies — a timestamp, a hash — are fine. |
| `application` | Use cases and ports | The things your software *does*, and the traits it needs the outside world to satisfy. Ports are declared here, in the application's own vocabulary, and implemented further out. |
| `infrastructure` | Driven adapters | Implementations of the driven ports: a database, an HTTP client, a filesystem. The one place a technology choice is allowed to show. |
| `cli` | Driving adapter, composition root | The operator's entry point: `fitness gym next` for the daily loop, and `extract`, `normalise`, `refusals`, `status`, `reset` beneath it. Extraction is invoked from a terminal or an external scheduler, never over HTTP. |
| `web` | Driving adapter, composition root | The HTTP surface. Translates requests into use-case calls. |

`cli` and `web` are peers at the same ring. Neither depends on the other, and a
capability belongs to whichever transport invokes it.

Two consequences worth keeping in mind:

- **Errors get translated at each boundary.** `application::RepositoryError`
  carries no SQL codes and no HTTP statuses, so a change of database cannot
  ripple inwards.
- **Use cases are generic over their ports.** `Extraction` takes any source,
  store and clock, which is why its tests use fakes and touch no I/O. If a test
  needs a database, the dependency is pointing the wrong way.

## The daily loop

```bash
export HEVY_API_KEY=...              # from https://hevy.com/settings?developer
export FITNESS_TRACKER_DATABASE=./local.db
export FITNESS_TRACKER_TIMEZONE=Europe/London

fitness gym next                     # collect, derive, prescribe, deliver
```

One command, because it is one question: *what am I doing next?* It runs the
four steps below in order, reports each one's outcome, and stops at the first
that fails — with that step's exit code, and the store correct as far as it got.
Re-running resumes rather than repeating: collection picks up from its
watermark, an unchanged session is not re-issued, and a session already at the
destination is not sent again.

It is nested under `gym` because a *pipeline* has one source and one sink only
once a kind of training has been named. The steps below stay flat and take a
stream, because collecting is not a discipline-shaped act.

## Collecting and normalising

```bash
fitness extract   hevy.workouts      # collect since the resumption point
fitness normalise hevy.workouts      # derive gym workouts from what raw holds
fitness refusals  hevy.workouts      # what the domain would not accept, and why
fitness status    hevy.workouts      # where both derivations stand
fitness reset     hevy.workouts      # discard the position; next run collects everything
```

`normalise` contacts nothing. It reads raw and writes the normalised layer, so
it works with every source down and can run while an extraction is in flight —
it takes no lock and moves no resumption point.

| Variable | Flag | Required | Default |
| --- | --- | --- | --- |
| `HEVY_API_KEY` | *none, deliberately* | to extract | — |
| `FITNESS_TRACKER_DATABASE` | `--database` | yes | — |
| `FITNESS_TRACKER_TIMEZONE` | *none, deliberately* | to normalise | — |
| `HEVY_API_BASE_URL` | `--base-url` | no | `https://api.hevyapp.com` |

The credential has no flag. A secret on the command line lands in shell history
and in `ps` output; put it in the environment or an untracked `.env`.

The time zone has no flag either, and no default. It is a declared interpretive
parameter rather than a per-invocation choice — an operator who can pass it per
run can produce two derivations that disagree — and a compiled-in default would
be an assumption about where you train, silently right for one account and
silently wrong for the next.

Exit codes are part of the contract, so an external scheduler can tell outcomes
apart: `0` success (including a run that found nothing, and a derivation that
recorded refusals), `1` the source was unreachable or rejected the credential,
`2` another run is in progress, `3` the store, `4` usage, `5` an exercise
template the mapping does not cover.

Refusals do not affect the exit code. A derivation that recorded 26 of them
succeeded: it found 26 things wrong with the data and said so, which is the
feature working.

## Commands

| Command | Does |
| --- | --- |
| `nix develop` | Enter a shell with the toolchain and every tool CI uses. `direnv allow` does this automatically. |
| `nix flake check` | Run everything CI runs: per-crate builds, rustfmt, clippy, tests, doctests, `cargo audit`, `cargo deny`, and a nix formatting check. |
| `nix build` | Build into `./result`. |
| `nix run .#cli` | Build and run `fitness`. |
| `nix fmt .` | Format the nix files. The path is required — a bare `nix fmt` passes no files and nixfmt then waits on stdin. |
| `cargo nextest run` | The fast inner loop, inside the dev shell. |
| `cargo sqlx prepare --workspace` | Regenerate `.sqlx/` after changing a query. The build reads it offline, so a stale directory fails to compile. |

CI enumerates `checks` from the flake, so adding a check there adds a CI job
with no workflow edit.

## Changing the toolchain

The Rust version is pinned in `rust-toolchain.toml`, and `flake.nix` records a
hash of it. Changing one without the other fails the build. To bump it:

1. Edit `rust-toolchain.toml`.
2. Set `sha256` in `flake.nix` to `lib.fakeHash`.
3. Build. Paste the hash nix reports back into `flake.nix`.

## Adding a crate

1. Create it under `crates/`.
2. Add it to `[workspace.dependencies]` in the root `Cargo.toml`, and depend on
   it from the crates that need it — respecting the direction above.
3. Add it to the `src` fileset and to `crateRings` in `flake.nix`, and give it a
   `buildPackage` block and an entry in `checks` if it should be built alone.

Step 3 is easy to forget: cargo will be perfectly happy while nix silently
ignores the new sources.

## Working on this on a small machine

A clean build compiles ~300 dependencies. If the editor and rust-analyzer are
also resident, four parallel jobs is enough to exhaust memory on an 8 GiB box —
`/proc/pressure/memory` is the place to look, not the OOM log, because the
machine stalls long before anything is killed.

`nix flake check` rebuilds the whole tree in its own sandbox, separately from
`target/`, so it is the expensive one:

```bash
nix flake check --cores 2 --max-jobs 1
CARGO_BUILD_JOBS=2 cargo nextest run     # the cheap inner loop
```
