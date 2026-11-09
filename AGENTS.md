## Agent skills

### Issue tracker

Issues are tracked with Beads (`br` CLI, prefix `z`); use `br ready` for unblocked work, create beads with `br create`, claim work with `br update <id> --claim`, and model blockers with `br dep add`. See `docs/agents/issue-tracker.md`.

### Triage labels

The canonical triage labels map directly to Beads labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repo: read root `CONTEXT.md` and relevant ADRs under `docs/adr/`. See `docs/agents/domain.md`.
