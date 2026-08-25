# Roadmap

**Goal**: author and prescribe a test week for the week commencing Monday 14
September 2026, using `fitness` as an installed binary rather than
`./target/debug/fitness` in this checkout.

**Written**: 2026-08-24. Revised 2026-08-25, when steps 1 to 3 landed and
decision 0018 changed what comes after them.

The dates below are ordering, not estimates. The constraint on this work is how
fast decisions get made, not how fast code gets written.

This is a living document. It records what is planned, in what order, and why —
so that a session picking the work up cold knows what was decided and what is
still open. Decisions that have actually been made live in `docs/decisions/`;
this is the plan, not the record.

---

## Where this stands on 2026-08-25

**The goal is reachable today.** Steps 1 to 3 have landed; step 4 is authoring
the block, and it has been run end to end against a copy of the real store. What
is left before 14 September is the operator sitting down and doing it.

**The interesting work is now the model rather than the goal**, which is a
better problem to have than the reverse. Decision 0018 is proposed and not
started, and it belongs after the block starts rather than before — see the end.

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

Landed since this was written, and steps 1 to 3 of the order below:

- **#25** the four exercises the autumn slots name; **#26** a bilateral stretch
  is held twice
- **#27** the operator's week, stored and shown — training slots, alterations,
  and which discipline each slot belongs to
- **#28** a programme reads the days it loses from the schedule
- **#29** an alteration asks about its own days; **#30** the programme wizard
- **#31** a prescription is drafted, published, or performed (§ 12.1), and a
  performed workout knows which session it was

In flight:

- **#32** — decision 0018, proposed. A programme counts cycles and the scheduler
  owns the calendar. It rewrites three of the steps below; see *What 0018
  changes* at the end.
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

### 1. The schedule — store, prompts, CLI — **done** (#27, #29)

Authored by prompts rather than a document: `fitness schedule add` asks when
there is room to train, `schedule alter` asks what departs from it, and
`schedule show` reads it back. A pattern is four or five slots and an alteration
is a date, a length and a reason — small enough that a file to hold them would
be a file to lose, and the store is where the object lives.

Each slot also names the discipline it belongs to, because an alteration can
move a slot and has to say whose the new one is. **0018 makes that derived
rather than authored**; see the end.

The zone came with it. `timezone` in `config.toml` has not gone yet — the
schedule holds the zone, and every other command still takes `--timezone`.

*What follows is the original plan, kept because the acceptance line is what was
met.*

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

### 2. Adding a programme consults the schedule — **done** (#28)

**And 0018 deletes it.** A programme that holds no dates has nothing to lose and
nothing to derive, so the interruption machinery this step built — deriving at
authoring, freezing the result, the override, the window that has to cover the
entry-test week — all goes. The work bought the understanding that produced
0018, which is not nothing, but none of the code survives.

*The original plan follows.*

The programme is *told* its start, its weeks and the slots it may use, and
*reads* which of those it loses. It records the result, so derivation never
re-consults and a prescription stays reproducible once the holiday is off
anyone's calendar — the pattern decision 0013 already uses for a test's
inherited fills.

`interruptions` in a programme document becomes derived, with the ability to
state one as an override.

**Acceptance**: adding the autumn block derives the loss of Monday 14 September
without it being stated.

### 3. The programme setup wizard — **done** (#30)

`programme add` with no document asks, writes a document, and authors that. Each
slot offers what `docs/slot-candidates.md` holds for it, ordered by what the
record shows performed and limited by nothing.

**0018 changes what it asks.** `gating_role` and `[programme.weekdays]` leave
the document, because the scheduler derives where the heavy session lands. The
seventeen slots stay.

*The original plan follows.*

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

**Ready now, on the model as it stands.** The wizard writes it, the schedule
takes 14 September out of it without being told, and `prescribe` refuses that
Monday and issues the Friday entry test. Run end to end against a copy of the
real store on 2026-08-25.

Nothing in 0018 is needed for it. The heavy session is on a Friday because the
document says so, and that is the right answer — 0018 would *derive* the same
Friday rather than change it.

---

## What 0018 changes, and when

Decision 0018 — a programme counts sessions, microcycles, mesocycles and
macrocycles, and the scheduler maps them onto the calendar — is a better model
and it rewrites three of the four steps above.

**It is not needed for the autumn block**, and that is the whole of the
scheduling question. The heavy session is on a Friday; 0018 derives that Friday
instead of being told it, and derives the same one. 14 September is already
taken out. There is nothing the block cannot do today that it could do after.

**So it lands after 14 September, not before.** Twenty days is not enough to
remove `Calendar`, `Skip`, `Interruptions`, `WeekKind` and `WeekIndex`, build an
allocator, and still have a block to run — and attempting it is how one arrives
at the 14th with neither. The deadline is what makes this the wrong side of it.

**Mid-block is acceptable, which is what makes waiting safe.** Two things make
it so: a pin and the spacing rule reproduce the operator's week exactly, so a
derived allocation does not move a running block's sessions; and § 12.1 means
issued prescriptions are recorded rather than re-derived, so nothing already
prescribed can change under it. The migration is mostly *dropping* columns.

Order, when it starts:

1. **Commitments.** Purely additive — padel is recorded, nothing reads it yet.
   The scheduler cannot allocate correctly without them.
2. **The ordinal programme.** Sessions, microcycles, mesocycles; `Calendar` and
   the interruption machinery go.
3. **The allocator.** Pin, alternation, spacing; `gating_role` and
   `[programme.weekdays]` leave the document.

Nothing is half-migrated between those: the current calendar keeps working until
the thing replacing it can place a session.

## What is left of the lifecycle, and it survives 0018

#31 landed the states and the link. Three pieces remain, and all three are work
0018 relies on rather than work it discards — it says what a performance *was*
is decided by the session it fulfilled, which is exactly this link:

- **A performance takes its role from its prescription, not its date.** The
  Friday session performed on Saturday morning. `place(performance.on)` is
  wrong today and 0018 deletes it, but the replacement is this.
- **Withdrawal** for a published, unperformed session — delete the routine at
  the source and drop the delivery row. Needs `ON DELETE CASCADE` on
  `prescribed_item` and its children, which is why a draft is not disposable
  today despite § 12.1 saying it is.
- **The comparison** — performed against prescribed, which `project` can already
  do and nothing calls.

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
- **The macro layer** — nutrition, the family calendar, and anything that
  *decides* how a week is spent. Slots are recorded **and allocated**, because
  the allocation turned out to be a fact rather than a plan; what waits is
  choosing it. See the note on fact versus planning below.

---

## Open questions

1. ~~**Where do schedules and patches get authored?**~~ **Answered**: prompts,
   straight to the store. No document.
2. **Does `--timezone` survive as a per-run override** once the store answers?
   Probably, and it should stop being *required*.
3. ~~**Is the wizard a must-have for autumn?**~~ **Answered**: must-have, and
   built (#30).
4. **Should the credential be obtainable by running a command** —
   `key_command = "pass show hevy/api"`, git's `credential.helper` model? It
   makes the config safe to commit by design and works with whatever the
   operator already uses. An OS keyring was considered and rejected as the *only*
   mechanism: it needs an unlocked session, which fails under cron, over SSH and
   in containers.
5. ~~**§ 12 and unperformed prescriptions.**~~ **Answered** by § 12.1 and #31: a
   prescription is drafted, published or performed, and "nothing regenerates it
   if lost" is true of only the last.

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
data this tool should hold.

**The allocation is on the fact side, and that was revised on 2026-08-25.** This
document said allocating a slot between the gym and cycling was planning, and
waited. It is not: *deciding* the split is planning, and still waits, but
*which discipline holds Monday evening* is a fact the schedule has to hold —
because an alteration can move it. A trip where the hotel gym is only free at
the weekend turns two weekday evenings into a Saturday morning, and the
allocation has to move with them. Anything holding the allocation elsewhere
would need to know about alterations too, which is the knowledge this module
exists to keep in one place.

So `Diary::unavailable` takes a discipline and reads the allocation, rather than
taking a set of slots somebody else kept in step. What the tool still will not
do is choose the split — that weighs cycling, nutrition and the family calendar,
and sits above the gym level.
