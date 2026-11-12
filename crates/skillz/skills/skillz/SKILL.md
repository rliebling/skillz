---
name: skillz
description: >-
  Use the skillz CLI to manage agent skill Libraries, Catalog Entries,
  Installations, Locks, and Drift. Use when adding, updating, or formalizing
  skills, or when tempted to copy/symlink skill files by hand.
---

# Managing skills with skillz

Prefer **skillz** over hand-copying skills between projects. Libraries are the union catalog; Managed Projects install and Lock skills into `skills_dir` (default `.agents/skills`).

## When to use skillz

- Discover or install skills into a Managed Project or user-global scope
- Formalize a tested Installation into a Library (`libcommit`)
- Record Drift / Branch Lock / Promote instead of silent edits to a Library
- Catalog a Tool-Provided skill so it can be re-installed from the providing tool

Do **not** invent Locks, silently vendor Upstream into a Library `skills/` tree, or copy skills between repos without going through Catalog Entry + Lock.

## Core scopes

| Scope | Meaning |
| --- | --- |
| **Library** | Catalog: Owned content and/or Upstream references |
| **Managed Project** | Repo with Bindings; Installations + Locks under `.skillz/` |
| **User-global** | `skillz --user` / `skillz user …` for `~/.agents/skills` |

## Common commands

```bash
skillz status
skillz list --catalog
skillz list --tag TAG
skillz add NAME
skillz add --tag TAG
skillz new NAME --tag topic
skillz fetch github:owner/repo SKILLNAME
skillz libcommit NAME --as authored --library LIBRARY
skillz libcommit NAME --as upstream --library LIBRARY --source 'github:…@rev'
skillz drift
skillz drift -i
skillz drift -g
skillz promote NAME
skillz lock NAME --main
skillz hooks show
skillz hooks install
skillz skill show
skillz skill install
```

Library registry / bootstrap: `skillz library init|register|list|import` (from inside a registered Library, `--library` defaults to that checkout).  
Project bootstrap: `skillz project init --binding LIBRARY`.
User-global: `skillz user init --binding LIBRARY`.

This skill ships inside the `skillz` binary. Humans can inspect it with `skillz skill show`; agents/tools materialize it with `skillz skill install --dir {dir}` (Tool-Provided `cmd:` template).

## Authored vs Upstream vs Tool-Provided

- **Authored** (`libcommit --as authored`): Owned Entry; skill files live in the Library.
- **Upstream github** (`--source github:owner/repo[/path]@rev`): Catalog reference; Project `add` fetches that revision. No Library mirror of the files.
- **Tool-Provided** (`--source 'cmd:…'`): Catalog reference whose source is an install **command template** (ADR 0005). Example:

  ```bash
  skillz libcommit skillz --as upstream --library personal \
    --source 'cmd:skillz skill install --dir {dir}'
  # or another tool, e.g. rexpad:
  skillz libcommit rexpad --as upstream --library personal \
    --source 'cmd:rexpad skill install --dir {dir}'
  ```

  - Placeholders: `{dir}` = Installation path, `{name}` = skill name.
  - Optional version probe override: `|version:rexpad version` (default `{argv0} --version`).
  - Catalog stores the template **without** `@rev`; Locks pin an observational **tool version**.
  - libcommit **eagerly validates** install + version probe (same spirit as checking `github:…` when cataloging).
  - Later `skillz add` **re-runs** the template; missing tools fail clearly. Collaborators still get committed Project Installations without running the tool.

## Edit / Drift flow

1. Edit the Installation in the Project (or user) skills dir.
2. `skillz libcommit` → Branch Lock or Overlay Fork (honest Lock).
3. `skillz promote` → Library main; then `skillz lock NAME --main`.
4. Drift gates (`hooks install`) block publishing dishonest skill paths unless explicitly allowed.

## Availability

`list` / `add` show where a skill is already visible (Project `skills_dir`, `~/.agents/skills`, other agent roots). Project Installations are what collaborators receive on clone.
