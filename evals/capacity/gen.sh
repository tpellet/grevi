#!/usr/bin/env bash
# Prints N distinct commit-subject-shaped candidate lines on stdout.
# Deterministic: the same N always yields the same list, so every run of the
# sweep judges the same input.
set -euo pipefail
n=${1:?usage: gen.sh N}
verbs=(add fix refactor rename document inline extract guard cache widen narrow drop restore split merge pin unpin relax tighten teach)
objs=(retry backoff window batch cache deadline threshold marker listing envelope telemetry inventory finalist shortlist excerpt recipe lister handle probe gate)
tails=("so a failed request is sent again" "without touching the shared index" "when the service names a wait" "before the first request leaves" "on the keyless backend only" "for a list that arrives newest first" "under a single overall deadline" "when the subject says nothing" "so the finals keep their evidence" "and report the coverage")
for ((i = 0; i < n; i++)); do
  printf '%s the %s %s (#%d)\n' \
    "${verbs[$((i % 20))]}" \
    "${objs[$(((i / 20) % 20))]}" \
    "${tails[$(((i / 400) % 10))]}" \
    "$i"
done
