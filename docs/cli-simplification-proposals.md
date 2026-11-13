# skillz CLI: simplification proposals

Proposals only — analysis of the current command surface for learnability and
consistency. No implementation implied by this document.

Reviewed against top-level / nested `--help`, `CONTEXT.md`, ADRs, and the
README workflow. Related scratch notes also appear in root `TODO.md`.

## Current surface (mental map)

| Job | Commands today |
| --- | --- |
| Bootstrap | `library init/register`, `project init`, `user init`, `hooks install` |
| Put skill in Project | `add`, `fetch`, `new`, (`project import`, `skill install`) |
| Put skill in Library catalog | `libcommit --as …`, `library import`, (`project import`) |
| Ship an edit | `libcommit` → `promote` → `lock --main` |
| Inspect | `status`, `list`, `list --catalog`, `library list`, `drift` |

The domain model is sound; the command surface teaches it poorly: too many
near-synonyms for “get a skill here,” one overloaded Library verb
(`libcommit`), and a post-promote dance that is easy to get wrong.

---

## Highest-friction overlaps

### 1. Three ways to “get a skill into this Project”

`new` / `fetch` / `add` all land files under `skills_dir`, but only `add`
creates an honest Lock. `new`/`fetch` intentionally create Drift until
`libcommit` — correct domain, opaque UX.

**Proposal:** One Project-facing verb for materializing skills, with clear
modes:

| Intent | Suggested shape |
| --- | --- |
| From catalog (Locked) | `skillz add NAME` *(keep)* |
| From GitHub/path (Drift) | `skillz add --source github:…` *(absorb `fetch`)* |
| Blank Authored scaffold (Drift) | `skillz new NAME` *(keep — creation, not acquisition)* |

Deprecate bare `fetch` (alias → `add --source` for a while). Teach:
**`add` from catalog = Locked; anything else = Drift until you publish to
the Library.**

### 2. `libcommit` does three different jobs under one opaque name

- First catalog as Authored
- First catalog as Upstream / Tool-Provided
- Later edit → Branch Lock / Overlay Fork

The name also collides with “git commit” and doesn’t say *where* state goes.
Root `TODO.md` already floats `libpush` / `push`.

**Proposal — split by intent (strongest win):**

| Intent | Suggested command |
| --- | --- |
| First time: Installation → Catalog Entry + Lock | `skillz publish NAME --as authored\|upstream …` |
| Later edit: Branch Lock / Overlay Fork | `skillz push NAME -m "…"` *(or `publish` with no `--as`)* |

If you want one verb, rename `libcommit` → **`publish`** (or **`push`**)
and keep mode detection — but **don’t keep `libcommit`**; it’s the #1
learnability tax.

### 3. `promote` then `lock --main` is a footgun

Promote updates Library main but **does not** re-Lock the Project. `lock`
without `--main` errors (`only lock --main is supported`), so the flag is
ceremony.

**Proposal:**

- Default: `skillz promote NAME` also re-Locks Project to main + refreshes
  Installation.
- Escape: `skillz promote NAME --keep-lock` (stay on branch Lock).
- Drop standalone `lock`, or reduce to `skillz lock NAME` (= today’s
  `--main`) with no required flag.

### 4. Two different `import`s

- `library import` = bulk **catalog** Upstream from GitHub
- `project import` = **migrate** Skillfile / adopt leftovers

**Proposal:** Rename project side to `project migrate` or `project adopt`.
Keep `library import` (or `library catalog-from`).

### 5. `add` / `install` / `register` vocabulary collision

- Project: `add` = create Installation
- Library: `register` aliased as `library add`
- Hooks / embedded skill: `install`
- Domain noun: **Installation**

**Proposal:**

- Project materialize stays `add` **or** rename to `install` to match the
  noun (pick one; don’t mix).
- Drop `library add` alias — only `library register`.
- Keep `hooks install` / `skill install` (different objects).

---

## Confusing terms (commands + domain leak)

| Term | Why it confuses | Proposal |
| --- | --- | --- |
| **libcommit** | Not English; ≠ git commit; ≠ promote | Rename/split (`publish` / `push`) |
| **Lock** (noun) vs **`lock`** (cmd) | Cmd only does “switch to main” | Fold into `promote`; reserve “Lock” as noun in help |
| **Authored** vs **Owned** | CLI `--as authored`; docs also say Owned Entry | One user-facing word (`authored` *or* `owned`) everywhere |
| **Upstream** vs **Tool-Provided** | Tool-Provided is a kind of Upstream in CLI (`--as upstream --source cmd:…`) | Keep model; help should say “Upstream (github or cmd)” |
| **Library** vs **Catalog** | `list --catalog`, `library list --catalog` | One catalog command path (below) |
| **`--user` vs `user`** | Flag mirrors almost every cmd; `user` only has `init` | Prefer `skillz --user …` for day-to-day; keep `user init` **or** `init --user` |
| **`skillz skill …`** | Tautology; looks like “manage any skill” | `skillz self show\|install` or `skillz provided …` |
| **Availability** vs “installed” | Good domain; help still says “install” a lot | Align help strings with Installation / Availability |

---

## Discovery / list / status overlap

Today:

- `list` → Project Installations
- `list --catalog` → bound Library Catalog
- `library list` → registry **or** catalog
- `status` → bindings + installations

**Proposal:**

- `list` = what’s Installed in this scope
- `catalog` (or `list --catalog` only) = what Libraries offer
- `library list` = **registry only** (paths/names); don’t dual-purpose it
- Keep `status` as the “am I set up?” summary (bindings, skills_dir, drift
  count) — distinct from row listing

Optional: clap **help headings** so `--help` groups Everyday vs Setup
instead of a flat ~16-command soup.

---

## Smaller cleanups (still worth doing)

1. **Missing top-level remove** — `remove` exists in core / `drift -i`
   only. Add `skillz remove NAME` (Project Installation + Lock; Library
   entry stays), matching README behavior.
2. **`tag`** — fine as mutate; document that `-t` on other cmds is filter
   *or* write-on-create; consider `untag` later.
3. **`library init` vs `register`** — keep both; make help contrast “create
   skeleton” vs “point at existing checkout.”
4. **`-l` / `--library`** — accept short alias wherever Library is
   targeted (also noted in `TODO.md`).
5. **Global `--user` on nonsense cmds** — hide on `library`,
   `completions`, `skill` where scope doesn’t apply (already hidden under
   `user`).

---

## Suggested target shape (everyday)

```text
# Setup (rare)
library init|register|list|import
project init|migrate
user init          # or: init --user
hooks show|install

# Everyday in a Project / --user
status
list / catalog
add NAME | add --source … | new NAME
publish / push     # replaces libcommit
promote            # includes re-Lock by default
drift [-i|-g]
tag / remove
```

**Learning story in one sentence:**  
*Try skills in the Project (`add` / `new`) → `publish` into a Library →
edit with `push` → `promote` to main.*

---

## What not to collapse

- **Library vs Managed Project vs user-global** — real scopes; nesting
  `library` / `project` is fine.
- **Drift as a first-class concept** — keep; don’t hide behind silent sync.
- **Branch Lock vs Overlay Fork** — keep as outcomes; don’t need separate
  top-level commands if `publish`/`push` explains them.
- **Promote as a concept** — keep; just stop requiring a second command
  afterward.

---

## Priority if only a few things ship

1. Rename/split **`libcommit`** → `publish` / `push`
2. Make **`promote` re-Lock by default**; simplify/remove **`lock --main`**
3. Absorb **`fetch` into `add --source`**; keep **`new`** for scaffold
4. Rename **`project import`** → `migrate`/`adopt`; drop **`library add`**
   alias
5. Group help + unify **list/catalog/status** roles
