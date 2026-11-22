# Subagents — the brief and the model

A subagent reads 50k tokens so the lead does not; what comes back is all the lead pays for. That is the whole
saving, so the return is the part of the brief to get right.

## Models by role

| Role | Model | For |
|---|---|---|
| Lead | the session's model (Opus) | planning, decisions, judgment-heavy writing, reviewing what comes back, subtle code (concurrency, security, migrations) |
| Bounded step | `sonnet` | reading docs, surveying a repo's conventions, mechanical edits over named files, running tests and reporting the failures |
| Plain lookup | `haiku` | where is X defined, which files match, does this string occur — a grep with a report |

Keep a step with the lead when it needs a decision the lead owns, or when the brief cannot be written clearly —
then the task is not understood well enough to hand off.

## How Claude Code picks a subagent's model

First match wins:

1. the Agent call's `model` (an alias: `sonnet`, `haiku`, `opus`);
2. the agent file's `model:` frontmatter;
3. `CLAUDE_CODE_SUBAGENT_MODEL` in the settings' `env`;
4. the parent's model.

What that means in practice, checked on Claude Code 2.1.329 (2026-11-20) with the account default set to `sonnet`
and no `model` in the calls:

| Agent | Runs on |
|---|---|
| general-purpose | `sonnet` — the account default |
| a custom agent with no `model:` | `sonnet` — the account default |
| Explore, Plan | the lead's model (Opus under an Opus lead) — they ignore the account default |

So **name the model in the call** for Explore and Plan (`haiku` for a lookup, `sonnet` for a plan of a bounded
part), and in any call where the default is not what the step needs. `CLAUDE_CODE_SUBAGENT_MODEL_FORCE=1` would
force one model onto every subagent, teammate and workflow agent, the per-call choice included — leave it unset,
so a Haiku lookup or an Opus subagent for one hard part stays possible.

## The brief

```text
Objective: <one sentence — what to find or do, and what decision it feeds>
Known / ruled out: <what the lead already has, so it is not redone>
Read: <named files, directories, URLs — not "explore the repo">
Boundaries: <read-only | edit only these files | no builds | no sessions | no network>
Return, at most ~1.5k tokens: <the exact shape — numbered answers, a table, a list of changed files>;
each claim with file:line or a URL; "not found" said plainly, never guessed.
```

- One objective per subagent. Independent ones go in one message, so they run in parallel.
- Hand it the question, not your hypothesis — handed a hypothesis, it tends to come back confirming it.
- For a review you want unanchored, give it the code, not your conclusion.
- Don't delegate a handful of tool calls whose target you know: the brief and the hand-back cost more.
- Once delegated, don't do it yourself too; wait for the result.
- Check the one or two claims your decision rests on — a `file:line` is cheap to open.
