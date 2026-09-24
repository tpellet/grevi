#!/usr/bin/env bash
# One sweep point: N candidates through `fill` on one backend at one concurrency.
# Appends one JSON object per attempt to evals/capacity/sweep.jsonl.
#
#   evals/capacity/sweep.sh <backend> <concurrency> <n> <run>
#
# The record holds what the bead asks for per attempt: the HTTP status jevify
# met (parsed out of its own message; null on success), wall time, the request
# count the client made, and whether a retry was sent and then succeeded.
set -uo pipefail
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
root=$(cd -- "$here/../.." && pwd)
backend=${1:?} conc=${2:?} n=${3:?} run=${4:?}
out=$here/sweep.jsonl
export JEVIFY_NO_CACHE=1 JEVIFY_BACKEND=$backend JEVIFY_CONCURRENCY=$conc
[ "$backend" = typesafe ] && export TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key
started=$(date +%s.%N)
"$here/gen.sh" "$n" | "$root/target/release/jevify" fill --json --dry-run -- \
  echo '@{-:the one that stops sending a request after the deadline}' \
  >"$here/.last.json" 2>"$here/.last.err"
code=$?
wall=$(echo "$(date +%s.%N) - $started" | bc)
python3 - "$backend" "$conc" "$n" "$run" "$code" "$wall" "$here" >>"$out" <<'PY'
import json, sys, re, pathlib
backend, conc, n, run, code, wall, here = sys.argv[1:8]
here = pathlib.Path(here)
try:
    env = json.loads((here / ".last.json").read_text())
except Exception:
    env = {}
err = (here / ".last.err").read_text().strip()
meta, tel = env.get("meta") or {}, ((env.get("meta") or {}).get("telemetry") or {})
posts = tel.get("inference_posts") or {}
msg = ((env.get("error") or {}).get("message") or "") + " " + err
status = re.search(r"HTTP (\d{3})", msg)
retries = tel.get("retry_sends", 0)
print(json.dumps({
    "backend": backend, "concurrency": int(conc), "candidates": int(n), "run": int(run),
    "exit": int(code), "wall_s": round(float(wall), 2),
    "http_status": int(status.group(1)) if status else None,
    "requests": meta.get("requests"),
    "posts_attempted": posts.get("attempted"), "posts_succeeded": posts.get("succeeded"),
    "posts_failed": posts.get("failed"), "posts_cancelled": posts.get("cancelled"),
    "retry_sends": retries,
    "retry_wait_ms": tel.get("retry_sleep_ms", 0),
    # A retry that succeeded: jevify sent one and the verb still answered.
    "retry_succeeded": (retries > 0 and int(code) in (0, 3)) if retries else None,
    "classifications": tel.get("semantic_questions"),
    "error_kind": (env.get("error") or {}).get("kind"),
    "message": (err or "")[:300],
}))
PY
echo "$backend c=$conc n=$n run=$run exit=$code ${wall}s"
