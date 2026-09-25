---
name: start
description: Pick up a GitHub issue from a clean, current main and carry it through to a pull request. Use when the operator says /start <issue>, or "start on #N", or pastes an issue link to begin work.
---

# /start <issue>

The operator's opening prompt used to be "PR #N is merged, please start on #M".
This skill is that prompt. `$ARGUMENTS` is the issue number or URL.

`gh` is in the flake, so run it as `nix develop --command gh …`.

## 1. Stand on the real main

- `git fetch`, switch to `main`, pull. If the tree is not clean, stop and say
  what is uncommitted.
- Free disk before building anything. `target/` regularly fills the root
  filesystem (116 GB on 2026-09-19, 94 GB on 2026-09-25), and a full disk kills
  builds and test runs midway with exit 137 or ENOSPC:

  ```
  rm -rf target/debug/incremental
  find target/debug/deps target/debug/examples target/debug/build \
    -maxdepth 1 -mtime +0 -exec rm -rf {} +
  df -h /
  ```

  It all regenerates. Say how much was freed in one line.
- Before saying anything about a PR, look it up on the remote
  (`gh pr list --state all --search "<issue>"`, `gh pr view`). The remote is
  the only source of truth for what has merged.

## 2. Read the issue, not a memory of it

- `gh issue view <n> --comments`: the body, the "done when", and every comment.
- What blocks it: `gh api repos/edpft/fitness-tracker/issues/<n>/dependencies/blocked_by`.
  If an open issue still blocks it, stop and name that issue. Do not start around it.
- Its milestone, and any discussion it links to.

## 3. Branch

`git switch -c <type>/<short-name> origin/main`, where `<type>` is the
Conventional Commit type the work will be.

## 4. Tell the operator, briefly

A few lines, no more:

- **What will be different for him** once this is done: the command he runs,
  and what it now does.
- **Done when**: the acceptance lines, each as a concrete example (a command and
  what it prints, or a scenario on his real dates, loads and schedule).
- **A decision, if one is genuinely needed**: a grounded scenario showing the
  choice and what each option does next, ending in the question. If none is
  needed, say nothing about decisions and start work.

## 5. Finish without being asked

Before the PR, run whatever the change prints and read every line as the
operator would:

- **Vocabulary.** A term that is not a name in `domain` is raised with him as a
  question, with a concrete example. It is not quietly reworded: see "Words highlight
  concepts" in CLAUDE.md.
- **Prose habits.** Cut commentary, cut his own rules quoted back to him, and cut
  any question the tool can answer for itself (a date it was already given, a
  choice with one possible answer).

When `nix flake check` is green:

1. Push and open the PR. The body says `Closes #<n>` and leads with what changes
   for the operator; internals get a line at most. Write the body to a file
   and pass `--body-file`.
2. Comment on the issue with what landed and anything that did not.
3. Tell the operator the PR is ready for his approval. The `main` ruleset needs
   it for PRs co-authored by Claude, and he tests from the installed build, which
   only sees `main`.
4. Run `/tidy`.
