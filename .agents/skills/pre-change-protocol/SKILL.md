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

Working-copy, git-vs-jj, describe, and finish rules live only in
`source-control` — do not reinvent them here; read that skill and follow it
at the steps below.

## Fast path (trivial)

Use when the request is a small, unambiguous edit (typo, one-liner, obvious
doc tweak) and no design choices are open:

1. Confirm the target in one sentence (silently is fine if already clear).
2. Follow `source-control` **start-of-work**.
3. Make the change.
4. Follow `source-control` **end-of-work**.

## Full path

### 1. Clarify

If the goal, scope, or success criteria are ambiguous, ask the minimum
questions needed — one at a time. Do not turn every task into a long design
interview; for deep design stress-testing, use `grilling` (or similar) instead.

If the repo has standing instructions (`AGENTS.md`, etc.), skim anything that
affects this change before editing.

### 2. Source-control start-of-work

Read `source-control` now and follow its start-of-work procedure before any
edits.

### 3. Plan (non-trivial only)

For non-trivial work, state a short plan (a few bullets) before editing.
Skip this step on the fast path.

### 4. Implement

Only after the steps above. Prefer the smallest change that satisfies the
request. While working, keep following `source-control` (e.g. refining the
jj description as intent shifts).

### 5. Source-control end-of-work

When the change is done, follow `source-control` finish rules.

## Out of scope

- Not a substitute for `grilling`, specs, or ticket breakdown.
- Does not replace project-specific `AGENTS.md` rules — apply those too.
- Does not duplicate git/jj procedure — that is entirely `source-control`
  (+ its jj docs).
