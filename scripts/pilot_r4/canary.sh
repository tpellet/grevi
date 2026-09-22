#!/bin/sh
# From the hunch-iu8 pilot (benchmarks/agents/PILOT.md). Lives in an evaluation directory
# EVAL/r4/ next to EVAL/sets/ (task inputs) and EVAL/r3_prompts.py (the eight tasks); set
# JEVAL_DIR to EVAL to run it from here.
# The containment canaries of the 2026-09-20 protocol (EXPERIMENT.md, stage 1), run through the
# arm wrappers exactly as an agent would: writes outside the run, reads of a sibling sentinel and
# of another run, the service credential, the jevify binaries for the control, a write inside the
# run, a temp file, network for gh. Output is the canary record; check.sh scans afterwards.
R4="${JEVAL_R4:-$(cd "$(dirname "$0")" && pwd)}"
for arm in with without; do
  D="$R4/runs/TC$arm"
  echo "===== canary arm=$arm"
  "$R4/check.sh" mark "$D"
  echo "C1 write into a repo:";        "$D/bin/run" 'touch ~/Projects/testify/CANARY-r4; echo exit=$?'
  echo "C2 write into /tmp:";          "$D/bin/run" 'echo x > /tmp/canary-r4.txt; echo exit=$?'
  echo "C3 read the sentinel:";        "$D/bin/run" "cat $R4/runs/_sentinel/gold.txt; echo exit=\$?"
  echo "C3b list another run:";        "$D/bin/run" "ls $R4/runs/T1with/; echo exit=\$?"
  echo "C4 key readable:";             "$D/bin/run" 'test -r ~/.ssh/typesafe-ai-key && echo readable || echo denied'
  echo "C5 installed jevify:";         "$D/bin/run" '~/.cargo/bin/jevify --version; echo exit=$?'
  echo "C5b jevify from target/:";     "$D/bin/run" '~/Projects/jevify/target/release/jevify --version; echo exit=$?'
  echo "C6 write inside work:";        "$D/bin/run" 'echo ok > ok.txt; pwd; ls; echo exit=$?'
  echo "C7 mktemp:";                   "$D/bin/run" 'f=$(mktemp); echo $f; echo exit=$?'
  echo "C8 gh over the network:";      "$D/bin/run" 'gh run list -R tpellet/jevify -L 1 >/dev/null; echo exit=$?'
  echo "C9 a Bash call outside the wrapper (what the harness does not stop):"
  mkdir -p "$R4/canary-direct" && echo direct > "$R4/canary-direct/TC$arm.txt" && echo written
  "$R4/check.sh" scan "$D"
done
echo "===== with arm: jevify through the wrapper, live"
D="$R4/runs/TCwith"
"$D/bin/run" "printf 'alpha the cat sleeps\nbeta the dog barks\n' | $D/bin/jevify pick 'the one about a dog'; echo exit=\$?"
"$D/bin/run" "$D/bin/jevify route --json 'render a terminal tape into a gif' | head -c 200; echo; echo exit=\$?"
cut -c1-700 "$D/jevify.jsonl"
ls "$D/cache"
ls ~/Projects/testify/CANARY-r4 /tmp/canary-r4.txt 2>&1
