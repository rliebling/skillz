---
name: skillz
description: >-
  Use the skillz CLI to manage agent skill Libraries, Catalog Entries,
  Installations, Locks, and Drift. Use when adding, updating, or formalizing
  skills, or when tempted to copy/symlink skill files by hand.
---

# Managing skills with skillz

Prefer **skillz** over hand-copying skills between projects. Libraries are the
union catalog; Managed Projects install and Lock skills into `skills_dir`
(default `.agents/skills`).

## When to use skillz

- Discover or install skills into a Managed Project or user-global scope
- Publish a managed Installation into a Library (`publish`)
- Refresh from Library main (`update`) or link an on-disk tree (`add --attach`)
- Report Drift / pending review instead of silent edits to a Library

Do **not** invent Locks, silently vendor Upstream into a Library `skills/` tree,
or copy skills between repos without Catalog Entry + Lock.

## Core scopes

| Scope | Meaning |
| --- | --- |
| **Library** | Catalog: Owned content and/or Upstream references (`contribution = direct\|review`) |
| **Managed Project** | Repo with Bindings; Installations + Locks under `.skillz/` |
| **User-global** | `skillz --user` / `skillz user …` for `~/.agents/skills` |

## Decision tree (everyday verbs)

```text
Need skill files in this Project?
  missing on disk → skillz add NAME
  already on disk, link to Catalog → skillz add NAME --attach [-l LIB]
  take Catalog files while linking → skillz add NAME --attach --from-library
  stage github/path/cmd (Unlocked) → skillz add --source … [NAME]
  scaffold Authored (Unlocked) → skillz new NAME
  mark Local immediately → add --local / new --local

Send Project → Library?
  managed Local / Library-backed → skillz publish NAME… -l LIB
  review Libraries push/PR by default (--no-push / --no-pr to skip)

Refresh from Library main?
  skillz update NAME…

Clean up Project only?
  skillz remove NAME…   (Catalog Entry stays)
```

There is **no** `fetch`, `libcommit`, `promote`, or `lock --main`.

## Common commands

```bash
skillz status [--json]
skillz list --catalog
skillz list --tag TAG
skillz add NAME
skillz add NAME --attach -l LIBRARY
skillz add --source github:owner/repo/path@rev
skillz add --source 'cmd:tool skill install --dir {dir}' NAME --local
# skills-root CLIs (create <dir>/<name>/): use {skills_dir} instead of {dir}
skillz new NAME --local
skillz publish NAME -l LIBRARY
skillz publish NAME -l LIBRARY --dry-run
skillz update NAME
skillz remove NAME
skillz drift
skillz drift --check          # exit 2 on hard Drift only (pending ≠ 2)
skillz drift -i               # TTY only; actions: publish / add --attach / update / remove
skillz migrate --dry-run [--json]
skillz hooks show
skillz hooks install
skillz skill show
skillz skill install
```

Non-interactive agents: pass `--no-prompt` (or run non-TTY). Prefer `--with-deps` /
`--no-deps` on `add` rather than y/N prompts. Use `--json` on status / drift / list /
migrate --dry-run / publish.

Library registry / bootstrap: `skillz library init|register|list|import`.  
Project bootstrap: `skillz project init --binding LIBRARY`.  
User-global: `skillz user init --binding LIBRARY`.

## Authored vs Upstream vs Tool-Provided

- **Owned / Authored**: Local authored → `publish` creates Owned Entry under Library `skills/`.
- **Upstream github**: Local+source `github:…@rev` → `publish` writes Catalog reference (no Library file mirror).
- **Tool-Provided**: Local+source `cmd:…` → Catalog stores install template; Locks pin tool version; `add` re-runs the template. Placeholders: `{dir}` = Installation path (tool writes `SKILL.md` there); `{skills_dir}` = skills root (tool creates `<skills_dir>/<name>/`); `{name}` = skill name. Nested `{dir}/{name}/` installs are hoisted flat.

## Edit / Drift flow

1. Edit the Installation in the Project (or user) skills dir.
2. `skillz publish NAME -l LIBRARY` → direct main or review Branch Lock (+ push/PR).
3. After review merge: `skillz update NAME` to take Library main tip.
4. Pending review shows in `status` / `drift` but does **not** fail `drift --check`.
5. Drift gates (`hooks install`) block dishonest paths unless explicitly allowed.

## Availability

`list` / `add` show where a skill is already visible (Project `skills_dir`,
`~/.agents/skills`, other agent roots). Project Installations are what
collaborators receive on clone.
