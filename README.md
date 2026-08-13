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
| `cli` | Driving adapter, composition root | The operator's entry point: `fitness extract`, `status`, `reset`. Extraction is invoked from a terminal or an external scheduler, never over HTTP. |
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

## Running extraction

```bash
export HEVY_API_KEY=...          # from https://hevy.com/settings?developer
export FITNESS_TRACKER_DATABASE=./local.db

fitness extract hevy             # collect since the resumption point
fitness status                   # when extraction last succeeded
fitness reset hevy               # discard the position; next run collects everything
```

| Variable | Flag | Required | Default |
| --- | --- | --- | --- |
| `HEVY_API_KEY` | *none, deliberately* | yes | — |
| `FITNESS_TRACKER_DATABASE` | `--database` | yes | — |
| `HEVY_API_BASE_URL` | `--base-url` | no | `https://api.hevyapp.com` |

The credential has no flag. A secret on the command line lands in shell history
and in `ps` output; put it in the environment or an untracked `.env`.

Exit codes are part of the contract, so an external scheduler can tell outcomes
apart: `0` success (including a run that found nothing), `1` the source was
unreachable or rejected the credential, `2` another run is in progress, `3` the
store, `4` usage.

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
