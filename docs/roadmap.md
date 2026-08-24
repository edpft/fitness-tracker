# Roadmap

**Goal**: author and prescribe a test week for the week commencing Monday 14
September 2026, using `fitness` as an installed binary rather than
`./target/debug/fitness` in this checkout.

**Written**: 2026-08-24. Twenty-one days to the goal.

This is a living document. It records what is planned, in what order, and why —
so that a session picking the work up cold knows what was decided and what is
still open. Decisions that have actually been made live in `docs/decisions/`;
this is the plan, not the record.

---

## The uncomfortable observation

**The capability already exists.** A test programme can be authored today
(decisions 0013 and 0016), prescribed, and delivered to Hevy. Nothing on the
list below is required to hit the goal.

What is missing is not features but **correctness and ergonomics**: week numbers
that are wrong in delivered routines, a base fill that misstates what a whole
block prescribed, and four commands to do one thing twice a week. The risk to
the goal is not that the tool cannot do it — it is that the tool is annoying or
subtly wrong in ways that make the operator stop using it.

So the order below puts correctness first and new capability second. That is a
deliberate inversion of the order these were designed in.

---

## Where things stand

Shipped, on `main`:

- Hevy extraction into raw, and normalisation into the gym domain
- Prescription: linear ladder, block periodisation, standalone tests,
  succession between programmes
- Delivery to Hevy — one routine per issued prescription, created never updated
  (decision 0017)
- XDG paths: the store at `$XDG_DATA_HOME/fitness-tracker/store.db`, settings at
  `$XDG_CONFIG_HOME/fitness-tracker/config.toml`
- `fitness init` — creates both, connects a source, reports what is outstanding
- Credentials in `credentials.toml`, `0600`, beside the settings

In flight:

- **#22** — schedule types: the operator's week and the holidays that depart
  from it. Types only, no store, no consumer.
- **#10** — release-please's release PR, deliberately **not** merged. See below.

---

## The first release

**1.0.0 is the version that programmes the autumn block**, and nothing before
it. Everything until then is development and beta testing, which is what the
summer block has been.

So #10 stays open and accumulates. Release-please keeps it current as commits
land; merging it is a deliberate act taken when the tool is ready for a block
the operator intends to run on it, not something done because conventional
commits have piled up.

Until then `nix profile install` tracks `main` and `nix profile upgrade` takes
whatever has merged. During beta that is the point rather than a hazard — the
operator wants the fixes he has just asked for. It stops being the point the
moment a block is running on it, which is exactly when the release gets cut.

---

## Order

### 0. Be able to use it standalone — days 1–2

Nothing here is code.

1. `nix profile install github:edpft/fitness-tracker` and verify
   `fitness --version`. This tracks `main`, which is what beta testing wants —
   see **The first release** below for why no version is cut yet.
2. **Move the store.** `local.db` in this checkout holds every authored
   programme and every prescription. Copy it to
   `~/.local/share/fitness-tracker/store.db`. Starting fresh instead is
   survivable — `extract` and `normalise` rebuild raw and normalised from Hevy —
   but the authored side does not come back (§ 12: nothing regenerates it).
3. Verify `fitness prescribe` and `fitness deliver` from the installed binary.

**Acceptance**: a delivered routine appears in Hevy, from a binary on `PATH`,
with the repository checkout untouched.

### 1. Correct what is already being delivered — days 2–5

Both of these are live inaccuracies in routines the operator is training from.

1. **`programme.toml` start → 2026-07-06.** The block began with the light
   session after the 3 July test, not 3 August. Delivered routines currently say
   "week 4" for what is week 7. Loads are unaffected — the ladder walks performed
   gating sessions, not the week index — so this is a numbering fix, not a
   loading one.
2. **Slot amendments.** A dated, reasoned change to one slot's fill, from a
   week onward — the shape agreed on 2026-08-23. `triceps` is currently changed
   at the *base*, which claims the whole block prescribed a dumbbell when the
   first three sessions were on the cable. Harmless only because nothing before
   24 August was ever delivered.

   One constraint survives from a longer discussion: **an amendment may not take
   effect at or before the last performed session.** That is the whole of "the
   past cannot change".

**Acceptance**: a delivered routine names the right week, and `programme.toml`
states the triceps change as an amendment from week 5 rather than at the base.

### 2. Make the daily loop one command — days 5–8

The operator runs this twice a week for three weeks before the goal.

1. **Porcelain.** `prescribe` implies `extract` and `normalise`, with a flag to
   opt out. The commands are lower-level than the job: in the normal flow there
   is no reason not to run all three.

   Delivery stays a separate command. § 36 wants a source being unreachable to
   leave a good prescription in the store, and folding them together makes one
   exit code answer for a programme problem and a network problem alike.

2. **Redelivery**, which reshapes decision 0017 rather than extending it. If a
   prescription is only in force once *delivered*, an undelivered one can be
   re-derived freely and its routine updated in place — `PUT`, same id, frozen
   once trained from. That removes the duplicate-routine problem entirely and
   makes the routine id a stable key per session. **Decision 0017 says the
   adapter never calls `PUT`; revising it is a decision to take explicitly, not
   a quiet change.**

**Acceptance**: one command takes the operator from nothing to a routine on the
phone.

### 3. The schedule — days 8–18

The seam agreed on 2026-08-24. See #22 for the types and their reasoning.

1. **Store, document and CLI** — `fitness schedule author|show`.
2. **Derivation reads the zone by date.** Today the zone is a single configured
   value, so changing it and re-normalising silently rewrites every workout's
   wall clock. § 13 requires the value in force at the time of the observation;
   § II.3 calls it "a versioned input to deterministic translation". This is a
   real defect, and it is why the zone must be read at *derivation* rather than
   passed at authoring: it applies to workouts performed after the programme was
   written.
3. **`config.toml` deleted.** With XDG adopted the store's location is known, so
   `database` is a flag and an environment variable and nothing else; the zone
   moves into the store. Nothing is left in the file. This undoes part of #19,
   three PRs after it merged, which is the beta loop working rather than churn.
4. **Authoring consults the schedule and records what it derived.** The
   programme is *told* its start, its weeks and its slots, and *reads* which of
   those slots it loses. It then stores the result, so derivation never
   re-consults and a prescription stays reproducible once the holiday is off
   anyone's calendar — the pattern decision 0013 already uses for a test's
   inherited fills.

   `interruptions` in `programme.toml` becomes derived, with the ability to
   state one as an override.

**Acceptance**: given the operator's absences as facts, authoring derives the
same interruption list he currently writes by hand.

### 4. Autumn — days 18–21

Author the test week for w/c 14 September, and the block that follows it.

Note that Monday 14 September falls inside the Rome absence, so the test week
loses its Monday session. The test itself is Friday 18 September.

---

## Deliberately not in the twenty-one days

- **The programme setup wizard.** It is the biggest piece and the least
  bounded, and step 3 changes the document it would write. Authoring autumn from
  a hand-written document one more time is better than a half-finished wizard
  writing against rules that are still moving. Revisit after the schedule lands.
- **A second data source.** Withings body weight is the strongest candidate —
  the degenerate entity § II.3 names, and it would exercise § 6's comparability
  classes, which nothing has touched. The architecture claims source
  independence and has never been tested against a second source, so this gets
  more expensive to discover the longer it waits. It competes for the same
  weeks and does not help the operator train.
- **The macro layer** — Peloton, nutrition, the family calendar. Slots are
  recorded but nothing allocates them. See the note on fact versus planning
  below.

---

## Open questions

1. **Where do schedules and patches get authored?** A document like
   `programme.toml`, or commands? A document was assumed in #22's plan but not
   settled.
2. **Does `--timezone` survive as a per-run override** once the store answers?
   Probably, and it should stop being *required*.
3. **Is the wizard a must-have for autumn or a nice-to-have?** Asked on
   2026-08-24 and not yet answered. The plan above assumes nice-to-have.
4. **Should the credential be obtainable by running a command** —
   `key_command = "pass show hevy/api"`, git's `credential.helper` model? It
   makes the config safe to commit by design and works with whatever the
   operator already uses. An OS keyring was considered and rejected as the *only*
   mechanism: it needs an unlocked session, which fails under cron, over SSH and
   in containers.
5. **§ 12 and unperformed prescriptions.** § 12 says authored data "keeps its
   history" because "nothing regenerates it if lost". That premise is false for
   an unperformed prescription, which re-derives exactly. Raised twice and not
   settled; it is not blocking, because the schedule work gives immutability
   structurally.

---

## Risks

- **The store is the only copy of authored data — and right now that is cheap.**
  Raw landing re-fetches from Hevy and everything derived rebuilds; programmes
  and prescriptions do not. § 12 calls that a primary input with no way back.

  But the programmes and prescriptions currently in `local.db`, and the contents
  of `programme.toml`, are **beta-testing artefacts**. Losing them costs a
  re-extract and a few minutes of re-authoring, not a fact. So do not be
  precious with this store: migrating it, starting fresh, or re-authoring
  against a changed document are all fine, and none of them needs a ceremony.

  What makes this a real risk is the autumn block, when the authored side stops
  being disposable. A backup wants to exist by then, not before.
- **`prescribing::deliver` hardcodes `catalogue::source("hevy")`.** The one
  place the tool is genuinely coupled to a vendor. It should be a `--to`
  argument or derived from the programme.
- **Tracking `main` during a block.** Beta testing wants the latest, but once
  the autumn block is running the operator should be on the pinned release. The
  transition is the release itself, and forgetting to make it is the risk.

---

## Two things a new session should read first

- `.specify/memory/constitution.md`, which governs. It is short and binding.
- `CLAUDE.md`, for the way of working. Spec Kit is retired; `specs/` is history.

And two framings settled in conversation that are not otherwise written down:

**Programming is a function of stated inputs, not a consulter of sources.** The
tool is told to generate a programme from x to y including absences a and b. The
brain that decides what the absences *are* sits above the gym level, because it
also weighs cycling, nutrition and the family calendar.

**Recording a fact is not coordinating.** `fitness` is not a gym-only tool; the
gym parts are simply what exists so far. So the line is not gym-versus-macro but
fact-versus-planning: recording that the operator can train on Monday evening is
data this tool should hold, while allocating that slot between the gym and
cycling is planning and waits.
