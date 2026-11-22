# Brief outputs, and talking agent to agent

## Brief outputs (the human, and the manager)

- Answer exactly what was asked. The outcome comes first, in a line or two.
- No recap of the steps taken, no restating the question, no closing summary of what was just said.
- No table, report or list of options unless asked; give a recommendation, not a survey.
- Details only on request — say they exist ("the log is in <path>") rather than pasting them.
- A code (issue number, id) is not said alone: name the thing, code after it only when it may be looked up.

## A brief to an agent

```text
Goal: <one sentence>
Criteria: <how done is judged — checkable>
Constraints: <what not to touch, limits, the model>
Read: <file pointers, `path:line` — never pasted bodies, logs or diffs>
```

The brief is the whole context the agent gets: pointers, not contents. For a long input, write it to a file and
send the path.

## A reply from an agent

A fixed short form, a hard ceiling of about 1.5k tokens:

```text
state: done | working | blocked | question | failed
changed: at most 5 lines
refs: file:line, …
verified: the command or the path of the log — or "not verified"
question: one, with a recommendation (only for question or blocked)
```

- Artifacts (reports, logs, plans, screenshots) go in files; the message carries only the path.
- Ask one question at a time. Two questions are two turns.
- "Not found" is said plainly, never guessed; a claim rests on a `file:line` or a command.

## Otto and a worker (manager to worker, worker to manager)

- The manager's brief is short: goal, criteria, constraints — plus file pointers; never the plan, a pasted log or a
  file body. It starts with "Use the token-economy skill."
- The worker ends every turn with the fixed REPORT (state, changed in ≤5 lines, files, verified, question with a
  recommendation, artifacts). No recap before it, no prose around it that repeats it.
- The reply answers only what was asked; extra findings go in a file named in `artifacts`, not in the message.
- The manager answers a question with the answer alone — no restating of the question, no thanks, no summary.
