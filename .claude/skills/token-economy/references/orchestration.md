# Orchestration — who plans, who works, how many

Whoever runs work through other sessions (a manager, a planner, a lead) follows this. A worker follows the
parts that name it.

1. **Opus plans, Sonnet works.** Opus only writes a plan: a planner worker reads what it needs, writes `plan.md`
   (steps, each a bounded brief for Sonnet with acceptance criteria) and ends. It does not implement, build, test or
   watch background work. Implementers are Sonnet; their subagents Sonnet or Haiku. A small or clear task needs no
   planner: Sonnet works directly. Never start an Opus lead to "do the whole task".
2. **At most 2 workers running at once.** Queue the rest, start the next when one ends. One heavy run (a full
   build, an emulator suite, a design round) at a time.
3. **At most 3 background subagents at once; no fan-out.** No swarm per file or per group. A task that needs more
   is split into sequential steps.
4. **No polling agents.** Wait with a shell loop or the harness's notification, not an agent that checks.
5. **Short contexts.** Read files in parts, filter every log, end at done instead of waiting. A worker idle in a long
   session is closed, not nudged: a follow-up is a fresh worker with a short brief (point it at the files and the
   `progress.md` or `plan.md`, not at the old chat).
6. **Hand off by file.** Plan, progress and state live in files (`plan.md`, `progress.md`, `state.md`); the next
   session reads them instead of inheriting a long context.
