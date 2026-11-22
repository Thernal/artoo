# Build output

## What Claude Code does on its own

A Bash result over 30,000 characters is saved to a file and the model gets the path and a 2,000-character
preview. A failed command over ~10,000 characters gets a head-and-tail excerpt with no file — and the error in
the middle of a Gradle log is the part that goes. (`bashOutputMaxChars`, 4,000–128,000, moves the 30k.) Either
way a build can put ~7k tokens into the context, every time it runs, and every later turn reads them again.

## The trim hook

`scripts/trim-build.sh` in this skill. Install it in a repo (run there):

```sh
sh <skill dir>/scripts/trim-build.sh install        # merges into .claude/settings.local.json
```

`<skill dir>` is `.claude/skills/token-economy` for a project install, `${CLAUDE_CONFIG_DIR:-~/.claude}/skills/token-economy`
for a user one. It merges, keeping every other key, a PreToolUse hook on Bash and the allow rule
`Bash(sh '<absolute path>/trim-build.sh' filter)`; an earlier install from another path is replaced, not doubled.
Local settings, because the path is this machine's. A new session picks it up; `/hooks` lists it.

What it does:

- **Rewrites** a command whose last step is `gradlew` (any path to it) or `gradle`, `cargo` with any `-flags` or
  `+toolchain` then `build|test|check|clippy|nextest|bench`, `xcodebuild`, or `swift build|test` — env
  assignments, `cd … &&` and `otto queue --` in front are fine — into `<command> 2>&1 | sh '<script>' filter`.
- **Leaves alone** a command that already pipes or redirects to a file, ends in `&`, has a trailing `# comment`,
  contains `TRIM_BUILD=0`, has its build before the last step (`./gradlew build && adb install …`), or has an odd
  number of quotes in the last step — quotes are not parsed, so `git commit -m "fix; ./gradlew build"` is not
  mistaken for a build. A form it does not know (`cargo -C dir test`, `bash gradlew`) runs untouched.
- **Filter**: up to 80 lines pass whole. A failure — `BUILD FAILED`, `FAILURE: Build failed`, `error[E…]:`,
  `error: could not compile`, `e: `, `: error: `, `** BUILD FAILED **`, `test result: FAILED`, a panic, a shell's
  "not found" — prints every line that matches those or `error:`, `ERROR`, `FAILED`, `failed`, `What went wrong`,
  `-->`, `Caused by:`, `Exception`, `assert`, with three lines after each, at most 150, then the last 10. A pass
  prints its summary lines and the last 12. A footer names the whole log in `$TMPDIR` — grep it when the cut is not
  enough.
- **Exit code**: the pipe loses the build's own, so the filter exits 1 on a failure it recognises and 0 otherwise.
  An exit of 0 does not prove the build passed — read the lines it printed.
- Tuning: `TRIM_BUILD_KEEP` (default 80), `TRIM_BUILD_MAX` (150). Needs `jq`; without it the hook does nothing.

Checked on Claude Code 2.1.329 (2026-11-20): a failing 406-line Gradle log reached the model as 18 lines, flagged
as an error.

### Why it is built this way

- **PreToolUse, not PostToolUse.** PostToolUse's `updatedToolOutput` does replace a successful Bash result, but a
  command that exits non-zero fires PostToolUseFailure instead, which cannot — and failures are the logs worth
  cutting.
- **A plain pipe, and an exact allow rule.** A rewritten command is checked against permission rules as rewritten.
  `<cmd> 2>&1 | sh '<path>' filter` passes with the build's own rule plus the filter's; a `{ …; }` group or a
  `set -o pipefail;` in front is never matched by an allow rule, and a wildcard rule
  (`Bash(sh *trim-build.sh filter)`) does not match the quoted path. Without the rule every build asks.
- **No `permissionDecision: "allow"`** in the hook: that would approve every build command whatever the rules say.

## By hand, without the hook

- `./gradlew build -q 2>&1 | tail -n 40`; Gradle's `--console=plain` drops progress lines.
- `cargo build -q 2>&1 | grep -E '^(error|warning)' -A6 | head -n 80`; `cargo test -q` prints one line per pass.
- `xcodebuild … 2>&1 | grep -E 'error:|FAILED|failed' | head -n 50`, or `xcbeautify --quiet` when installed.
- Long-running builds with `run_in_background`, then grep the output file for the failure.
