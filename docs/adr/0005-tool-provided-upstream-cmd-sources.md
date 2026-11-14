# Tool-Provided skills as Upstream `cmd:` sources

Tool-Provided skills enter a Library only on explicit libcommit, as Upstream catalog references—not Owned file trees—so skillz can re-materialize them through the providing tool instead of a Library mirror. The portable source string is a command template (`cmd:…` with skillz placeholders), optionally `|version:…` when `{argv0} --version` is wrong; catalog entries omit `@rev`, while Locks record an observational tool version after a successful probe. On Library add/libcommit, skillz eagerly validates both the install template and the version probe—exercising them the way `github:…` is checked when cataloging—so a bad CLI shape fails before a later Project `add`. Project `add`/refresh always runs the template; missing tools are hard errors. Rejected: a third EntryKind; `tool:provider/skill` identity with opaque adapters as the Lock key; verbatim paths in sources; content-hash pins as the primary Lock rev; Library-vendored fallback copies.

## Placeholders and install layouts

Tools disagree about what `--dir` means. skillz supports both via placeholders and post-install normalization:

| Placeholder | Value | Use when the tool… |
| --- | --- | --- |
| `{dir}` | Installation path (`<skills_root>/<name>`) | Writes `SKILL.md` **into** that directory (install-dir CLIs, e.g. `skillz skill install`) |
| `{skills_dir}` | Parent of `{dir}` (the skills root) | Creates `<dir>/<name>/SKILL.md` (skills-root CLIs, e.g. rexpad / rex-canvas) |
| `{name}` | Skill name | Needs the Catalog/Installation name explicitly |

After the template runs, skillz requires `SKILL.md` at the Installation root. If the tool instead wrote `{dir}/{name}/SKILL.md` (skills-root semantics with `{dir}`), that nested tree is hoisted into `{dir}` and the nest is removed—so a wrong or unknown tool layout still yields a flat Installation. When both a flat file and a nest exist, the nest is treated as the tool’s fresh output and overwrites the flat file. Catalog authors should still prefer the matching placeholder so installs do not rely on hoist.
