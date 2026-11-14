# skillz

**skillz** manages agent skills across the projects you work in.

Instead of copying `SKILL.md` trees by hand from old repos (and watching those copies diverge), you keep one or more **Libraries** — personal, team, or company catalogs of the skills you actually use — and **Managed Projects** select from them with honest, versioned **Locks**. Day-to-day editing happens in the project (or user-global) skills directory; **`publish`** formalizes a managed skill into the Library.

skillz is a Rust CLI (with optional Ratatui `-i` flows).

## Why it exists

Typical pain:

- Skills live in whatever project last used them; you forget the source.
- Copies drift across repos; collaborators never see laptop-only globals.
- “Just commit the tweak” quietly lies about where the skill came from.

skillz’s model:

| Idea | Role |
|------|------|
| **Library** | Union catalog: Owned skill content you edit, plus Upstream references you consume |
| **Managed Project** | A repo opted in via ordered **Library Bindings**; Installations are committed under `skills_dir` (default `.agents/skills`) |
| **Lock** | Pin of which Catalog Entry / revision an Installation came from |
| **Drift** | Installation ≠ Lock (fine while testing; blocked from casual publish by hooks) |
| **publish** | Project → Library (direct main commit, or review Branch Lock + push/PR) |
| **update** | Library main tip → Project Installation + Lock |

Upstream identity is a single portable string: `github:owner/repo[/path][@rev]` (or Tool-Provided `cmd:…`).

## Build

```bash
cargo build -p skillz --release
# binary: target/release/skillz
```

For day-to-day development:

```bash
cargo run -p skillz -- --help
```

Config lives under `~/.config/skillz/` (override with `--config-dir` or `SKILLZ_CONFIG_DIR`).

## Shell completions

Generate a completion script for your shell (bash, zsh, fish, elvish, or powershell) and install it the usual way for that shell:

```bash
# bash (~/.bashrc or a file under bash_completion.d)
skillz completions bash > ~/.local/share/bash-completion/completions/skillz

# zsh — add to ~/.zshrc *before* compinit, then generate + reload the shell:
#   fpath=(~/.zfunc $fpath)
#   autoload -Uz compinit && compinit
mkdir -p ~/.zfunc && skillz completions zsh > ~/.zfunc/_skillz
# then: exec zsh

# fish
skillz completions fish > ~/.config/fish/completions/skillz.fish

# elvish / powershell
skillz completions elvish
skillz completions powershell
```

Completions are generated from the clap command tree, so they stay in sync with `skillz --help`.

## Quick start

### 1. Create or register a Library

```bash
skillz library init ~/code/my-skills --name personal
# or register an existing checkout:
skillz library register ~/code/acme-skills --name company --read-only
skillz library list

# Bulk-add Upstream Catalog Entries from a GitHub skills repo (Library only; no Project install).
# From inside a registered Library checkout, --library can be omitted:
skillz library import github:mattpocock/skills --library personal
cd ~/code/my-skills && skillz library import github:mattpocock/skills
skillz library import https://github.com/mattpocock/skills --dry-run
skillz library import github:mattpocock/skills --path skills/engineering
skillz library import github:mattpocock/skills --tag elixir --tag phoenix
skillz library import github:mattpocock/skills -i
skillz library list --catalog
```

The user registry (`~/.config/skillz/registry.toml`) maps Library **name → local path**. Project Bindings store the shared name plus remote URL so other machines can resolve the same Library.

### 2. Opt a repo into skillz

```bash
cd ~/code/new-app
skillz project init --binding personal
# or: --binding company:ro --binding personal
# or: --skills-dir .claude/skills
skillz status
skillz hooks show      # dry-run: planned gate path + contents
skillz hooks install   # git pre-commit XOR jj push-with-checks (by VCS)
```

This writes `.skillz/skillz.toml` and an empty `.skillz/skillz.lock`. No skills are installed yet.

### 3. Discover and install from a Library

```bash
skillz list                         # Project Installations only (aligned columns)
skillz list --catalog               # full bound Library Catalog
skillz list --tag elixir --tag phoenix
skillz list --dedupe
skillz add ecto-thinking            # prompts to add missing sibling deps
skillz add ecto-thinking --with-deps
skillz add ecto-thinking --no-deps
skillz add -i                       # interactive multi-select (Catalog; / to filter)
skillz add --tag elixir -i          # optional pre-filter, then multi-select
```

`list` shows **Availability** (project skills dir, `~/.agents/skills`, `.cursor/skills`, `.claude/skills`, …) so you know what the agent can already load. `add` materializes into the Project `skills_dir` and records a Lock (collaborators get the files on clone).

### 4. Author, stage Upstream, then publish

Project-first: try the skill locally, then formalize into a Library.

```bash
# New Authored skill
skillz new my-skill --tag process --local
# …test with your agent…
skillz publish my-skill -l personal

# Stage Upstream from GitHub or a local path, then publish
skillz add --source github:owner/repo/path/to/skill@abc1234 --local
skillz add --source github:owner/repo ecto-thinking --local
skillz publish ecto-thinking -l personal

# Tool-Provided Upstream (command template; see ADR 0005)
# Catalog stores cmd:…; Lock pins observational tool version.
skillz add --source 'cmd:rexpad skill install --dir {dir}' rexpad --local
skillz publish rexpad -l personal

# skillz's own agent skill is Tool-Provided the same way:
skillz skill show                              # print embedded SKILL.md
skillz skill install                           # → ~/.agents/skills/skillz/SKILL.md
skillz add --source 'cmd:skillz skill install --dir {dir}' skillz --local
skillz publish skillz -l personal
```

Without `--local`, staged/scaffolded skills stay **Unlocked** until `migrate`, `--local`, `add --attach`, or `publish` prerequisites are met. Agents working in this repo can load the **`skillz`** skill (`.agents/skills/skillz`) for CLI workflow guidance.

### 5. Edit and publish again

```bash
$EDITOR .agents/skills/grilling/SKILL.md
skillz publish grilling -l personal -m "tweak question style"
# contribution = review Libraries: Branch Lock + push/PR by default
skillz update grilling                            # after merge: take Library main tip
```

Link an existing on-disk skill to a Catalog Entry without publishing Project content:

```bash
skillz add grilling --attach -l personal
skillz add grilling --attach --from-library -l personal   # also take Catalog files
```

## User-global skills

Same Bindings / Lock / add / list / publish / update / drift model for `~/.agents/skills`:

```bash
skillz user init --binding personal
skillz status --user
skillz list --user
skillz add ecto-thinking --user
```

## Migrating unmanaged skills

```bash
skillz migrate --dry-run
skillz migrate --authored
skillz migrate -i --authored
skillz migrate grilling --source github:owner/repo/grilling@abc1234
skillz migrate --user --authored --dry-run
```

## Drift and publish gates

```bash
skillz drift                         # current Managed Project only (or --user)
skillz drift -i                      # board: same current-scope surface
skillz drift -g                      # all Managed Projects + user-global
skillz drift --check                 # non-zero exit for hooks (current Project)
skillz hooks show                    # dry-run: paths + planned contents
skillz hooks install
```

Default `drift` / `drift -i` stay on the **current scope** (this Managed Project, or `~/.agents/skills` with `--user`). Use `-g/--global` to scan every registered Managed Project plus user-global.

In `drift -i`, scroll the list (`j`/`k`), change the planned action (`←`/`→`), confirm removes in the modal, then **Enter** to apply. Press **`?`** in any interactive UI for keybindings. Remove drops the Project Installation + Lock only; the Library entry stays. Confirming Remove warns (soft) when a retained Installation appears to depend on the skill being dropped.

- **git-only:** pre-commit runs `skillz drift --check --project …` on the Managed Project.
- **jj** (including colocated): repo-local alias `jj push-with-checks` only (drift check, then `jj git push`). No git pre-commit — `jj commit` would not run it. Ordinary `jj git push` stays unwrapped.
- Escape: `--allow-drift` or `SKILLZ_ALLOW_DRIFT=1`.
- `hooks show` prints the target file and planned gate text without writing.

Sibling dependency detection is best-effort (`--with-deps` / `--no-deps` on `add`).

## Layout (cheat sheet)

| Layer | Path | In VCS? |
|-------|------|---------|
| User registry | `~/.config/skillz/registry.toml` | No |
| Managed Project config / lock | `PROJECT/.skillz/skillz.toml`, `skillz.lock` | Yes |
| Installations | `skills_dir` (default `.agents/skills`) | Yes |
| User scope | `~/.config/skillz/user.toml`, `user.lock` → `~/.agents/skills` | No |
| Library | `library.toml` + `skills/…` in the Library repo | Yes |

## Development

```bash
cargo test --workspace
```

Application behavior lives in `skillz-core` (the test seam); the `skillz` binary is a thin CLI/TUI over that API. Domain language and ADRs: `CONTEXT.md`, `docs/adr/`.
