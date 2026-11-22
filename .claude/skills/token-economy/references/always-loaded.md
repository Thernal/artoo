# Always-loaded text, and MCP or CLI

## What loads before the first prompt

Every session pays for these on every turn:

- CLAUDE.md files — the user's, the project's, parent directories' — and everything they `@import`. An import
  does not save anything: the imported file loads whole. AGENTS.md costs the same once CLAUDE.md imports it.
- Rules (`.claude/rules/*.md`) without `paths:` frontmatter. A rule with `paths:` loads only when matching files
  are touched.
- The auto-memory index `MEMORY.md`: its first 200 lines or 25 KB.
- Every installed skill's `description` (with `when_to_use`, capped at 1,536 characters in the listing). The body
  loads only when the skill is invoked. `disable-model-invocation: true` takes the description out too, for a
  skill only ever run by name.
- MCP tool definitions — deferred while tool search is on (the default on current models), loaded whole when off.

`/context` shows the split in tokens, which is where an audit starts.

## The audit

1. **Measure.** `wc -l CLAUDE.md AGENTS.md .claude/rules/*.md`, the files they import (`grep -n '^@' CLAUDE.md`),
   `ls .claude/skills` with each description's length, and `/context` in a live session.
2. **Target: under ~200 lines per file** (Claude Code's own guidance). Longer files cost context and are followed
   less well.
3. **Sort every section:**

   | It is | It goes |
   |---|---|
   | True in every session and short: stack, build and test commands, layout, hard "never" rules | stays |
   | A procedure for a task done sometimes: a release, a migration, an ingest flow | a skill in `.claude/skills/<name>/`, whose description names when it fires; one line in place pointing at it |
   | Reference: tables, templates, long lists | a file under `docs/`, one line pointing at it |
   | True only for some paths | a rule with `paths:` |
   | Stale, a duplicate, or what the code already says | deleted |

4. **Skills too:** a rarely used skill with a 1,000-character description costs every session; shorten it to the
   trigger phrases and the "Not for" clause. Two skills that fire on the same words are a merge or a rename.
5. **Report before editing:** lines before and after per file, each move, what gets deleted. Change only what the
   owner agrees to. A moved procedure keeps its words: move it, don't rewrite it.

## MCP or CLI

The owner's rule (2026-11-20):

- **Drop an MCP server only where a well-known CLI does the same job**: a GitHub MCP → `gh`; a GitLab one → `glab`.
- **Keep** MCPs with no good CLI, or with their own sign-in or state: Otto, Artoo, Figma, the mail and calendar
  connectors.
- **Filter a CLI's output before the model reads it**: `gh pr list --json number,title --jq '.[] | …'`,
  `gh api … --jq`, `glab … -F json | jq '…'`, `| grep`, `| head -n 50`. A CLI gives only what was asked for; an
  MCP result is often a page of JSON.
- MCP results warn above 10,000 tokens and are cut at `MAX_MCP_OUTPUT_TOKENS` (default 25,000); a text result over
  50,000 characters is saved to a file.
- Adding or removing an MCP server mid-session breaks the prompt cache when tool search is off — decide at the
  start of a session, not in the middle.
