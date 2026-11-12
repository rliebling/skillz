# Jujutsu (day-to-day)

Read this when the repo has a `.jj/` directory. For revsets, conflicts,
splits, recovery, immutable/stale working copies, and bookmark/push edge
cases, also read [`jujutsu-advanced.md`](jujutsu-advanced.md).

## Hard rules

- Never run raw `git`. `jj git …` and `gh` are fine.
- Do not run `jj resolve`, `jj diffedit`, or `jj arrange` (interactive).
- `jj split` only with filesets and `-m` — never bare, never `-i`/`--tool`.
- Always pass `--git` to `jj diff`, `jj show`, and `jj log -p`.
- After history-changing ops (`jj new`, `jj rebase`, `jj squash`,
  `jj restore`, `jj abandon`, …), run `jj status` before continuing.
- Do not move bookmarks during ordinary implementation — only when
  publishing (push / PR). See advanced for positioning.
- Do not edit `~/.config/jj/config.toml` to add aliases for the agent.
  User aliases (e.g. `log-5`) are for humans; do not depend on them.

## Descriptions

Use Conventional Commits unless `AGENTS.md` (or equivalent) explicitly
states another style.

For ticket work, put the ticket id at the end of the subject in
parentheses:

```text
feat: tighten start-work rules (z-abc)
```

Refine with `jj describe` as the work evolves; the first description is an
intent, not a final message.

## Start of work

Before editing, run `jj status` and inspect `@`.

| Working-copy state | Action |
| --- | --- |
| No diff, no description | `jj describe -m "…"`, then edit |
| No diff, stale description | Reuse `@`; `jj describe -m "…"` with the new intent; then edit |
| Diff clearly belongs with this task | Stay on `@`; refine `jj describe` if needed; then edit |
| Diff clearly unrelated | `jj new`, then `jj describe -m "…"`, then edit |
| Relatedness ambiguous | Stop and ask |

Describe **before** starting edits when you open a new change (or re-describe
an empty `@`).

## During work

- Prefer single `jj` commands — no `&&`, `;`, `echo` banners, or `| head`
  around jj (those combinations get held for approval).
- Working-copy inspection: `jj status`, `jj diff`, `jj diff --stat`
  (these snapshot).
- History / read-only inspection: use `jj-read` (below), not ad-hoc
  pipelines. Add `--ignore-working-copy` on raw read-only jj only when the
  working copy is already known snapshotted; omit it right after creating,
  editing, or deleting files.

## End of work

When the implementation for this change is finished:

1. Final `jj describe` if the message no longer matches what landed.
2. `jj new` so the finished work stays on its change and `@` becomes a
   fresh empty commit.

Do **not** wait for the user to ask to "commit." In jj, the working-copy
change is already a commit; `jj new` closes the unit of work.

Do not push or create a PR unless the user asked. When they do, follow
bookmark positioning in [`jujutsu-advanced.md`](jujutsu-advanced.md).

## Read history with `jj-read`

Use the `scripts/jj-read` next to this skill's `SKILL.md` (project install
or `~/.agents/skills/source-control/scripts/jj-read`). One command, no
extra shell syntax:

```bash
<path-to-skill>/scripts/jj-read log [N]
<path-to-skill>/scripts/jj-read show REV [-- paths]
<path-to-skill>/scripts/jj-read diff REV [-- paths]
<path-to-skill>/scripts/jj-read stat REV [-- paths]
```

`jj-read` is read-only: `--ignore-working-copy`, no pager, truncates output
(`JJ_READ_MAX`, default 20000 bytes), refuses to run outside a `.jj` repo.
It is a script, not a jj alias — an alias cannot cap output or replace
`echo`/`head` glue.
