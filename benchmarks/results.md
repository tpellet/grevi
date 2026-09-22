| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `pick cold` | 741.7 ± 40.4 | 692.4 | 838.4 | 223.02 ± 25.10 |
| `pick warm` | 6.8 ± 2.0 | 5.6 | 13.6 | 2.05 ± 0.65 |
| `is cold` | 451.0 ± 29.4 | 377.5 | 494.2 | 135.62 ± 16.01 |
| `why cold` | 644.6 ± 52.7 | 585.1 | 792.2 | 193.82 ± 24.81 |
| `run cold route-only` | 1849.4 ± 80.2 | 1756.6 | 2027.7 | 556.10 ± 59.85 |
| `run cold full` | 1984.5 ± 129.4 | 1864.3 | 2325.3 | 596.74 ± 70.48 |
| `rg baseline` | 3.3 ± 0.3 | 2.9 | 4.0 | 1.00 |

| Run | p50 [ms] | p95 [ms] | runs | failed |
|:---|---:|---:|---:|---:|
| `pick cold` | 739 | 804 | 15 | 0 |
| `pick warm` | 6 | 10 | 15 | 0 |
| `is cold` | 454 | 488 | 15 | 0 |
| `why cold` | 638 | 732 | 15 | 0 |
| `run cold route-only` | 1823 | 2007 | 15 | 0 |
| `run cold full` | 1940 | 2251 | 15 | 0 |
| `rg baseline` | 3 | 4 | 15 | 0 |

## Keyless quota per verb (measured 2026-09-22)

jevify 0.7.0, backend classifier.dev without a key, one IP, `JEVIFY_CONCURRENCY=4`,
`JEVIFY_NO_CACHE=1`. Requests went through a counting proxy on 127.0.0.1 that forwards them
unchanged and records what jevify sent (`items`, `dimensions`) and what the service accounted
(`usage.classifications`, `RateLimit-Remaining`). jevify's own count is
`meta.telemetry.semantic_questions`.

The service's unit is one classification: one item under one dimension, so a request of
`items × dimensions`. Its policy header is `RateLimit-Policy: 3000;w=60, 20000;w=86400`, per IP,
and `RateLimit-Remaining` drops by exactly `usage.classifications` (2990 → 2970 → 2920 for
batches of 20 and 50). No header reports the day's remainder. On that day the service answered
with `inclusionai/ling-3.0-flash` (`usage.fallback`), not Jev, without scores, so jevify refused
every answer (exit 4 `api_protocol`) and no verb reached its second round; the requests were sent
and counted all the same. A window of 100 labels answered HTTP 502 `upstream_other` on most
attempts and was not counted.

| Verb | Input | Requests sent | jevify `semantic_questions` | Service `classifications` | Result |
|:---|:---|---:|---:|---:|:---|
| `is` | 1 statement | 1 | 1 | 1 | counted |
| `is` | 3 statements | 1 | 3 | 3 | counted |
| `filter` | 10, 20, 50, 60 records | 1 each | 10, 20, 50, 60 | 10, 20, 50, 60 | counted |
| `filter` | 75, 99, 100 records | 1 each | 75, 99, 100 | 0 | HTTP 402 `request_spending_limit` |
| `label` (3 labels) | 100 records | 1 | 100 | 0 | HTTP 402 `request_spending_limit` |
| `pick` | 50 lines | 1 | 2 | 2 | counted (choice over 51 labels + `any`) |
| `pick` | 156 lines (2 windows) | 8 (with retries) | 4 | 0 | HTTP 502 on every attempt |
| `why` | 156 lines (2 windows) | 4 | 4 | 4 | the 58-label window counted twice, the 100-label one HTTP 502 |
| `route` | PATH of 1,883 commands (20 windows) | 6 | 40 | 2 | one window counted, the others HTTP 502 |
| `route` | inventory of 40 tools | 4 | 2 | 0 | HTTP 502 on every attempt |

Round 2 of `pick`, `why` and `route`: NOT RUN (no round 1 completed). Its shape is one request:
one item with one choice and one `any` question for `pick` and `why` (2 classifications), one
item with one question per finalist, at most 12, for `route`. `fill` uses the shape of `pick`.
Total counted by the service over the session: 154 classifications.

Cost per call, from the shapes and the counts above:

| Verb | Classifications per call | Calls a day on one IP (20,000) |
|:---|:---|:---|
| `is` | 1 per statement | 6,600 (3 statements) to 20,000 (1) |
| `filter`, `label` | 1 per distinct record; a batch of 60 records is accepted, 75 is refused (HTTP 402) | 20,000 records in total: 333 calls of 60 records, 2,000 calls of 10 |
| `pick`, `why` | 2 per window of 99 lines, plus 2 for the final round when there is more than one window | 10,000 calls of at most 99 lines; 830 of 1,000 lines; 100 of 9,801 |
| `route` | 2 per window of 99 commands, plus 1 per finalist (at most 12) | 380 over a PATH of 1,883 commands; 340 over 2,200 |

Routing does not cost one classification per command on the PATH: a window of 99 commands is
one item under two dimensions. Twenty routing calls over 2,200 commands cost about 1,200
classifications, well under 20,000; the 429 of 2026-09-19 is not explained by this count.
