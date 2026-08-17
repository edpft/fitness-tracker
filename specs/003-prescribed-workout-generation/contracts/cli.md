# Contract: CLI

Two new commands and one addition to an existing one. Built with clap's **builder
API**, not the derive macros — the derive macros generate
`#[allow(clippy::unwrap_used)]`, and panics are `forbid`, so the attribute is a
compile error (E0453).

`--database` is inherited on every command, as it already is, with
`FITNESS_TRACKER_DATABASE` behind it and no path compiled in (§ 34).
`--timezone` / `FITNESS_TRACKER_TIMEZONE` likewise, and with no default: the zone
decides what "the session of 2026-08-17" means, and a default would be an
assumption about where the operator trains.

---

## `fitness prescribe`

```text
Issue the prescription for a date, or show what was already issued

Usage: fitness prescribe [OPTIONS]

Options:
      --date <date>          The session to prescribe for, as YYYY-MM-DD.
                             Defaults to the next programmed day at or after today
      --database <database>  Where the store lives. No path is compiled in
                             [env: FITNESS_TRACKER_DATABASE=]
      --timezone <timezone>  IANA identifier. No default is compiled in
                             [env: FITNESS_TRACKER_TIMEZONE=]
  -h, --help                 Print help
```

**`--date` defaults forward, not to today.** "The next session" is what the operator
wants on a rest day, and today is what they want on a training day; taking the next
programmed day at or after today gives both. It prints the date it resolved, so the
default is never silent.

### Output

```text
prescribing 2026-08-17 (Monday, light) — week 6 of 8, ladder at 100%
anchor 90kg (tested, from 2026-07-03), history through 2026-08-14

  plyometric
    pogo                            3 × 20
  power
    box jump                        3 × 5
  strength
    front squat  [primary]          35 × 4  (warm-up)
                                    55 × 3  (warm-up)
                                    70 × 2  (warm-up)
                                    80 × 1  (warm-up)
                                    80 × 3
                                    67.5 × 6      × 3 sets
    pull-up      ┐ superset         bodyweight × 6, 1-2 in reserve
    chest dip    ┘                  -7 × 6, 1-2 in reserve
    nordic hamstring curl           3 × 4
  hypertrophy
    preacher curl        ┐ superset 32.5 × 5-6
    overhead triceps ext ┘          30 × 5-6
    wrist extension      ┐ superset 7 × 6
    palms-up wrist curl  ┘          10 × 6
    cable twist                     30 × 6
  mobility
    handstand hold                  60s
    dead hang            ┐ superset 45s
    couch stretch        ┘          2 × 60s
    ninety-ninety                   2 × 60s
    stretching                      60s

issued as prescription 12
```

The loads above are **illustrative**, not a fixture. The primary's numbers depend on
the ladder's span, which is not yet authored (research D8) — the 100% shown here is
what a 92.5%→105% ladder would reach in week 6 of 8, putting the light top set at
88.5% of 90kg.

**What the header carries and why.** The resolved date and weekday, so the default
is visible; the role, so a wrong weekday mapping is obvious; the week and ladder
percentage, so the operator knows where they are in the block and against what; the
anchor with its provenance and date, so the number the whole session hangs on is
never implicit; and `history through`, which is § 38 — a prescription derived from
history that stops four days before the last session is visibly stale.

**Re-running for a date already issued** prints the same workout with
`already issued as prescription 12` in place of the final line, and issues nothing
(FR-010).

**An underivable slot** prints in place, and the command still succeeds:

```text
    sissy squat                     — not derivable: never performed, and the
                                      programme sets no starting load
```

**Exit codes.** `0` for a prescription issued or re-shown, including one with
underivable slots. `1` for no programme, no parameters, a date on no programmed
weekday, or an unavailable store.

**A date on no programmed weekday** names what the programme does run:

```text
fitness: 2026-08-19 is a Wednesday; this programme runs Monday (light) and
Friday (heavy). Pass a date on one of those, or author a programme that runs
Wednesdays.
```

That is D7's decision made visible: no implicit nearest match, because silently
prescribing Friday's session for a Wednesday is worse than declining.

---

## `fitness programme`

```text
Author or inspect the programme in force

Usage: fitness programme <COMMAND>

Commands:
  author  Read a programme document and store it, superseding the previous one
  show    Print the programme and parameters in force
```

### `fitness programme author <path>`

Reads the TOML document described in [programme.md](./programme.md), converts it
into domain types, validates what the types cannot (the three checks in
[ports.md](./ports.md)), and stores the programme and its parameters with the
authoring date.

**Supersedes rather than overwrites** (§ 12). `show` reads the latest; every
earlier version stays. An issued prescription names the parameter version it used,
so a superseded row is never consulted but also never lost.

```text
authored programme 3 — front squat, knee-dominant primary, 8 weeks from
2026-07-06, gating on the heavy session
anchor 90kg (tested 2026-07-03), fixed for the block
parameters authored 2026-08-17T19:20:04+01:00
  ladder 92.5% → 105% of anchor over 7 climbing weeks, +2.08%/week derived
  week 8 is the test
  heavy top set 1 rep; light top set 3 reps at 88.5% of the heavy load
  warm-up 40/60/80/90% of top set at 4/3/2/1 reps
  back-off 85% of top set, plate increment 2.5kg
  reset 1 −10% at +5kg/week; reset 2 −5% at +2.5kg/week
```

Validation failures name the field and what was expected. A document that parses
but is inconsistent — gating on a role the programme never runs — fails here rather
than at the first `prescribe`.

### `fitness programme show`

Prints the same block as `author`, plus the slot fills and the whole ladder with
the position reached and how it got there:

```text
anchor 90kg (tested 2026-07-03) — fixed for this block

week  ladder   heavy   light   state
  1    92.5%    82.5    72.5   completed
  2    94.6%      85      75   completed
  3    96.7%    87.5    77.5   completed
  4    98.8%      90    79.5   failed, held
  5    98.8%      90    79.5   failed → reset 1
  6      —         80      —   re-climbing (+5kg/week)
  7      —       85       —    re-climbing
  8     test
```

The ladder is derived, so this table is the audit trail for every load in the
block. Printing it whole is what makes a derived plan checkable — a derived number
nobody can check is not better than a stored one, and the two arithmetic errors
this feature exists to prevent were exactly numbers nobody checked.

---

## `fitness status`

Gains a third section, beside extraction and derivation:

```text
prescription
  programme        3, front squat, week 6 of 8
  anchor           90kg (tested, from 2026-07-03), fixed for the block
  ladder           at 100% of anchor; endpoint 105% in week 7
  last issued      2026-08-14 (prescription 11)
  next programmed  2026-08-17 (Monday, light)
```

§ 38 again: the point is that a programme which has stopped issuing is visible
rather than merely absent.

---

## What the CLI does not gain

- **No `--role` or `--week` override, and no `--anchor`.** The first two are
  derived and a flag would let the operator prescribe a heavy session on a light
  day. The third is authored, and overriding it for one invocation would produce a
  prescription no stored programme accounts for. If a derived value is wrong the
  programme is wrong, and that is the thing to fix.
- **No `--dry-run`.** Issuing is idempotent per date, so re-running is the dry run.
- **No routine export.** Out of scope; the prescription reaches the phone by the
  operator reading it.
- **No `prescribe` entry in `cli::catalogue`.** The catalogue is one entry per
  *stream* — what this build can collect. Prescription collects nothing, so an
  entry there would be a category error, and no `HEVY_`-style environment
  derivation applies to it.
