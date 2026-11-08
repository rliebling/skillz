# Prefer Library Branch Lock over project-local patches

Managed Projects often need to ship a skill fix before a company/team Library merges to its main line. Project-local patches (Skillfile-style pin) leave the Lock pointing at an upstream revision while content diverges. We prefer Library Branch Lock: push a branch/PR revision to the target Library and Lock the Project to that revision, with a report of Locks not on the Library main line. When the Library is not writable even for branches, fall back to Overlay Fork into a writable bound Library and Promote upstream separately.

## Considered Options

- Long-lived Intentional Drift / dishonest Locks with patches applied on install
- Overlay Fork as the only path
- Library Branch Lock primary, Overlay Fork fallback (chosen)
