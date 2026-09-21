---
name: tidy
description: Project hygiene across GitHub issues, dependencies, milestones and discussions — close what is done or no longer relevant, fix what is wrong, and report. Use at the end of every issue (/start calls it), or when the operator says /tidy or asks for issues, dependencies or milestones to be brought up to date.
---

# /tidy

The operator's closing prompt used to be "ensure that all issues, dependencies
and milestones are up to date and still relevant". This skill is that prompt.
What goes where (issues, discussions, milestones) is set out in CLAUDE.md.
This skill keeps each of those places accurate.

**Act, then report.** Close what is done or no longer relevant, each with a
one-line comment saying why. Do not ask first. The operator, 2026-09-21: *"If I
need to revert, I'll tell you, but re-creating an issue is better than keeping
loads around."*

`gh` is in the flake, so run it as `nix develop --command gh …`. Read the
current state from the remote. Do not rely on memory of it.

## Issues — every open one

- **Done**: its PR merged, or the work landed some other way. Close it and say
  where the work landed.
- **Duplicate** of another open issue: close the thinner one, pointing at the
  other, and carry over anything only it said.
- **Overtaken**: a later decision or change made it moot. Close it and name what
  overtook it.
- **Still wanted but wrong**: the title or "done when" no longer describes the
  work. Edit it.

## Dependencies

- Each `blocked_by` still holds: the blocker really must land first.
- Nothing is missing. Where the work cannot start before another issue lands,
  add `gh issue edit <n> --add-blocked-by <m>`.
- An issue in a milestone is not blocked by an issue in a later milestone or in
  the backlog. Move the blocker in, or remove the link if it no longer holds.

## Milestones

- **Passed**: move anything still needed for the next milestone into it, and
  send everything else to the backlog (no milestone). If it wasn't needed for
  the milestone that passed, it wasn't really part of it.
- **Complete**: every issue it needs is in it, blockers included.
- **Relevant**: every issue in it is needed for what the milestone names.
  Move the rest to the backlog.
- **Accurate**: the description still says what the milestone is for and why
  it comes in this order.

## Discussions — every open one

A discussion is only open if answering it unblocks something.

- **Answered**: write the answer where it now lives (in an issue, the code or a
  decision record), comment with a link, and close as resolved.
- **Overtaken, or never blocking anything**: comment why and close as outdated.

Discussions have no `gh` subcommand. Use GraphQL: `addDiscussionComment`, then
`closeDiscussion` with `reason: RESOLVED | OUTDATED | DUPLICATE`.

## Report

A short list, one line per change, grouped by what was done:

```
closed   #<n>   landed in #<pr>
closed   D#<n>  overtaken: <what overtook it>
moved    #<n>   → backlog; <milestone> does not need it
blocked  #<n>   by #<m>: <why it must land first>
```

Then the next unblocked issue in the current milestone. Nothing else.
