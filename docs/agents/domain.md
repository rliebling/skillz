# Domain Docs

How the engineering skills should consume this repo's domain documentation when exploring the codebase.

## Before exploring, read these

- `CONTEXT.md` at the repo root.
- Relevant ADRs under `docs/adr/`.

If any of these files do not exist, proceed silently. Do not flag their absence or suggest creating them upfront.

## Layout

This is a single-context repo:

```text
/
|-- CONTEXT.md
`-- docs/adr/
```

## Use the glossary's vocabulary

When your output names a domain concept, use the term as defined in `CONTEXT.md`. Do not drift to synonyms the glossary explicitly avoids.

If the concept you need is not in the glossary yet, either reconsider whether it belongs in this domain language or note it as a possible gap for domain-modeling work.

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than silently overriding it.
