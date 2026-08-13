# Contract: command surface

The `cli` crate's public interface — the contract with the operator and with
any external scheduler (§ 34: extraction is invoked, never self-triggering).

Binary: `fitness`.

## Configuration

Supplied by environment or local config, never committed (FR-009, § 35).
`.env` is already gitignored; the `secrets` flake check is the backstop.

| Variable | Flag | Scope | Required | Default |
| --- | --- | --- | --- | --- |
| `<SOURCE>_API_KEY` | — | the named source | yes, to extract | none |
| `<SOURCE>_API_BASE_URL` | `--base-url` | the named source | no | the source's API root |
| `FITNESS_TRACKER_DATABASE` | `--database` | global | yes | none |

The first two are named after whichever source the invocation names, so for
`hevy.workouts` they are `HEVY_API_KEY` and `HEVY_API_BASE_URL`. Neither is
global: a credential and an API root belong to the system that issued them, and
a second source brings its own rather than sharing Hevy's flag. `--base-url`
exists only on `extract`, which is the only command that contacts a source.

The API root carries no version segment (`https://api.hevyapp.com`). The
adapter owns the path it calls; a base that already ends in `/v1` composes
`/v1/v1/workouts/events`, which is exactly what a live run once did.

`<SOURCE>_API_KEY` has no flag. A credential passed on the command line lands in
the shell history and in `ps` output.

## Naming a stream

Every command takes one required argument: the landing stream, written
`source.entity` — the same text the system prints back. `hevy` is a source, not
a stream, and is refused: a source serving two kinds of thing has two
resumption points and neither is the default.

Which streams a build collects is its catalogue, and an unknown one is refused
with the list:

```
$ fitness status strava.rides
fitness: unknown stream "strava.rides"; this build collects hevy.workouts
```

## Commands

### `fitness extract <stream>`

Collect everything the source has served since the resumption point, and land
it. The whole of user story 1.

```
$ fitness extract hevy.workouts
extracting hevy.workouts …
run 4 succeeded: 164 events seen, 164 records landed
resumption point advanced to 2026-08-10T19:29:47.199Z
```

No per-page reporting: how a source instalments its answer is the adapter's
business and does not cross the port. A run reports what it saw and what it
landed.

A repeat run against unchanged data (FR-005, SC-002):

```
$ fitness extract hevy.workouts
extracting hevy.workouts …
run 5 succeeded: 0 events seen, 0 records landed
resumption point unchanged at 2026-08-10T19:29:47.199Z
```

`events seen` and `records landed` are always both reported. A run that saw 40
events and landed 0 is not the same as one that saw none, and neither is a
failure (FR-011).

### `fitness status <stream>`

The most recent successful extraction for that stream (FR-008, § 38). Broken
ingestion is visible rather than silent. Needs no credential and no network.

```
$ fitness status hevy.workouts
stream           last succeeded         events seen  records landed  records held
hevy.workouts    2026-08-11T18:19:59Z             0               0           164

resumption point: 2026-08-10T19:29:47.199Z
```

Exits `0` whether or not any run has succeeded. Never having run is a fact to
report, not an error:

```
$ fitness status hevy.workouts
stream           last succeeded         events seen  records landed  records held
hevy.workouts    never                            -               -             0

resumption point: unset — the next run collects the full history
```

### `fitness reset <stream>`

Discard the resumption point so the next extraction collects the full history
(FR-007).

```
$ fitness reset hevy.workouts
resumption point for hevy.workouts cleared (was 2026-08-10T19:29:47.199Z); \
the next run collects the full history
nothing was landed and nothing was removed
```

Lands nothing and removes nothing from raw. The subsequent full run re-serves
every payload, and FR-005 means identical payloads land no records — which is
acceptance scenario 6.

## Exit codes

Part of the contract: an external scheduler distinguishes outcomes by these.

| Code | Meaning |
| --- | --- |
| `0` | Success, including a run that found nothing |
| `1` | Source unavailable or unauthorised — raw unchanged, resumption point unmoved (scenario 7, § 36) |
| `2` | Another run is already in progress (FR-010) |
| `3` | Store unavailable or corrupt |
| `4` | Usage error — missing configuration, unknown or malformed stream |

`std::process::exit` is unavailable: `clippy::exit` is `forbid`, and no
`#[allow]` can rescue it (E0453). `main` returns `std::process::ExitCode`.

## Guarantees

- **Idempotent.** Running `extract` twice with no intervening source change
  lands nothing the second time (FR-005, SC-002).
- **Interruptible.** `SIGINT` or a crash mid-run leaves landed pages durable
  and the resumption point unmoved. The next run reaches the same end state
  (FR-006, SC-004).
- **Single-flight.** A second concurrent `extract` exits `2` immediately,
  landing nothing and moving nothing (FR-010).
- **Non-destructive.** No command removes or alters a landing record. The store
  enforces this independently of the code (D6).
