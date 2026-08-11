# Contract: command surface

The `cli` crate's public interface — the contract with the operator and with
any external scheduler (§ 34: extraction is invoked, never self-triggering).

Binary: `fitness`.

## Configuration

Supplied by environment or local config, never committed (FR-009, § 35).
`.env` is already gitignored; the `secrets` flake check is the backstop.

| Variable | Flag | Required | Default |
| --- | --- | --- | --- |
| `HEVY_API_KEY` | — | yes | none |
| `HEVY_API_BASE_URL` | `--base-url` | no | `https://api.hevyapp.com/v1` |
| `FITNESS_TRACKER_DATABASE` | `--database` | yes | none |

`HEVY_API_KEY` has no flag. A credential passed on the command line lands in the
shell history and in `ps` output.

## Commands

### `fitness extract hevy`

Collect everything the source has served since the resumption point, and land
it. The whole of user story 1.

```
$ fitness extract hevy
run 4 started
  page 1/17 … 10 events, 10 landed
  …
  page 17/17 … 4 events, 4 landed
run 4 succeeded: 164 events seen, 164 records landed
resumption point advanced to 2026-08-10T19:29:47.199Z
```

A repeat run against unchanged data (FR-005, SC-002):

```
$ fitness extract hevy
run 5 started
run 5 succeeded: 0 events seen, 0 records landed
resumption point unchanged at 2026-08-10T19:29:47.199Z
```

`events seen` and `records landed` are always both reported. A run that saw 40
events and landed 0 is not the same as one that saw none, and neither is a
failure (FR-011).

### `fitness status`

The most recent successful extraction per source (FR-008, § 38). Broken
ingestion is visible rather than silent.

```
$ fitness status
source  last succeeded          events seen  records landed  landing records
hevy    2026-08-11T18:19:59Z             0               0              164
```

Exits `0` whether or not any run has succeeded. Never having run is a fact to
report, not an error:

```
$ fitness status
source  last succeeded          events seen  records landed  landing records
hevy    never                             -               -                0
```

### `fitness reset hevy`

Discard the resumption point so the next extraction collects the full history
(FR-007).

```
$ fitness reset hevy
resumption point for hevy cleared; the next run collects the full history
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
| `4` | Usage error — missing configuration, unknown source |

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
