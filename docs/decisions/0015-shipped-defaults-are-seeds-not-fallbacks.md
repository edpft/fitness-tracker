# 0015 — Shipped defaults are seeds, not fallbacks

**Date**: 2026-08-22
**Status**: Accepted
**Raised by**: the two setup wizards.
**Settles**: an apparent conflict with § 34 and with the argument in
`ConfigError::MissingTimeZone`. There is no conflict, and the reason is worth
writing down before someone reads one as licence for the other.

## What was decided

**The tool ships the operator's sensible defaults for template-level parameters,
and they are configurable.** Back-off percentages, rep and set ranges, the ladder
rate, the reset protocols, the warm-up steps.

**A default is a seed at authoring time and never a fallback at read time.** The
wizard prefills the value; the document states it; the store keeps it with the
date it was authored. Nothing on the generation path ever reaches for a compiled-in
number, because by then every value is in the record.

```text
authoring    default → prefilled → confirmed → written into the document
generation   read from the store, or fail. No default is reachable from here.
```

## Why it does not conflict with § 34

**§ 34 is about deployment.** "No hardcoded paths, hosts, ports or environment
assumptions; all of it configuration. Moving between hosts requires no code
change." A back-off of 85% is not an environment assumption — it does not change
when the tool moves to another machine, and no host makes it wrong.

**The timezone is a different thing and stays undefaulted.** § 13 names the
default timezone as an interpretive parameter, and `ConfigError::MissingTimeZone`
argues the case precisely: a compiled-in `Europe/London` "would be correct for
this account and wrong for the next, and because it would be correct here no test
would ever catch it." That argument is about a value the environment decides. It
does not reach a value the operator decides, and it is not weakened by this: the
environment wizard prefills the zone from the system and makes the operator
confirm it, so the value is stated rather than adopted.

**§ 13's discipline is satisfied because the seed becomes a dated record.**
`generation_parameters` is keyed on `authored_at` and reads take the greatest, so
the value in force at a time is recoverable. A shipped default that later changes
does not rewrite anything already authored — it only seeds the next document.

## What it costs

**A wrong default is wrong in every programme authored after it, and silently
so.** The guard is that the default is never invisible: it is written into the
document in full, so a value the operator never thought about is still a value
they can read back. A parameter that were left out of the document and defaulted
at generation time would have neither property, which is why that is forbidden
rather than merely discouraged.

**The defaults are the operator's, not the literature's.** They are what he
currently trains with, which is a better basis than a number fitted to his record
but not a claim about anyone else. When the tool has a second user, they are a
starting point to be overwritten, not a recommendation.

## Consequences

- Both wizards prefill from the shipped defaults and require confirmation.
- The programme document continues to state every value it uses; no key becomes
  optional-with-a-default.
- `TODO` still refuses rather than defaulting — an unsettled value and a
  defaulted one are different states, and 003 built the mechanism to keep them
  apart.
- Where the defaults live is an implementation question, not a decision here,
  except that they may not be reachable from the generation path.
