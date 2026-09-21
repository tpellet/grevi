#!/usr/bin/env bash
# Run as: TYPESAFE_API_KEY_FILE=/path/to/key benchmarks/bench.sh   (sandbox disabled)
# Compares cold (no cache) and warm runs for each verb on fixed inputs.
set -euo pipefail
H=./target/release/jevify
LS=$(mktemp); ls /usr/bin > "$LS"
LOG=benchmarks/fixtures/cargo-fail.log
J=$(mktemp)
# --ignore-failure: exit 3 (abstain) and exit 1 (`is` no) are legitimate outcomes, and one
# flaky network error must not abort the whole run; hyperfine records the failure and continues.
hyperfine --ignore-failure --warmup 1 --runs 15 --export-markdown benchmarks/results.md --export-json "$J" \
  -n "pick cold"            "JEVIFY_NO_CACHE=1 $H pick 'compress files' < $LS" \
  -n "pick warm"            "$H pick 'compress files' < $LS" \
  -n "is cold"              "JEVIFY_NO_CACHE=1 $H is 'mentions compression' < $LS || true" \
  -n "why cold"             "JEVIFY_NO_CACHE=1 $H why < $LOG" \
  -n "route cold archive"   "JEVIFY_NO_CACHE=1 $H route extract a tar archive" \
  -n "route cold dvd"       "JEVIFY_NO_CACHE=1 $H route burn a dvd from this iso" \
  -n "rg baseline"          "rg -c compress $LS || true"
# hyperfine's markdown has mean/min/max only; the README quotes p50/p95, so append them from the
# per-run times (linear interpolation between order statistics). "failed" counts non-zero exits.
python3 - "$J" >> benchmarks/results.md <<'EOF'
import json, sys
def pct(xs, p):
    xs = sorted(xs); k = (len(xs) - 1) * p; f = int(k); c = min(f + 1, len(xs) - 1)
    return xs[f] + (xs[c] - xs[f]) * (k - f)
print("\n| Run | p50 [ms] | p95 [ms] | runs | failed |\n|:---|---:|---:|---:|---:|")
for r in json.load(open(sys.argv[1]))["results"]:
    t = r["times"]; failed = sum(1 for c in r["exit_codes"] if c != 0)
    print(f"| `{r['command']}` | {pct(t, .5) * 1000:.0f} | {pct(t, .95) * 1000:.0f} | {len(t)} | {failed} |")
EOF
