# PROJECT_NAME

<!-- Replace this file's contents once the template has served its purpose. -->

A Rust workspace laid out for hexagonal architecture, built with
[crane](https://github.com/ipetkov/crane) over a
[fenix](https://github.com/nix-community/fenix) toolchain.

## Layout

Dependencies point inward only. Nothing in an inner ring may name anything in
an outer one:

```
web ──▶ infrastructure ──▶ application ──▶ domain
```

| Crate | Role | What belongs here |
| --- | --- | --- |
| `domain` | The core | Entities, value objects, and the rules that govern them. Depends on nothing — not on `application`, and never on a framework, database, or transport. |
| `application` | Use cases and ports | The things your software *does*, and the traits it needs the outside world to satisfy. Ports are declared here, in the application's own vocabulary, and implemented further out. |
| `infrastructure` | Driven adapters | Implementations of the driven ports: a database, an HTTP client, a filesystem. The one place a technology choice is allowed to show. |
| `web` | Driving adapter, composition root | Translates requests into use-case calls, and is the only crate that names a concrete adapter. |

Two consequences worth keeping in mind:

- **Errors get translated at each boundary.** `application::RepositoryError`
  carries no SQL codes and no HTTP statuses, so a change of database cannot
  ripple inwards.
- **Use cases are generic over their ports.** `ItemService<R>` takes any
  `ItemRepository`, which is why its tests use a fake and touch no I/O. If a
  test needs a database, the dependency is pointing the wrong way.

The example `Item` type exists only to demonstrate the shape. Delete it.

## Commands

| Command | Does |
| --- | --- |
| `nix develop` | Enter a shell with the toolchain and every tool CI uses. `direnv allow` does this automatically. |
| `nix flake check` | Run everything CI runs: per-crate builds, rustfmt, clippy, tests, doctests, `cargo audit`, `cargo deny`, and a nix formatting check. |
| `nix build` | Build the `web` binary into `./result`. |
| `nix run` | Build and run it. |
| `nix fmt` | Format the nix files. |
| `cargo nextest run` | The fast inner loop, inside the dev shell. |

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
3. Add it to `workspaceSrc` in `flake.nix`, and give it a `buildPackage` block
   and an entry in `checks` if it should be built in isolation.

Step 3 is easy to forget: cargo will be perfectly happy while nix silently
ignores the new sources.
