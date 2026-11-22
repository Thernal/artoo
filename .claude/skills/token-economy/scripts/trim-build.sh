#!/bin/sh
# Keeps a build's whole log out of the model's context: the failing lines and the tail reach it, the full log
# goes to a file it can grep.
#
#   sh trim-build.sh hook     PreToolUse hook on Bash (reads the hook's JSON on stdin). When the command's last
#                             step is a Gradle, cargo, xcodebuild or swift build/test, it becomes
#                             `<command> 2>&1 | sh '<this script>' filter`. Anything else, or a command that
#                             already pipes or redirects, passes untouched.
#   sh trim-build.sh filter   reads a build's output on stdin and prints the trimmed version. Exits 1 when the
#                             output shows the build failed (a pipe loses the build's own exit code), else 0.
#
#   sh trim-build.sh install [settings file]
#                             merges the hook and its allow rule into a settings file (default
#                             .claude/settings.local.json of the current directory), keeping every other key.
#
# The rewritten command is checked against permission rules as written, so the settings that add the hook also
# allow `Bash(sh '<this script>' filter)` with the exact path (a wildcard rule does not match it) — without it,
# every build asks.
# Opt out for one command: put TRIM_BUILD=0 in it. Tune: TRIM_BUILD_KEEP (print everything up to this many
# lines, default 80), TRIM_BUILD_MAX (most matched lines kept, default 150).
# Needs jq for `hook`; without jq the hook does nothing and the command runs as written.
set -u

hook() {
    command -v jq >/dev/null 2>&1 || exit 0
    case $0 in /*) self=$0 ;; *) self="$(pwd)/$0" ;; esac
    jq -c --arg self "$self" --arg q "'" '
        (.tool_input // {}) as $in | ($in.command // "") as $c
        | ($c | [splits("&&|;|\n")] | last // "") as $step
        | def build: test("^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+)*(otto[[:space:]]+queue[[:space:]]+--[[:space:]]+)?([^[:space:]]*/)?(gradlew|gradle|xcodebuild|cargo([[:space:]]+[-+][^[:space:]]+)*[[:space:]]+(build|b|test|t|check|c|clippy|nextest|bench)|swift[[:space:]]+(build|test))([[:space:]]|$)");
          # already shaped (a pipe, a redirect to a file, a background job, a trailing comment), opted out, or
          # the build sits inside a quoted string (quotes are not parsed: an odd count in the last step)
          def shaped: test("\\||>[[:space:]]*[^&[:space:]]|&[[:space:]]*$|[[:space:]]#|TRIM_BUILD=0|trim-build[.]sh");
          def quoted: ([match("\""; "g")] | length) % 2 == 1 or ([match($q; "g")] | length) % 2 == 1;
        if ($step | build) and ($step | quoted | not) and ($c | shaped | not) then
            {hookSpecificOutput: {hookEventName: "PreToolUse",
              updatedInput: ($in | .command = "\($c | sub("([[:space:]]+2>&1)?[[:space:]]*$"; "")) 2>&1 | sh \($q)\($self)\($q) filter")}}
        else empty end'
    exit 0
}

filter() {
    keep=${TRIM_BUILD_KEEP:-80} max=${TRIM_BUILD_MAX:-150}
    dir=${TMPDIR:-/tmp}; dir=${dir%/}
    log=$(mktemp "$dir/trim-build.XXXXXX") || { cat; exit 0; }
    cat >"$log"
    # awk and grep -E read these as EREs; brackets, not backslashes, quote the special characters (awk -v
    # turns backslashes into escapes)
    # a failure these lines do not show exits 0: the pipe has lost the build's own exit code
    failed='BUILD FAILED|FAILURE: Build failed|^error([[]E[0-9]+[]])?:|error: could not compile|test result: FAILED|^e: |: error: |[*][*] [A-Z ]+ FAILED [*][*]|^FAILED|panicked at|^(sh|bash|zsh|dash): .*(not found|No such file|Permission denied)'
    rc=0; grep -Eq "$failed" "$log" && rc=1
    n=$(wc -l <"$log" | tr -d ' ')
    if [ "$n" -le "$keep" ]; then
        cat "$log"; rm -f "$log"; exit "$rc"
    fi
    if [ "$rc" -eq 0 ]; then
        pat='BUILD SUCCESSFUL|^test result:|Executed [0-9]+ tests?|[*][*] [A-Z ]+ SUCCEEDED [*][*]|generated [0-9]+ warnings?'
        after=0 tail=12
    else
        pat="$failed"'|error:|[[]ERROR[]]|ERROR:|FAILED|What went wrong|[Ff]ailed|^failures:|^[[:space:]]+--> |Caused by:|Exception|[Aa]ssert'
        after=3 tail=10
    fi
    awk -v n="$n" -v pat="$pat" -v after="$after" -v tail="$tail" -v max="$max" '
        { keep = 0
          if ($0 ~ pat) left = after + 1
          if (left > 0) { keep = 1; left-- }
          if (FNR > n - tail) keep = 1
          else if (keep && shown >= max) keep = 0
          if (keep) { if (last && FNR > last + 1) print "  …"; print; last = FNR; shown++ } }' "$log"
    if [ "$rc" -eq 0 ]; then state=passed; else state=failed; fi
    printf '[trim-build] %s lines, the build %s — the whole log is %s (grep it, or read it in parts)\n' "$n" "$state" "$log"
    exit "$rc"
}

install() {  # [settings file] — merges the hook and its allow rule; other keys are kept
    command -v jq >/dev/null 2>&1 || { echo "trim-build: jq is needed to merge settings — add them by hand" >&2; exit 1; }
    case $0 in /*) self=$0 ;; *) self="$(pwd)/$0" ;; esac
    f=${1:-.claude/settings.local.json}
    mkdir -p "$(dirname "$f")"; [ -s "$f" ] || printf '{}\n' >"$f"
    # an earlier install from another path is replaced, not doubled
    jq --arg h "sh '$self' hook" --arg r "Bash(sh '$self' filter)" '
        (if .hooks.PreToolUse then .hooks.PreToolUse |= (map(.hooks |= map(select((.command // "") | test("trim-build[.]sh. hook$") | not))) | map(select(.hooks | length > 0))) else . end)
        | (if .permissions.allow then .permissions.allow |= map(select(test("trim-build[.]sh. filter[)]$") | not)) else . end)
        | .hooks.PreToolUse = ((.hooks.PreToolUse // []) + [{matcher: "Bash", hooks: [{type: "command", command: $h, timeout: 10}]}])
        | .permissions.allow = ((.permissions.allow // []) + [$r])' "$f" >"$f.tmp" \
        || { rm -f "$f.tmp"; echo "trim-build: $f is not valid JSON — nothing changed" >&2; exit 1; }
    mv "$f.tmp" "$f"
    echo "trim-build: hook and its allow rule are in $f (a new session picks them up)"
}

case ${1:-} in
    hook) hook ;;
    filter) filter ;;
    install) shift; install "$@" ;;
    *) echo "usage: sh trim-build.sh hook | filter | install [settings file]" >&2; exit 2 ;;
esac
