# evals/capacity — what a `fill` marker holds before the backend refuses

The sweep behind "The `fill` capacity both backends serve" in `benchmarks/results.md`.

`gen.sh N` prints N distinct commit-subject-shaped lines, the same N always giving the same
list, so every attempt judges the same input. `sweep.sh <backend> <concurrency> <n> <run>` pipes
that list into `fill` under an `@{-:…}` marker with `--dry-run` and appends one JSON object per
attempt to `sweep.jsonl`: the HTTP status jevify met, wall time, the requests and
classifications it spent, the retries it sent, and whether a retry then carried the verb to an
answer. `.last.json` and `.last.err` hold the envelope and the stderr of the most recent
attempt.

```bash
for conc in 4 2 1; do
  for run in 1 2 3; do
    for n in 500 750 1000 1250 1500 2000 3000 3267; do
      evals/capacity/sweep.sh classifier $conc $n $run
    done
  done
done
```

TypeSafe takes `typesafe` in place of `classifier`, concurrencies 8, 2 and 1, and candidate
counts up to 13,200. `sweep.sh` sets `JEVIFY_NO_CACHE=1` itself and reads the key from
`TYPESAFE_API_KEY_FILE=$HOME/.ssh/typesafe-ai-key`. Exit 0 and exit 3 are both answers: the
sweep measures the request path, so whether the winner is decided or the field is too close
does not count as a failure. A keyless pass over the eight points at one concurrency costs 228
classifications against the 20,000 a day the service allows one IP.

`stub.py <port> <status> [retry-after]` stands in for the backend on 127.0.0.1 and answers every
POST with one status, which times the failure path without spending quota:

```bash
python3 evals/capacity/stub.py 18301 429 &
evals/capacity/gen.sh 1500 | JEVIFY_NO_CACHE=1 JEVIFY_BASE_URL=http://127.0.0.1:18301 \
  jevify fill --dry-run -- echo '@{-:the one that stops sending a request after the deadline}'
```
