# Jujutsu (advanced)

Consult this when day-to-day guidance in [`jujutsu.md`](jujutsu.md) is not
enough. Standing start/finish and description rules still apply.

## When to read this

- Writing or evaluating a non-trivial revset
- Resolving conflicts
- Creating, moving, tracking, forgetting, or deleting bookmarks
- Rebasing, squashing, restoring, abandoning, undoing, or restoring an
  operation
- Splitting a change (fileset-only)
- Stale or immutable working-copy errors
- Fetch, push, or PR creation beyond a simple requested push

## Snapshot discipline

Add `--ignore-working-copy` to read-only inspection (`jj log`, `jj diff`,
`jj bookmark list`, `jj evolog`, `jj op log`) only when the working copy is
already known snapshotted — it suppresses a new snapshot and can report
stale state.

Omit it at the start of work and after creating, editing, or deleting a
file: those moments need a fresh snapshot.

For capped, approval-friendly history reads, prefer `scripts/jj-read` from
[`jujutsu.md`](jujutsu.md).

## Splitting a change by file

```bash
jj split -m "<description for the extracted change>" <path>...
```

Filesets keep it out of the diff editor; `-m` keeps it out of the text
editor. Hunk-level splits need an interactive editor — hand those to the
user.

## Resolving conflicts

Edit conflict markers in the files to the intended final content, then
confirm with `jj status` that no conflict remains. Saving the file is the
resolution: do not run `jj resolve`, and do not look for a `git add`
equivalent afterwards.

## Positioning a bookmark for publication

Do not move bookmarks during ordinary implementation. Position one only
when the user asks to push or create a PR (including when push is implicit
in the PR request).

Never assume `@` is the revision to publish — after `jj new` it may be
empty. Target the newest ancestor of `@` that has both content and a
description, and read it back before moving the bookmark:

```bash
jj log --ignore-working-copy --no-graph \
  -r 'heads(::@ & ~empty() & ~description(exact:""))' \
  -T 'change_id.short() ++ " " ++ description.first_line() ++ "\n"'

jj bookmark create <name> -r 'heads(::@ & ~empty() & ~description(exact:""))'
jj bookmark move <name> --to 'heads(::@ & ~empty() & ~description(exact:""))'
jj status
```

Do not use `jj bookmark advance`; spell out the target revision.

## Remote and PR workflow

Unless instructed otherwise, use `main` as the PR base and preserve each
change in a stack (do not squash stacked changes when preparing a PR).
Track an untracked remote bookmark with `jj bookmark track <name>@origin`.
Skip the push when local and remote already match.

PR title follows the same description rules as changes (Conventional
Commits unless `AGENTS.md` says otherwise; ticket id in parentheses when
applicable). Write the PR body for the audience of the repo; do not assume
a language.

```bash
jj git fetch
jj log --ignore-working-copy
jj bookmark list --all --ignore-working-copy
# position bookmark — see above
jj git push -b <name>
gh pr create --base main --head <name> --title "<type>: <summary>" --body "<body>"
```

## Recovery and troubleshooting

Before `jj undo`, `jj op restore`, `jj restore`, or `jj abandon`, inspect
the affected operation or revision and confirm it is exactly the intended
target. Do not discard or rewrite user work on an assumed target.

**Stale working copy** — `jj workspace update-stale`, then `jj status`.

**Immutable revision** — do not bypass immutability. Check the target,
choose the intended mutable change, and retry there.

If a task cannot be completed safely with non-interactive commands, ask
the user to perform that part.
