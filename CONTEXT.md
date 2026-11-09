# Skills Manager

Libraries of agent skills (personal, team, or company) used across Managed Projects, so skills in use have a union catalog and owned skills have a clear source of truth instead of drifting copies.

## Language

**Library**:
A curated union catalog of skills in use: owned editable content plus references to skills consumed as Upstream (or similar). The source of truth for what that library's users standardize on.
_Avoid_: master repo, global skills, Skillfile (the old tool)

**Personal Library**:
A Library owned by an individual for their own managed work.
_Avoid_: master repo, home skills

**Skill**:
A named agent capability packaged for reuse (typically a `SKILL.md` and related files).
_Avoid_: prompt, rule (unless a specific product uses that name)

**Tag**:
A label on a Catalog Entry used for discovery and filtering within a Library (e.g. listing by topic or toolchain).
_Avoid_: label (when meaning skill discovery), category

**Authored Skill**:
A skill whose content originated with the library owner.
_Avoid_: custom skill, my skill

**Upstream Skill**:
A third-party skill recorded in a Library as a catalog reference (source + Lock), not as owned editable content.
_Avoid_: downloaded skill, remote skill

**Owned Entry**:
A Library entry whose skill content is Authored or Forked and editable in that Library.
_Avoid_: local skill, vendored skill (when meaning library-owned)

**Catalog Entry**:
Any Library entry representing a skill in use — Owned or Upstream — so the Library stays the union of skills used under that Library.
_Avoid_: index row, registry item

**Forked Skill**:
A skill that began as Upstream, Tool-Provided, or another Library's content and was then edited; the Library that holds the Owned Entry owns the resulting content.
_Avoid_: tweaked skill, modified copy

**Tool-Provided Skill**:
A skill installed by a tool onto the machine (often as a global); out of scope for the Personal Library until promoted to Authored or Forked.
_Avoid_: installed skill, global skill

**Project**:
A codebase that may contain skill Installations for agent work.
_Avoid_: repo (when meaning the consumer of skills), workspace

**Managed Project**:
A Project explicitly opted into one or more Libraries via an ordered list of Library Bindings. Lasting Installations are tied to Catalog Entries via Locks; new or edited skill files may exist briefly as Drift while being tested, then enter the Library workflow through libcommit. Installations/Locks/Drift are this tool's concern.
_Avoid_: my project, tracked project

**Library Binding**:
An association from a Managed Project to a Library, in priority order; on name conflict, earlier bindings win. Each Installation records which bound Library it came from. A binding is read-only or writable for the user: installs may come from either; new Catalog Entries and Promotes only go to a writable Library, with an explicit target when more than one is writable. In version control, a Binding records a shared name plus the Library's stable remote identity (URL); each machine resolves that to a local path via the user's skillz registry.
_Avoid_: library link, search path

**External Project**:
A Project not opted into a Library (e.g. someone else's clone); the tool may observe skills read-only but does not require Catalog Entries or rewrite the Project unless the user explicitly imports.
_Avoid_: foreign repo, unmanaged clone

**Installation**:
A materialized copy of a skill at a configured skills directory, taken from a Library entry (owned content or Upstream) at a specific revision. In a Managed Project the default directory is `.agents/skills` (overridable, e.g. `.claude/skills`) and Installations are committed so collaborators receive them on clone. The user-global skills directory (`~/.agents/skills`) is a skillz-managed scope with the same Lock/Install model but local to the user.
_Avoid_: symlink, live link, checkout, fetch-only install

**Lock**:
The recorded identity of an installed skill's source and revision (which library entry or Upstream source, which version).
_Avoid_: Pin (collides with other tools' "local patch" meaning), reference, pointer

**Promote**:
An explicit action that proposes an improved skill into a target Library's main line, distinct from libcommit (which records a Library revision / Branch Lock from a tested Installation) and from re-Locking a Project to main.
_Avoid_: sync, push back, auto-merge, libcommit (different step)

**Drift**:
A difference between a Project's Installation and the Library revision its Lock claims; not the happy path for lasting edits.
_Avoid_: divergence (as the normal workflow), dirty copy

**Drift Report**:
A review of detected Drift across Managed Projects; may offer Overlay Fork, Library Branch Lock, or Promote as explicit next actions — never a silent merge into a Library.
_Avoid_: sync job, auto-sync, cron merge

**Library Branch Lock**:
The preferred way to ship a skill change in a Managed Project before Library main-line approval: push a branch/PR revision to the target Library and Lock the Project to that revision. A report lists Locks that are not on the Library main line.
_Avoid_: Skillfile pin, local patch, intentional dishonest lock

**Overlay Fork**:
Fallback when the desired Library is not writable even for branches: Fork the skill into a writable bound Library, Lock the Project to that Owned Entry, and Promote upstream separately later.
_Avoid_: personal copy (vague), bypass
