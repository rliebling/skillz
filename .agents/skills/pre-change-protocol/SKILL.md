---
name: pre-change-protocol
description: >-
  Mandatory checklist to run BEFORE creating, editing, or deleting any files
  (application code, docs, README, config, scripts, tests, etc.). Trigger at
  the start of a feature, refactor, bug fix, doc update, cleanup, or any other
  repo change — before the first edit.
depends: [source-control]
---

# Pre-change protocol

Paired with `source-control`. Read and follow this skill **before** the first
file create/edit/delete in a task. Do not postpone until commit time.

## Fast path (trivial)

Use when the request is a small, unambiguous edit (typo, one-liner, obvious
doc tweak) and no design choices are open:

1. Confirm the target in one sentence (silently is fine if already clear).
2. Run **source-control start-of-work** (below).
3. Make the change.
4. Run **source-control end-of-work**.

## Full path

### 1. Clarify

If the goal, scope, or success criteria are ambiguous, ask the minimum
questions needed — one at a time. Do not turn every task into a long design
interview; for deep design stress-testing, use `grilling` (or similar) instead.

If the repo has standing instructions (`AGENTS.md`, etc.), skim anything that
affects this change before editing.

### 2. Source-control start-of-work

Read and follow the `source-control` skill **now**:

- Detect git vs jj (`.jj/`).
- Check working-copy state before editing.
- In jj: describe the change before starting work when opening a new unit of
  work; stay on `@` only when existing diffs belong with this task.
- In git: ask if the tree is dirty with unrelated (or ambiguous) changes; do
  not commit unless the user asked.

### 3. Plan (non-trivial only)

For non-trivial work, state a short plan (a few bullets) before editing.
Skip this step on the fast path.

### 4. Implement

Only after the steps above. Prefer the smallest change that satisfies the
request. Keep the source-control description updated in jj as intent shifts.

### 5. Source-control end-of-work

When the change is done, follow `source-control` finish rules:

- **jj:** final `jj describe` if needed, then `jj new` (do not wait to be
  asked to "commit").
- **git:** leave changes uncommitted unless the user asked to commit.

## Out of scope

- Not a substitute for `grilling`, specs, or ticket breakdown.
- Does not replace project-specific `AGENTS.md` rules — apply those too.
