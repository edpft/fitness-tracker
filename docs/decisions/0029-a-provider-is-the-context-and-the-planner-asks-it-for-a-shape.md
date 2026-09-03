# 0029 — A provider is the context, and the planner asks it for a shape

**Date**: 2026-09-03

**Amends** `0026-do-not-mix-bounded-contexts.md`, decision 1. The bounded
context is the **provider**, not the published programme. 0026's reasoning is
unchanged and its boundary moves out one level.

**Closes** 0026's open question 2, the cycling intent vocabulary.

## Context

The operator, 2026-09-03:

> "what if you treat Peloton as provider of programmes in total, which would
> allow it to own the Discover → Boost → Build → Peak invariant and how they
> degrade. What the planner needs to tell it is: a number of sessions per
> microcycle, a number of microcycles per mesocycle, and an intention so that it
> can decide whether to pull, say, 12 weeks from Build and Peak or Boost and
> Build. This is the "brain" I've occasionally referenced because, once it gets
> back the possible options from the cycling programme provider it needs to
> optimise against the gym programme provider to produce a coherent hybrid
> programme. That's the main aim to the programming side of this tools
> functionality to produce coherent hybrid programmes."

**0026 put the boundary in the wrong place.** It made each published programme
its own context. But Peloton's four power-zone programmes share a vocabulary
*and an ordering* — Discover, then Boost Your Base, then Build, then Peak — and
an ordering across programmes is a fact no programme-level context can hold. As
0026 stood, "putting Build after Peak inverts the intention" had nowhere to
live and had to come from the operator each time.

## Decision

**1. The provider is the bounded context.** Stronger By Science, Peloton, Joe
Friel, the old CrossFit gym. A provider owns its vocabulary, its programmes, the
order they are meant to be run in, and how that degrades when a span will not
fit them.

**2. A provider is a port.** It is asked a question and answers with candidates.
Adding one is an adapter rather than a change to the planner, which is the same
shape a second data source has, and the first test of this architecture's
independence claim on the programming side.

**3. The planner asks for a shape, in three parts:**

```text
sessions per microcycle     how much of the week this discipline gets
microcycles per mesocycle   the shape to conform to
intention                   what is being trained for
```

The first two generalise 0026's decision 4 and its amendment. The third is new
and is what lets a provider choose *which* of its programmes to draw on: twelve
weeks of Build and Peak, or of Boost and Build, is a question only Peloton can
answer.

**4. An intention comes from its domain's published literature** — not from a
provider, not from a style of programming, and not from us. The operator,
2026-09-03: *"I'd prefer it if these come from the sports science literature
instead of one specific programme, style of programming or my head."*

**Strength**: the American College of Sports Medicine's 2026 position stand,
its first revision since 2009, synthesising 137 systematic reviews. It organises
around **muscle strength**, **muscle size / hypertrophy**, **power**, and
*physical performance*. **Local muscular endurance was dropped** from the 2009
stand and no reason is given for the omission.

**Endurance**: **Coggan's** adaptation targets — the published meaning of each
power zone, which `domain::cycling::zone` already carries and already credits as
*"Coggan's own names"*.

### Coggan is not Peloton's model, and the distinction is the whole point

An agent objected that adopting Coggan would be a provider's vocabulary wearing
a neutral hat, since Coggan's zones are what Peloton's power zones *are*. The
operator, 2026-09-03:

> "if Peloton has adopted Coggan model, then we're not adopting Peloton's model,
> we're also adopting Coggan's, which is an academic / practitioner model on par
> with the ACSM."

**The direction of derivation decides it.** Peloton adopted Coggan; Coggan did
not come from Peloton. That is unlike a vendor's own invention — Hevy's exercise
categories, say — which is the case the rule about a source's identifiers exists
for. A model that a provider adopted from outside itself is available to us on
the same terms it was available to them.

The rejected candidate was the literature's **determinants of endurance
performance** — VO2max, lactate threshold, and efficiency/economy. That is a
real taxonomy from the peer-reviewed literature and it answers a different
question: it predicts how fast someone goes, rather than naming what a session
trains. *Economy* is not something a power-zone ride targets, which is the tell.

**5. The brain enumerates, ranks and does not choose.** It takes the candidates
each provider returns, forms the hybrids, keeps the ones whose fatigue profiles
coincide, and presents them. Choosing between two coherent arrangements is a
training judgement — whether peaking before building is acceptable this term —
and stays the operator's. This holds his earlier answer: *"yes, enumerate."*

**6. The session split is an input now and a lever later.** How many sessions
each discipline gets is stated. The operator, 2026-09-03, on whether the brain
might vary it: *"yes, that does make sense. if I said I wanted to focus on
strength say, we could dial up the gym and dial down the cycling."* Not for this
autumn, where the split is stated as two and two.

## The aim, in the operator's words

> "That's the main aim to the programming side of this tools functionality to
> produce coherent hybrid programmes."

That is a better statement of purpose than "you give it a span and it returns
arrangements", which is the mechanism rather than the point.

## Open

**1. Whether Coggan is fine-grained enough to carry a session's intent.** The
operator's original requirement, 2026-09-02, was that *"two Power Zone Rides
with the same amount of Z5 can be different if they are structured differently
to elicit different adaptation."* The test case is in the transcription: week 3
day 1 is ten thirty-second Z5 bursts, week 5 day 1 is three sustained Z5 blocks.
Both are Z5, so both are VO2max under Coggan, and the vocabulary does not
separate them. Either they are one intention realised two ways — which is a
defensible answer and simplifies matters — or something finer is needed. Not
settled here.

**2. Who decides what a provider drops** when a span will not fit its
programmes. Carried forward from 0026's amendment: the judgement moves into
whoever transcribes the provider, which is the right place for it, but it is a
decision to record rather than something to let fall out of an implementation.

**3. Fat loss is an input the tool does not model.** The operator's goal for this
autumn, 2026-09-03: *"I want to try and balance strength and conditioning gains
while trying to loose fat."* A deficit lowers recovery capacity and so bears on
what fatigue profiles are tolerable. Nutrition is currently out of scope in
`docs/roadmap.md`; this is the first time it has appeared as an input to
programming rather than as a deferred layer. Named, not scoped.
