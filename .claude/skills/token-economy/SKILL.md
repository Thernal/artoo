---
name: token-economy
description: Token discipline for every session and every worker — use at the start of any work, before delegating or starting a worker or subagent, when planning a task, on a long task, and when a session costs too much or fills its context fast. Covers who plans and who implements (Opus plans, Sonnet works, worker and background limits, no polling agents, fresh short follow-ups), brief outputs (answer what was asked, outcome first, no recaps), agent-to-agent briefs and the fixed short reply, writing a subagent brief and picking models, trimming Gradle, cargo and xcodebuild output, auditing CLAUDE.md, AGENTS.md and MEMORY.md, /usage, /clear and /compact. "use the token-economy skill", "write a brief for a worker", "which model for subagents", "why is this session so expensive", "the build log floods the context", "audit CLAUDE.md". Not for prompt caching in an app that calls the API (claude-api).
---

# Token economy

Whatever enters the context is paid for again by every later turn until the context is cleared — cache reads are
cheap, not free, and a cache miss re-writes all of it. So the savings that matter are what never gets in: the whole
build log, the file read in full for three lines, a subagent's 8k-token report, a CLAUDE.md of 600 lines. Quality
comes first: cut what the model does not need, never what it does.

## In every session

- **The lead plans and decides; bounded steps go to subagents** with a brief (`references/subagents.md`): objective,
  what to read, boundaries, a return of at most ~1.5k tokens with `file:line`. Name the model in every Agent call —
  `sonnet` for bounded work, `haiku` for plain lookups: the account default (`CLAUDE_CODE_SUBAGENT_MODEL`) moves
  general-purpose and custom agents, but Explore and Plan run on the lead's model unless the call names one.
  Don't delegate a handful of tool calls whose target you know.
- **No whole build log in the context.** Gradle, cargo and xcodebuild output reaches the model as the failing lines
  and the tail: the hook in `references/build-output.md`, or `| tail` / `| grep` by hand.
- **Filter CLI output before reading it**: `--json` with `--jq`, `grep`, `head`. Prefer a well-known CLI to an MCP
  server doing the same job (`gh` over a GitHub MCP); keep MCPs with no good CLI or with their own sign-in or state.
- **Read in parts** when you know where to look; don't re-read what is already in the context.
- **Keep the prompt cache**: don't switch model or toggle MCP servers mid-task; at a task boundary suggest `/clear`
  to the owner, `/compact` only when a long task must go on (`references/session.md`).

- **Brief outputs.** Answer exactly what was asked, outcome first. No recap of what you did, no tables or long
  reports unless asked; details on request. This holds for the human and for the manager reading a worker
  (`references/communication.md`).
- **Orchestration** (a worker, a planner, a manager): Opus only plans and ends; Sonnet implements; at most 2 workers
  at once, at most 3 background subagents, no polling agents; a follow-up is a fresh short session, not a nudge
  (`references/orchestration.md`).
- **Agent to agent.** A brief is goal + criteria + constraints + file pointers, never pasted bodies. A reply is the
  fixed short form, at most ~1.5k tokens; Otto and a worker have their own section; artifacts go in files and the message carries the path; one question at a
  time, with a recommendation (`references/communication.md`).

## Procedures

| When | Read |
|---|---|
| Auditing a repo's always-loaded text; deciding MCP or CLI | `references/always-loaded.md` |
| A build or test log floods the context; installing the trim hook | `references/build-output.md` |
| Planner, implementer, worker and subagent limits; follow-ups | `references/orchestration.md` |
| Brief outputs; the brief and the reply between agents | `references/communication.md` |
| Delegating: the brief, models by role, how the subagent model is chosen | `references/subagents.md` |
| A session costs too much: `/usage`, the cache line, `/clear` or `/compact`, background spend | `references/session.md` |

## Placement

| Kind | Default root | Visibility | Ownership |
|---|---|---|---|
| Build-trim hook and its allow rule | `.claude/settings.local.json` (merged, other keys kept) | local | own |
| Procedures moved out of always-loaded text | the project's skill folder, `.claude/skills/<name>/` | shared | own |

The audit edits a project's CLAUDE.md or AGENTS.md only as far as the owner agrees, after the report.
