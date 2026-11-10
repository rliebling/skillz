# Issue Tracker

This repo tracks work with **Beads** via the `br` CLI (agent-first issue tracker). Issue IDs use the prefix `z` (e.g. `z-1`).

Workspace data lives under `.beads/` (SQLite + `issues.jsonl`). Use `br where` to confirm the active workspace before mutating issue state. Use `br help` for command details.

Do **not** run `br setup` in this repo unless a human explicitly asks.

## Finding work

Use `br ready` to find work that is open, unblocked, and not deferred.

Do not use `br list` to browse for the next task. If `br ready` returns nothing, check `br blocked` for stalled upstream work.

## Starting work

Before making code changes for an existing bead, claim it:

```bash
br update <id> --claim
```

Then inspect it:

```bash
br show <id>
```

## Creating work

```bash
br create -t task -p P2 "Title" -d "Description"
```

For related work, create an epic first and attach children with `--parent`:

```bash
br create -t epic "Umbrella title"
br create -t task --parent <epic-id> "Child task"
```

## Publishing a spec

When a skill says "publish to the issue tracker", create a Beads issue whose description is the full spec body:

```bash
br create -t feature -p P1 --title "Spec: <name>" -l ready-for-agent -d "$(cat path/to/spec.md)"
```

Prefer putting the entire spec in the issue description (not a separate `.scratch/` file). Apply the `ready-for-agent` label at create time (or via `br update <id> --add-label ready-for-agent`).

## Dependencies

```bash
br dep add <issue> <depends-on>
```

Use `br dep list <id>` to inspect blockers and `br dep tree <epic-id>` for epic structure.

## Updating and closing

Use `br update` for status, labels, description, acceptance criteria, and notes.

Use `br close <id> -r "Reason"` when work is complete. Prefer `br close <id> --suggest-next` when continuing through newly unblocked work.

Close **before** creating the implementation commit so `.beads/issues.jsonl` updates land with the code. Do not commit the work first and then close into a separate revision.

## Labels

Apply triage labels with `br update <id> --add-label <label>` or `br create ... -l <label>`. See `docs/agents/triage-labels.md`.

## Commits

When a commit implements a bead, end the subject with that bead’s id:

```text
Add Library Binding config (z-12)
```

Rules:

- **One leaf bead per commit.** Do not implement or close two leaf beads in one commit, and do not mention a second leaf id in the subject or body.
- **Close with the work.** `br close` (and claim) metadata in `.beads/issues.jsonl` belongs in the same commit as the implementation. Do not leave a follow-up commit whose only change is closing a bead (e.g. subject `Close …`).
- Finish, close, and commit the current leaf before starting the next one.
- Exception: when closing the last child of a Spec/epic, the parent Spec’s close may ride in that child’s commit’s beads metadata; the commit subject still uses the child id only.
