# skillz usage flows (agreed)

## Metadata layout (agreed)

| Layer | Path | Versioned? |
|--------|------|------------|
| User registry | `~/.config/skillz/registry.toml` | No |
| Managed Project config | `PROJECT/.skillz/skillz.toml` | Yes |
| Managed Project locks | `PROJECT/.skillz/skillz.lock` | Yes |
| Installations | `skills_dir` (default `.agents/skills`) | Yes |
| User scope config/lock | `~/.config/skillz/user.toml`, `user.lock` | No |
| User Installations | `~/.agents/skills` | No |
| Library catalog + Owned | Library repo (`library.toml` + `skills/…`) | Yes |

Managed = has `.skillz/skillz.toml`; `init`/`import` also registers path in user registry for drift. No disk-wide scan.

Upstream `source` string (CLI + TOML, not structured fields):

```text
github:owner/repo[/path/to/skill][@rev]
```

Parse: after `github:`, first two path segments = owner/repo; optional further segments = path within repo; optional `@rev` suffix. Root skill: `github:owner/repo@rev`.

Example lock/catalog rows use `source = "github:owner/repo/path/to/skill@abc123"` (rev required in locks; catalog may omit `@rev` until pinned).

## Implementation stack (agreed)

- **Language:** Rust (binary + `skillz-core` lib as the application command API / test seam)
- **CLI:** `clap`
- **Config:** TOML via `toml` / `toml_edit`
- **VCS:** shell out to `git` / `jj` (infer from `.jj/` vs git metadata)
- **TUI (later):** `ratatui` + `crossterm` for `-i` flows; not FrankenTUI for v1 (unstable API, nightly, heavier than needed). Keep interactive UI behind a thin boundary so a backend swap remains possible.
- **HTTP/GitHub:** prefer CLI/`git` fetch first; add crates only as needed

---

## A. Bootstrap / register a Library

```text
$ skillz library init ~/code/my-skills --name personal
$ skillz library add ~/code/acme-skills --name company --read-only
$ skillz library list
```

Decisions:
- Committed Project Bindings use shared `name` + Library remote URL; local registry maps name → path (user-global registry, not per-project).
- VCS inferred: `.jj/` → jj, else git; optional `--vcs` escape hatch only.

### A2. Bulk-import Upstream from a GitHub skills repo

Catalog-only: upsert Upstream Catalog Entries into a writable Library. Does **not** materialize Project or `--user` Installations (use `add` later to install selectively).

```text
$ skillz library import github:mattpocock/skills --library personal
$ skillz library import https://github.com/mattpocock/skills --library personal --dry-run
$ skillz library import github:mattpocock/skills --library personal --path skills/engineering
$ skillz library import github:mattpocock/skills --library personal -i   # multi-select
```

Decisions:
- Accept `github:owner/repo[/path][@rev]` and `https://github.com/owner/repo…`; normalize to canonical `github:…@rev` in the catalog (ADR 0003).
- Discover every `**/SKILL.md` under the repo (honoring optional source path + `--path` prefix); skill name = containing directory name.
- `--dry-run` prints the plan; `-i` multi-selects a subset; `--dry-run -i` errors.
- Name collision with a different existing source → skip + warn; identical → no-op.
- Distinct from `project import` (Skillfile / adopt local leftovers into a Managed Project).

## B. Initialize a new Managed Project

```text
$ cd ~/code/new-app
$ skillz project init --bind personal
$ skillz project init --bind company:ro --bind team:ro --bind personal
$ skillz project init --bind personal --skills-dir .claude/skills
$ skillz status
```

Decisions:
- Init writes Bindings + empty Locks; does not install skills yet.
- Skills directory defaults to `.agents/skills`, overridable per Project (committed with config).
- User registry is name → local Library path only.
- `hooks install` runs as part of init (git pre-commit; jj `push-with-checks` alias).

## B2. User-global skills scope

```text
$ skillz user init --bind personal
$ skillz status --user
```

Decisions:
- `~/.agents/skills` is a first-class skillz-managed scope (not a git Project collaborators clone).
- Same Library Bindings / Locks / add / Drift / libcommit flows; Installations are local to the user.

## C. Convert an existing Project

```text
$ skillz project import --dry-run
$ skillz project import --bind personal --from skillfile
$ skillz project import --bind personal --adopt-local
$ skillz project import --bind personal -i   # TUI: per-skill upstream | authored | skip
$ skillz status
```

Decisions:
- Detect skills dir; map Skillfile/local skills into Catalog Entries + Locks; keep matching files; leftovers are Drift/import candidates.
- Scriptable flags plus `-i` interactive TUI for skill-by-skill adopt decisions.
- `skillz user import` for ~/.agents/skills.

## D. Discovery

```text
$ skillz list --library personal
$ skillz list --library personal --tag elixir
$ skillz list --tag elixir --tag phoenix
$ skillz list --tag elixir,phoenix
$ skillz list
$ skillz list --dedupe
$ skillz list --user --tag elixir
```

Example columns include **Availability** (where files already exist for this machine/scope):

```text
NAME              KIND       LIBRARY    TAGS      AVAILABLE
ecto-thinking     upstream   personal   elixir    project
grilling          owned      personal   process   user, .cursor/skills
domain-modeling   owned      personal   process   —
```

Decisions:
- `--tag` accepts repeated flags and/or comma-separated lists; combined filter is AND across all tags named.
- Default list shows every matching Catalog Entry from bound Libraries, ordered by Binding precedence; `--dedupe` keeps the winner per skill name.
- Outside a Project: require `--library` or `--user`. List is read-only.
- **Availability** reports presence in: Project `skills_dir`, user-global `~/.agents/skills`, and other known agent skill roots (e.g. `.agents/skills`, `.cursor/skills`, `.claude/skills`) even when they are not the install target. Install target remains a single `skills_dir`.
- `add`: already Locked/in `skills_dir` → report and no-op (or refresh); present only in `user`/other roots → warn that collaborators will not see those copies unless installed into the Project.

## E. Add a skill (Managed Project or `--user`)

### E1 — Already in a bound Library

```text
$ skillz add ecto-thinking
# if AVAILABLE includes project → already installed; no-op/refresh message
# if only user / .cursor/skills / … → warn, offer to install into Project skills_dir

$ skillz add ecto-thinking --library personal
$ skillz add --library personal --tag elixir
$ skillz add --library personal --tag elixir,phoenix -i
$ skillz add … --with-deps   # warn on sibling refs; recurse with flag (E1+E2)
```

### E2 — New Upstream (Project-first → libcommit)

```text
$ skillz fetch github:owner/repo/path/to/skill
$ skillz fetch github:owner/repo ecto-thinking
# materializes into Project / --user skills dir only (unmanaged until libcommit)

# use / test with the agent …

$ skillz libcommit ecto-thinking --library personal --as upstream --tag elixir
# creates Upstream Catalog Entry + Lock from the tested Installation
# --with-deps applies here too when fetching/libcommitting graphs
```

### E3 — Newly Authored (Project-first → libcommit)

```text
$ skillz new my-skill --tag process
# scaffolds under Project / --user skills dir (unmanaged until libcommit)

# use / test …

$ skillz libcommit my-skill --library personal --as authored
```

Decisions:
- E1 installs from an existing Catalog Entry (Binding precedence / `--library`).
- E2/E3 stage in the local skills dir first; **`libcommit`** creates the Catalog Entry + Lock (union catalog stays complete once you keep the skill).
- Until `libcommit`, new local skills are Drift; git pre-commit / `jj push-with-checks` block unless `--allow-drift`.
- Bulk add by `--tag`; `-i` TUI multi-select (E1).
- `github:owner/repo SKILLNAME` name lookup for nested repos.
- Dep heuristics: frontmatter, relative links, `/skill-name`, sibling names in body (e.g. Pocock `grill-with-docs` → `grilling`, `domain-modeling`).

## F. Edit in place → libcommit / hooks / Promote

Primary edit surface = Installation. Test freely; formalize with `libcommit`.

```text
$ $EDITOR .agents/skills/grilling/SKILL.md
$ skillz libcommit grilling -m "tweak question style"
# Branch Lock from Installation (or Overlay Fork if needed); update Lock

$ skillz promote grilling
$ skillz lock grilling --main
```

Hooks:
```text
$ skillz hooks install
# git  → pre-commit drift check on skill paths (block by default)
# jj   → repo-local alias `jj push-with-checks` → drift check, then jj git push
# escape: --allow-drift / SKILLZ_ALLOW_DRIFT=1
# no `skillz push` as the jj push entrypoint
```

Decisions:
- `libcommit` = Installation → Library workflow (new Catalog Entry, Branch Lock, or Overlay Fork as appropriate).
- Promote = Library main-line; separate from Project Lock bumps.
