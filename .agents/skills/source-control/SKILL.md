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

Paired with `pre-change-protocol` (runs before the first file edit). This
skill owns git vs jj detection and the start-of-work / end-of-work procedures
that skill calls.

## Detect git vs jj

Walk from the working directory up to the filesystem root:

- A `.jj` directory at any level → **jj repo**. Use jj; never run raw `git`
  (including `git status`, `git log`, `git show`, `git diff`). Colocated
  `.git` does not change this. `jj git fetch` / `jj git push` are jj commands.
- No `.jj` directory → **git repo**. Use git.

If this is a jj repo, read [`jujutsu.md`](jujutsu.md) before any jj work
(including start-of-work / end-of-work below). Do not load jj guidance in a
pure git repo.

## Start of work

### Git

Run `git status` (and `git diff` if dirty).

- Clean tree → proceed.
- Dirty, and the changes clearly belong with this task → proceed on that
  worktree state.
- Dirty with unrelated changes, or relatedness is ambiguous → **stop and ask**.
  Git cannot cheaply peel work onto a new commit the way jj can.

### jj

Follow **Start of work** in [`jujutsu.md`](jujutsu.md) (status, describe
before edits, when to `jj new`).

## During work

- **Git:** leave the tree uncommitted unless the user asked to commit.
- **jj:** refine `jj describe` as intent shifts; see [`jujutsu.md`](jujutsu.md).

## End of work

### Git

Do **not** commit, push, amend, rebase, or rewrite history unless the user
asked.

When the user asks to commit, follow the repo's commit rules if present
(e.g. `AGENTS.md`). Otherwise use Conventional Commits. For ticket work,
put the ticket id at the end of the subject in parentheses, e.g.
`feat: add login throttle (z-abc)`.

### jj

Follow **End of work** in [`jujutsu.md`](jujutsu.md) (final describe, then
`jj new`). Do not wait for the user to ask to "commit."

Non-trivial jj ops (bookmarks, push/PR, conflicts, recovery): 
[`jujutsu-advanced.md`](jujutsu-advanced.md).
