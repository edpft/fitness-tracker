# Roadmap

**Goal**: author and prescribe a test week for the week commencing Monday 14
September 2026, using `fitness` as an installed binary rather than
`./target/debug/fitness` in this checkout.

**Written**: 2026-08-24. Revised 2026-08-25, when steps 1 to 3 landed and
decision 0018 changed what comes after them. Revised again 2026-08-27, when the
operator ran the wizard end to end and it asked three questions it had no
business asking. Revised again 2026-08-29, when the operator found that
`prescribe` was not reading the workout he had just landed — decision 0021 — and
2026-08-30, when fixing that stranded a session at the destination and decision
0022 answered it with `PUT`.

The dates below are ordering, not estimates. The constraint on this work is how
fast decisions get made, not how fast code gets written.

This is a living document. It records what is planned, in what order, and why —
so that a session picking the work up cold knows what was decided and what is
still open. Decisions that have actually been made live in `docs/decisions/`;
this is the plan, not the record.

---

## Now

The three-line version, kept current so a session starting cold does not have to
read the rest to know what to pick up.

- **Just landed**: the cycling domain and `fitness cycling next`. All
  twenty-five sessions of *Peak Your Power Zones* are transcribed into
  `domain::cycling::seed`, checked against the app's own stated ride durations
  and movement counts for every one of them. The command prints the session, its
  zones, what each zone means in watts at a given FTP, the Peloton class that
  realises it, and a warning on the one class the operator's account cannot
  start. Decision 0025 has the model; `docs/cycling-peak-your-power-zones.md`
  has the data.

  **Not built, and named rather than implied**: Peloton as a source and a sink.
  Decision 0025 settled that it should be both and that a session should ideally
  be scheduled into the operator's Peloton calendar. Until then cycling
  prescribes and stops, which is why it is not a `KnownDiscipline` — a catalogue
  entry pointing at streams that do not exist would make the shape look finished.

  **Also not built**: persistence. The programme start and the FTP arrive as
  arguments, because the store holds neither yet. The FTP has no default and
  never will have one — a session prints its zones without it and its watts only
  when told what a zone is a share of.

- **Retired as authority**: `specs/`. 8,244 lines of agent-authored prose the
  operator has never read, and a session had just quoted `specs/003`'s
  out-of-scope list back at him as a reason cycling was not on the path.
  Concurrent gym and cycling programming is the point of the tool. `CLAUDE.md`
  withdraws it; deletion is pending — see 0024.
- **Just landed**: `fitness gym next` — the porcelain. One command for the daily
  loop, nested by discipline because that is the level at which a pipeline has
  one source and one sink. The four plumbing commands are untouched; it wraps
  them, and each step still reports its own outcome and its own exit code.
  `PUT` came first and was the right order: it removed the stale-session warning
  the wrapper would otherwise have had to find somewhere to put.

  **Withdrawal is no longer the piece that matters.** 0022 took its case away —
  a superseded session is replaced in place rather than left behind — so what is
  left for it is removing a session nobody replaces, which nothing yet asks for.

- **Just landed**: the SBS gym half, end to end. `template = "sbs"` authors a
  four-week cycle, it stores and reads back, and both sessions of a week issue
  what the chart states. `domain::prescription::sbs` splits into the published
  chart and the programme built around it.

  **The new shape it needed**: a working set that carries *no load*. The chart
  says `3 × 5–6 @ 8RM`, and that load is the result of the set above it, so it
  does not exist when the prescription is written. `PrimaryLoad::RepMax` issues
  the top set as an attempt and the back-offs as repetition ranges with nothing
  in the load column.

  **And the maximum moves off the record**, which is the mechanism rather than
  a refinement: each performed rep-max day is read, run through SBS's table, and
  applied in order, so week 3 is a share of what week 2 produced. A week nobody
  trained leaves it where it was, and a failed attempt advances nothing.

- **Parked**: `feat/block-derives-from-prilepin` (decision 0023). Finished,
  green, never raised as a PR, and it stays unraised. It was prescribing `8 × 2`
  to stay inside Prilepin's bands — the degeneracy 0023 itself documented as an
  unresolved open question, and then prescribed anyway. That open question is
  closed by nothing asking it any more. The `WorkUp` variant it added survives
  and is what SBS's rep-max days are built on.
- **After 14 September**: decision 0018, in the order commitments → ordinal
  programme → allocator. Not before; see *What 0018 changes, and when*.

## Where this stands on 2026-08-27

**The goal is reachable today**, and has been since the 25th. What changed since
is that the operator ran the wizard for real, and the run found three questions
worth removing rather than anything broken — a block was authored at the end of
it either way.

**The lesson generalises, and 0019 and 0020 both state it**: a question is worth
asking only if the operator is the one who knows the answer. The wizard asked
for training days the schedule already held, for a week count the end date
already fixed, and offered no choice of template while the document reader had
read three of them all along. None of that was visible from the types; it took a
transcript.

**The interesting work is still the model rather than the goal**, which is a
better problem to have than the reverse. Decision 0018 is accepted and not
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

Landed on 2026-08-26 and 2026-08-27, after the wizard met a real operator:

- **#32** decision 0018, accepted — a programme counts cycles and the scheduler
  owns the calendar. It rewrites three of the steps below; see *What 0018
  changes* at the end.
- **#33** setup seeds the generation parameters and shows them; **#36** a pigeon
  stretch for external hip rotation
- **#35** decision 0019 — the wizard asks dates and intents and derives the plan
- **#37** the 1.0.0 release, backed out. See below.
- **#38** decision 0020 — the schedule says which days are the gym's, the dates
  decide the weeks, and a span too long for one block is refused rather than
  split

In flight:

- **#39** — the wizard authors a test and a ladder, not only a block. The
  document reader has read all three templates since they existed; the wizard
  reached one of them, so the only route to the other two was the hand-written
  document it exists to replace.
- **the release PR** — release-please's, deliberately **not** merged. See below.

---

## The first release

**1.0.0 is the version that programmes the autumn block**, and nothing before
it. Everything until then is development and beta testing, which is what the
summer block has been.

So the release PR stays open and accumulates. Release-please keeps it current as
commits land; merging it is a deliberate act taken when the tool is ready for a
block the operator intends to run on it, not something done because conventional
commits have piled up.

**This has been got wrong once.** #10 was merged on 2026-08-26, tagging `v1.0.0`
and publishing a release, and the next release PR then proposed 2.0.0 off a
`feat(gym)!`. It was backed out: the commit reverted, the tag and the release
deleted, the standing release PR closed so release-please regenerates from
0.1.0. The rule above is the whole reason the merge was wrong, which is why the
rule now says which PR number to leave alone rather than assuming it is obvious.

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

**0019 changed what it asks**, after the operator ran it end to end on
2026-08-26. A block is stated by its dates and the phases are derived from them;
the primary is knee or hip dominant and not four patterns; and the entry test's
target is matched, beaten or declared rather than typed with a date beside it.
The seventeen slots are unchanged.

**0020 changed it again**, after a second run on 2026-08-27 found 0019 half
applied. The days come from the schedule rather than from seven questions
consulting nothing; the phase count is gone, because the end date already fixed
it; and the refusal for an over-long span names the ceiling and points at the
end that has to move. **A span past fifteen phase weeks is refused, not split**
— 0020 records what a second periodisation would have cost, which is a starting
1RM asserted about a test that has not happened.

**And #39 lets it author the other two templates.** `document.rs` has read
`test`, `linear` and `block` since the templates existed; the wizard reached
`block`, so a standalone test between two blocks meant hand-writing the document
this exists to replace.

**0018 changes what it asks again.** `gating_role` and `[programme.weekdays]`
leave the document, because the scheduler derives where the heavy session lands.

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
real store on 2026-08-25, and again after each of 0019 and 0020.

**The 14th is now the strongest test of `Diary::ordinarily`.** It is the last
day of the Rome alteration, which leaves no room to train at all — so a wizard
reading the *altered* week there would offer the block no days and no heavy
session. It reads the ordinary week, and the loss goes on being taken separately
as a skip.

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

#31 landed the states and the link. Three pieces remained, and all three are
work 0018 relies on rather than work it discards — it says what a performance
*was* is decided by the session it fulfilled, which is exactly this link. **Two
of the three have landed**; withdrawal has not:

- ~~**A performance takes its role from its prescription, not its date.**~~
  **Done**, 2026-08-28. `Performance` carries the session it fulfilled —
  resolved through the published id, which is the only thing that links the two
  — and the gate reads the role off that where there is one. A published session
  performed on Saturday morning now gates.

  **The calendar remains the fallback**, and the first cut of this was wrong to
  drop it. Run against a copy of `local.db` it moved the summer block from week
  four of its ladder to week one and prescribed its last six sessions 7.5kg
  light, because every heavy session of that block was trained against a Hevy
  routine the operator made by hand and the tool never delivered. Those sessions
  were trained. So: the prescription answers where it can, the calendar answers
  otherwise, and an *unlinked* session performed a day late is still lost —
  which is a test rather than a surprise.

  Two things the work turned up. **A programme's identity across re-authorings
  is its name, not its row id**: re-authoring writes a new `programme` row, so
  a link holding a `ProgrammeId` would have dropped every session prescribed
  before the last correction — six of them for `summer-2026-front-squat`.
  `latest_of_each` already picks by name for the same reason. And **the corpus
  already carries its routine ids**: every July session titled Heavy names one
  Hevy routine and every Light one names another. A re-normalise populates
  `performed_against` for eleven sessions back to 6 July, though only the one
  delivered from the tool resolves to a prescription.

  `Calendar::place` is untouched on the prescribing path, where asking what a
  *date* is for is the right question. 0018 deletes both it and the fallback, by
  deleting the calendar.
- **Withdrawal** for a published, unperformed session — delete the routine at
  the source and drop the delivery row. Needs `ON DELETE CASCADE` on
  `prescribed_item` and its children, which is why a draft is not disposable
  today despite § 12.1 saying it is.
- ~~**The comparison** — performed against prescribed, which `project` can
  already do and nothing calls.~~ **Done**, 2026-08-28. `fitness compare` pairs
  a performance with the prescription it answers and reports what diverged.
  `project` and `satisfies` were both already there; what was missing was the
  pairing, and the pairing is the whole problem — two records that meet in one
  place.

  **That place is the published id**, so a session performed on a different day
  from the one it was prescribed for is still found. Where nothing names the
  session the day stands in, and the output says which it got: a pairing by id
  is a fact the record holds, one by date is an assumption, and they read
  identically otherwise. Two sessions on one day with nothing naming the
  prescription is refused rather than guessed between.

  It writes nothing. A comparison re-derives exactly from the two records, and
  § 12 asks us to keep what cannot be regenerated.

## Deferred, and none of it on the critical path

- **The zone read by date at derivation.** The § 13 defect is real — change the
  zone, re-normalise, and every workout's wall clock is rewritten — but it bites
  only if the operator trains in another zone, and he cannot train in Rome. It
  should land before it can bite, not before 14 September.
- **`config.toml` deleted entirely.** Step 1 empties it of the zone; `database`
  is the last thing in it and goes when there is a reason to touch the file.
- ~~**Porcelain**~~ **Done**, 2026-08-30. `fitness gym next`.
- **Slot amendments** — needed the next time equipment moves, not before.
- **`programme.toml` start → 2026-07-06** — it corrects week numbers on a block
  about to end, in a document about to be superseded.
- ~~**Redelivery via `PUT`**~~ **Done**, 2026-08-30 — decision 0022. A reference
  names a place at the destination, `deliver` creates and `replace` updates, and
  a corrected session replaces the one on the operator's phone instead of
  landing beside it.

  Rejected along the way: having `deliver` **refuse** when a stranded sibling
  exists. The operator's reason, on the spot: correcting a session is the thing
  being asked for, and a delivery that declines to send the correction prevents
  the very case it exists to serve. Cheapness is not a reason to build the wrong
  thing.

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
6. **What anchors a programme that follows another?** Raised by 0020 and left
   open there. Today an anchor is a number authored into the record, so a
   programme after a block can only assert what that block *plans* to reach. A
   programme able to say "my anchor is whatever the one before me exits at" —
   resolved at prescription rather than at authoring — is what would let a long
   span hold two periodisations. It is not a small change: it makes an authored
   record depend on a future measurement, which is the opposite of what § 12 and
   0011 rely on. Nothing needs it before the autumn block.

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

  **The other half of that risk has now happened.** The release PR was merged on
  2026-08-26 because conventional commits had piled up, tagging `v1.0.0` on a
  tool that could not yet author the autumn block correctly. It was backed out
  in #37 — the commit reverted, the tag and the GitHub release deleted, the
  standing release PR closed. Cutting the release too early is as real a failure
  as cutting it too late, and cheaper to make.

---

## Two things a new session should read first

- `docs/constitution.md`, which governs. It is short and binding.
- `CLAUDE.md`, for the way of working. Spec Kit is retired and `specs/` is
  deleted — see decision 0024.

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
