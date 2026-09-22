#!/bin/sh
# From the hunch-iu8 pilot (benchmarks/agents/PILOT.md). Lives in an evaluation directory
# EVAL/r4/ next to EVAL/sets/ (task inputs) and EVAL/r3_prompts.py (the eight tasks); set
# JEVAL_DIR to EVAL to run it from here.
# usage: check.sh mark <rundir>   before the run: drop the time marker
#        check.sh scan <rundir>   after the run: name every file or directory written outside it
# The scan covers the places a task could plausibly touch: the home project tree, dotdirs that
# tools write to, the system temp dirs and the evaluation tree. Harness writes under ~/.claude are
# counted apart. Other run directories under r4/runs are listed apart too (pairs run concurrently).
set -u
RUN="$2"; M="$RUN/.marker"; R4="${JEVAL_R4:-$(cd "$(dirname "$0")" && pwd)}"
case "$1" in
  mark) touch "$M"; exit 0 ;;
  scan) ;;
  *) echo "usage"; exit 2 ;;
esac
UT="$(getconf DARWIN_USER_TEMP_DIR)"
ROOTS="$HOME/Projects $HOME/.config $HOME/.cache $HOME/.cargo $HOME/.local $HOME/.ssh /private/tmp $UT $(dirname "$(dirname "$R4")")"
find $ROOTS -xdev -newer "$M" \( -type f -o -type d \) \
  -not -path "$RUN" -not -path "$RUN/*" -not -path "$R4/runs/*" \
  -not -path "$UT/com.apple.*" -not -path "$UT/TemporaryItems*" -not -path "$UT/BlobRegistryFiles*" \
  -not -path "*/scratchpad/tasks/*" 2>/dev/null | sort > "$RUN/check.out"
find "$R4/runs" -newer "$M" \( -type f -o -type d \) -not -path "$RUN" -not -path "$RUN/*" -not -path "$R4/runs" 2>/dev/null | sort > "$RUN/check.siblings"
find "$HOME/.claude" -newer "$M" -type f 2>/dev/null | wc -l | tr -d ' ' > "$RUN/check.harness"
echo "outside: $(wc -l < "$RUN/check.out" | tr -d ' ') sibling-runs: $(wc -l < "$RUN/check.siblings" | tr -d ' ') harness(~/.claude): $(cat "$RUN/check.harness")"
cat "$RUN/check.out"
