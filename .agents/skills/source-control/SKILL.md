---
name: source-control
description: >-
  Choose and use the repo's source control tool. Auto-detects jj (jujutsu)
  vs git via a .jj/ directory. Use at the start of work (working-copy check,
  jj describe) and when inspecting history, diffs, or status, or when
  committing, bookmarking, pushing, or otherwise touching source control,
  including any git or jj command. Paired with pre-change-protocol.
depends: [pre-change-protocol]
---

# Source control

Paired with `pre-change-protocol`: that skill should run before the first
file edit; this skill owns git vs jj detection, start-of-work working-copy
rules, and finish-of-work (`jj new` / git commit policy).

## Detect git vs jj

Walk from the working directory up to the filesystem root:

- A `.jj` directory at any level → **jj repo**. Use jj; never run raw `git`
  (including `git status`, `git log`, `git show`, `git diff`). Colocated
  `.git` does not change this. `jj git fetch` / `jj git push` are jj commands.
- No `.jj` directory → **git repo**. Use git.

If this is a jj repo, read [`jujutsu.md`](jujutsu.md) before any jj work.
Do not load jj guidance in a pure git repo.

## Git

### Before editing

Run `git status` (and `git diff` if dirty).

- Clean tree → proceed.
- Dirty, and the changes clearly belong with this task → proceed on that
  worktree state.
- Dirty with unrelated changes, or relatedness is ambiguous → **stop and ask**.
  Git cannot cheaply peel work onto a new commit the way jj can.

### Committing

Do **not** commit, push, amend, rebase, or rewrite history unless the user
asked.

When the user asks to commit, follow the repo's commit rules if present
(e.g. `AGENTS.md`). Otherwise use Conventional Commits. For ticket work,
put the ticket id at the end of the subject in parentheses, e.g.
`feat: add login throttle (z-abc)`.

## jj

All jj procedure lives in [`jujutsu.md`](jujutsu.md) (day-to-day) and
[`jujutsu-advanced.md`](jujutsu-advanced.md) (non-trivial ops). In jj repos,
finishing work includes leaving a new empty change on `@` — that is not the
same as git's "commit only when asked."
