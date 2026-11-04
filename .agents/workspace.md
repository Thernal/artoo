# Workspace configuration (shared)

Local rows (scratch root, task naming) live in `.misc/workspace.md`; on a conflicting value the local table wins.

## Git delivery

| Row | Value |
|---|---|
| Commit subject | Conventional Commits — `<type>(<scope>): <subject>`, imperative, lowercase, no trailing period (`AGENTS.md`) |
| Subject enforcement | none — no hook |
| Ticket reference | GitHub issue `#<n>` of Thernal/artoo: subject ends with `(#<n>)`; the commit that finishes it says `Closes #<n>` in its body |
| Branches | none: work is committed on `main` itself (the developer, 2026-11-04) — no `draft/` or task branch |
| Subject without a ticket | `chore: <description>`, or the issue opened first |
| Integration | rebase |
| Delivered shape | preserved commits |
| Protected branches | `main` |
| Commit attribution | on — `Co-Authored-By` trailer, as in the first commit |
| Never staged | `.misc/`, `CLAUDE.local.md`, `.claude/settings.local.json` |
