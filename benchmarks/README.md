# Benchmarks

Measured end-to-end latency of each `hunch` verb on fixed inputs, with `rg` as the local-tool
baseline. Every latency number quoted anywhere for hunch (README, `--help`, launch posts) comes
from `results.md`, per verb, as p50 and p95, with the conditions below. There is no global
"hunch takes N ms" claim: the verbs differ by an order of magnitude.

## How to run

```sh
cargo build --release
TYPESAFE_API_KEY_FILE=/path/to/key benchmarks/bench.sh
```

`bench.sh` needs `hyperfine`, `rg` and `python3`. It writes `results.md` (hyperfine's table, then a
p50/p95 table computed from the per-run times). Live API calls: each cold run is one or more real
Jev requests, so the numbers include the network.

## Conditions

| | |
|:---|:---|
| Date | 2026-09-19 |
| Machine | a typical macOS dev machine: Apple M4 Pro, 24 GB, macOS 26.6.2 |
| Network | consumer Wi-Fi; to `api.typesafe.ai`: TCP connect ~80 ms, TLS done ~160 ms, empty HTTPS round trip ~240 ms (curl, 3 samples) |
| Model | `jev-1.13.0` (hunch's pinned default) |
| Toolchain | hunch 0.1.0, `cargo build --release`, rustc 1.93.1 |
| hyperfine | 1.20.0, `--warmup 1 --runs 15 --ignore-failure` |
| ripgrep | 15.2.0 |
| Inputs | `pick`/`is`: `ls /usr/bin` (924 lines on this macOS); `why`: `fixtures/cargo-fail.log` (a real failing `cargo build`, 12 lines); `run`: the two requests in `bench.sh` |

"cold" sets `HUNCH_NO_CACHE=1` (every answer is a live request); "warm" replays the answer cache.
`run` numbers include the installed-tool inventory read from its cache (the warm-up run built it);
"route-only" is `--no-args`, "full" adds the argument round and the man-page renders.

## Numbers

p50/p95 are linear-interpolated order statistics over 15 runs (`results.md`, second table);
mean ± σ from hyperfine's own table. All 15 runs of every row exited 0.

| Run | p50 | p95 | mean ± σ |
|:---|---:|---:|---:|
| `pick cold` | 721 ms | 799 ms | 719.6 ± 53.8 ms |
| `pick cold no-prewarm` | 738 ms | 849 ms | 750.5 ± 66.5 ms |
| `pick warm` (cache hit) | 7 ms | 12 ms | 7.6 ± 2.6 ms |
| `is cold` | 434 ms | 512 ms | 445.6 ± 38.9 ms |
| `why cold` | 602 ms | 676 ms | 600.5 ± 46.9 ms |
| `run cold route-only` | 1,879 ms | 2,003 ms | 1,889.8 ± 68.4 ms |
| `run cold full` | 1,946 ms | 2,110 ms | 1,989.1 ± 80.9 ms |
| `rg baseline` (`rg -c compress`) | 2 ms | 3 ms | 2.4 ± 0.3 ms |

Reading the table:

- `is` is one request: its p50 (~430 ms) is close to the network round trip above, so most of it
  is the API, not hunch.
- `pick` and `why` are two rounds (a window Choice, then finals) on a ~900-line input.
- `run cold full` is the number people feel when they type `, <something>`: **p50 is about 1.9 s,
  above 1 s**, and the user-facing README must say so. Route-only saves only ~70 ms at p50: the
  time is in the routing rounds over the inventory, not in argument pointing.
- `rg baseline` sits under hyperfine's 5 ms shell-calibration floor (hyperfine warns about it), so
  read it as "a few milliseconds", not as a precise figure. hunch is not competing with `rg` on
  speed; the row shows what a local tool costs on the same input.
- Warm `pick` (a cache hit) is ~7 ms: the cache makes a repeated question effectively free.

## Prewarm decision: removed

hunch used to open the TLS connection early (a `GET /v1/models` spawned before stdin/inventory
were read) so the first real request would find a pooled connection; `HUNCH_NO_PREWARM=1`
disabled it. The decision rule (plan, Task 12 Step 4): keep prewarm only if `pick cold` is at
least 50 ms faster at p50 than `pick cold no-prewarm`.

| Run | p50 | p95 | mean ± σ |
|:---|---:|---:|---:|
| `pick cold` (prewarm on) | 721 ms | 799 ms | 719.6 ± 53.8 ms |
| `pick cold no-prewarm` | 738 ms | 849 ms | 750.5 ± 66.5 ms |

Prewarm gained 17 ms at p50, under the 50 ms bar. An earlier full run of the same script on the
same day (before the p50/p95 table was added to `bench.sh`) had prewarm on the wrong side of zero:
716.0 ± 51.7 ms with prewarm vs 697.2 ± 51.2 ms without (means). Reading a ~900-line stdin takes
well under a millisecond, so the spawned GET had almost no local work to overlap with; the gain
is within run-to-run noise (σ ≈ 50–65 ms).

Removed, in the same commit: `Client::prewarm`, the `prewarm` field of `Config`, the
`HUNCH_NO_PREWARM` variable (also from `capabilities()`), the four call sites in `is`, `pick`,
`why` and `run`, and the "pick cold no-prewarm" line of `bench.sh`. `results.md` is the 8-row run
that made the decision, produced before that line was removed; a rerun of the current script
yields 7 rows.

### Check after the removal, and a finding on `run`

Same day, same conditions, the rebuilt binary (prewarm code gone), 15 runs each, means ± σ:
`pick cold` 709.6 ± 56.0 ms, `is cold` 405.0 ± 45.0 ms, `why cold` 603.2 ± 53.3 ms — unchanged
within noise — but `run cold full` 2,252 ± 137 ms against 1,989 ± 81 ms in the decision run.

An A/B on the pre-removal binary, `run cold full`, prewarm on vs `HUNCH_NO_PREWARM=1`, run in
ABBA order so network drift cancels (15 runs per arm, means ± σ):

| Arm | prewarm | mean ± σ |
|:---|:---|---:|
| A1 | on | 2,001 ± 109 ms |
| B1 | off | 2,209 ± 120 ms |
| B2 | off | 2,260 ± 154 ms |
| A2 | on | 2,127 ± 143 ms |

Prewarm on averages 2,064 ms, off 2,235 ms: about 170 ms (8%) in favour of prewarm, in both
orderings. The difference between the verbs is the local work available to overlap with the TLS
handshake: `run` spends ~1 s of CPU reading its tool inventory before its first request; `pick`
reads a 900-line stdin in under a millisecond. Two identical arms (the same binary twice) differed
by 10 ms in the same session, so 170 ms is well above the noise between arms.

The decision rule is defined on `pick`, so prewarm is removed as the plan requires. If `run`'s
latency is worth 170 ms, the evidence here supports re-adding prewarm for `run` only (the verb
with local work to overlap), decided on a `run cold full` A/B like the one above.
