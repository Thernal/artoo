# A session's spend

## `/usage`

The Session block: `Total cost` (list price, computed locally), durations, `Total code changes`, and
`Usage by model` — input, output, cache read, cache write per model. On a subscription it adds plan usage and a
breakdown by skill, subagent, plugin and MCP server: the place to see which one costs most. `/clear` resets it.

The cache line (main conversation only, after the first response):

```text
Prompt cache (main):   14 requests · 91% of input tokens from cache · 2 misses (last 6m 10s ago, 310.2k tokens re-cached) · 1 expected rebuild (compaction or tool-result clearing) · warm (1h TTL, last activity 40s ago)
```

- **% from cache** — 90% and up is a healthy long session; low means the context keeps being re-written.
- **misses … re-cached** — each miss wrote that many tokens again at the cache-write price. Newer versions add
  `likely cause: …` (for example "tool definitions changed").
- **expected rebuild** — compaction or tool-result clearing: normal.
- **warm / cold, TTL** — past the TTL the next turn is a miss. 5 minutes by default; 1 hour on a subscription within
  plan usage, for the main conversation only — subagents and compaction stay at 5 minutes.

`/context` shows what fills the window: system prompt, tools, memory files, skills, messages.

## What breaks the cache mid-session

- `/model` (each model has its own cache), `opusplan` switching in plan mode, a model fallback, a skill whose
  frontmatter names another model.
- Changing effort — except on the 5.5 models and Fable 5.1+ through an API key or a subscription, where it is safe.
- Turning fast mode on.
- MCP servers or tools added or removed, a plugin bringing MCP servers, a deny rule for a whole tool — when tool
  search is off.
- `/compact` (by design), many images, a Claude Code upgrade, another working directory.

Safe: editing CLAUDE.md (it is not re-read until `/clear`, `/compact` or a restart — so a rule edit takes effect
only then), editing files, permission mode, output style, skills, `/recap`, rewind.

A break longer than the TTL costs a full re-write on the next turn. On a long context after a long pause, `/clear`
with a short written handoff is often cheaper than going on.

## `/clear` and `/compact`

- **`/clear` between unrelated tasks.** It costs nothing and reloads CLAUDE.md. Before it, what the next task needs
  goes into a file (a task's progress note), not the chat.
- **`/compact` sparingly** — when one long task must go on and its history still matters. It is a large request of
  its own and invalidates the cache; give it the brief: `/compact keep the plan, the failing test and the files
  touched`.
- The auto-compact point: `/autocompact <tokens>`, saved as `modelSettings.<model>.autoCompactWindow`; a smaller
  window compacts sooner (cheaper turns, more summaries).
- An agent cannot run either: it says so to the owner at a task boundary, in one line.

## Spend in the background

- Prompt suggestions (`promptSuggestionEnabled`, on by default): a short request after each response, mostly cache
  reads.
- Away recaps (`awaySummaryEnabled`, `CLAUDE_CODE_ENABLE_AWAY_SUMMARY=0`) after a few minutes unfocused.
- Idle compaction (`idleCompaction`), scheduled prompts and `/loop` (each wake-up re-sends the context), goal
  check-ins (`CLAUDE_CODE_GOAL_CHECKIN_MINUTES=0`).

Off switches matter for an unattended session; in an interactive one they are the owner's choice.

## Bash and MCP output limits

`bashOutputMaxChars` (or `BASH_MAX_OUTPUT_LENGTH`, default 30,000 characters) and `MAX_MCP_OUTPUT_TOKENS`
(default 25,000) — `references/build-output.md` for builds.
