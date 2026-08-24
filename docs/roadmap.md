# Roadmap

**Goal**: author and prescribe a test week for the week commencing Monday 14
September 2026, using `fitness` as an installed binary rather than
`./target/debug/fitness` in this checkout.

**Written**: 2026-08-24, revised the same day.

The dates below are ordering, not estimates. The constraint on this work is how
fast decisions get made, not how fast code gets written.

This is a living document. It records what is planned, in what order, and why —
so that a session picking the work up cold knows what was decided and what is
still open. Decisions that have actually been made live in `docs/decisions/`;
this is the plan, not the record.

---

## The uncomfortable observation

**Most of the capability already exists.** A test programme can be authored
today (decisions 0013 and 0016), prescribed, and delivered to Hevy — and as of
2026-08-24 the tool is installed and running against its own store rather than
this checkout.

What the goal actually needs is narrow: **a way to record that Monday 14
September is unavailable, and a way for a programme to know which slots it may
use.** Both are the schedule. Everything else on the old list — week numbers, a
base fill that misstates a block, four commands to do one job — is correctness
and ergonomics on a block that ends on 13 September, and none of it is on the
path.

The wizard **is** on the path. An earlier revision of this document argued
otherwise on the grounds that a test inherits its fills; that reasoning applies
to a *standalone* test, and the autumn block is a periodised one, which owns its
entry test and states every slot itself.

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

### 0. Be able to use it standalone — **done**

`fitness` is installed at `~/.nix-profile/bin/fitness`, and the store at
`~/.local/share/fitness-tracker/store.db` has been repopulated by `extract` and
`normalise` — 165 workouts, and nothing authored. `local.db` in this checkout
stays where it is; it is a beta-testing artefact and was not worth copying.

Nothing else is needed here. An earlier revision of this document claimed the
summer block had to be re-authored so a standalone test could inherit its fills.
**It does not** — see step 3, which is one programme rather than two.

### 1. The schedule — store, document, CLI

`fitness schedule add|show|list|remove`, over the types in #22.

Two things get recorded: the **ordinary week** — Monday evening, Wednesday
evening, Friday evening, Sunday morning, in `Europe/London` — and the
**patches**, of which the one that matters is Monday 14 September being
unavailable.

The schedule also becomes the source of the zone, replacing `timezone` in
`config.toml`. That removes a duplicated fact rather than fixing the § 13
defect: derivation still uses the zone in force *now* rather than the one in
force on the date of each workout. No regression, and the full fix is listed
below.

**Acceptance**: `fitness schedule show` prints the ordinary week and the
September patches.

**Command verbs are the standard words** — add, remove, list, change — unless a
domain term is better suited. `programme author` became `programme add` in #24
for exactly this reason: it was the word in a conversation, and a conversational
coinage should not harden into an interface.

### 2. Adding a programme consults the schedule

The programme is *told* its start, its weeks and the slots it may use, and
*reads* which of those it loses. It records the result, so derivation never
re-consults and a prescription stays reproducible once the holiday is off
anyone's calendar — the pattern decision 0013 already uses for a test's
inherited fills.

`interruptions` in a programme document becomes derived, with the ability to
state one as an override.

**Acceptance**: adding the autumn block derives the loss of Monday 14 September
without it being stated.

### 3. The programme setup wizard

**A periodised block owns its entry test.** `BlockWeek::Entry` makes week one
the measurement the rest of the block is a share of, `phase_weeks_of` takes that
week out of the phase count, and the anchor is an *expectation* the test week
confirms rather than a number that must already have been measured.

So the autumn block is **one programme starting Monday 14 September**, not a
standalone test followed by a block on the 21st. There is no predecessor to
inherit fills from, and a fresh periodisation states every slot itself —
seventeen of them, out of a hundred-and-thirty-two exercises. That is the pain
this exists to remove, and it is on the critical path after all.

Scoped to that pain: propose each fill from `docs/slot-candidates.md`, ordered
by what the record shows has been performed but **not limited to it** — the
operator wants to see options he has not done before. The candidate lists are
his, stated on 2026-08-24, and four of them name exercises the vocabulary does
not yet have. The block-level facts — name, template, start, duration,
primary, anchor — are few and short.

**It writes a document and then authors it**, rather than authoring directly.
That keeps one authoring path, and leaves the operator an artefact that is
reviewable, diffable and re-authorable.

### 4. The autumn block — from Monday 14 September

---

## Deferred, and none of it on the critical path

- **The zone read by date at derivation.** The § 13 defect is real — change the
  zone, re-normalise, and every workout's wall clock is rewritten — but it bites
  only if the operator trains in another zone, and he cannot train in Rome. It
  should land before it can bite, not before 14 September.
- **`config.toml` deleted entirely.** Step 1 empties it of the zone; `database`
  is the last thing in it and goes when there is a reason to touch the file.
- **Porcelain** — one command for the daily loop. Worth having for the six
  remaining summer sessions, not worth displacing the schedule.
- **Slot amendments** — needed the next time equipment moves, not before.
- **`programme.toml` start → 2026-07-06** — it corrects week numbers on a block
  about to end, in a document about to be superseded.
- **Redelivery via `PUT`**, revising decision 0017.

## Deliberately out of scope

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
