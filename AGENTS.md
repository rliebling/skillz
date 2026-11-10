## Agent skills

### Issue tracker

Issues are tracked with Beads (`br` CLI, prefix `z`); use `br ready` for unblocked work, create beads with `br create`, claim work with `br update <id> --claim`, and model blockers with `br dep add`. See `docs/agents/issue-tracker.md`.

Commit rules for beads work:

- One leaf bead per commit (subject ends with exactly one `(<id>)`; do not bundle sibling leaves).
- Close the bead before committing so `.beads/issues.jsonl` claim/close updates ship in the same commit as the implementation — never a follow-up beads-only `Close …` commit.
- Never `br close` a bead unless the next step is committing that close with the implementation in the same revision. If you cannot commit yet (including when the user has not asked to commit), leave the bead `in_progress`.

### Triage labels

The canonical triage labels map directly to Beads labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repo: read root `CONTEXT.md` and relevant ADRs under `docs/adr/`. See `docs/agents/domain.md`.
