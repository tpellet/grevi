# Benchmarks

Measured end-to-end latency of each `grevi` verb on fixed inputs, with `rg` as the local-tool
baseline. Every latency number quoted anywhere for grevi (README, `--help`, launch posts) comes
from `results.md`, per verb, as p50 and p95, with the conditions below. There is no global
"grevi takes N ms" claim: the verbs differ by an order of magnitude.

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
| Model | `jev-1.13.0` (grevi's pinned default) |
| Toolchain | grevi 0.1.0, `cargo build --release`, rustc 1.93.1 |
| hyperfine | 1.20.0, `--warmup 1 --runs 15 --ignore-failure` |
| ripgrep | 15.2.0 |
| Inputs | `pick`/`is`: `ls /usr/bin` (924 lines on this macOS); `why`: `fixtures/cargo-fail.log` (a real failing `cargo build`, 12 lines); `run`: the two requests in `bench.sh` |

"cold" sets `GREVI_NO_CACHE=1` (every answer is a live request); "warm" replays the answer cache.
`run` numbers include the installed-tool inventory read from its cache (the warm-up run built it);
"route-only" is `--no-args`, "full" adds the argument round and the man-page renders.

## Numbers

p50/p95 are linear-interpolated order statistics over 15 runs (`results.md`, second table);
mean ± σ from hyperfine's own table. All 15 runs of every row exited 0.

| Run | p50 | p95 | mean ± σ |
|:---|---:|---:|---:|
| `pick cold` | 739 ms | 804 ms | 741.7 ± 40.4 ms |
| `pick warm` (cache hit) | 6 ms | 10 ms | 6.8 ± 2.0 ms |
| `is cold` | 454 ms | 488 ms | 451.0 ± 29.4 ms |
| `why cold` | 638 ms | 732 ms | 644.6 ± 52.7 ms |
| `run cold route-only` | 1,823 ms | 2,007 ms | 1,849.4 ± 80.2 ms |
| `run cold full` | 1,940 ms | 2,251 ms | 1,984.5 ± 129.4 ms |
| `rg baseline` (`rg -c compress`) | 3 ms | 4 ms | 3.3 ± 0.3 ms |

This is the run of the current script on the current binary (connection prewarm in `run` only,
see below), later on the same day as the decision run in the next section; that earlier 8-row
run had `pick cold` 721/799 ms, `is cold` 434/512 ms, `why cold` 602/676 ms, `run cold full`
1,946/2,110 ms (p50/p95), so the verbs without prewarm moved within run-to-run noise.

Reading the table:

- `is` is one request: its p50 (~450 ms) is close to the network round trip above, so most of it
  is the API, not grevi.
- `pick` and `why` are two rounds (a window Choice, then finals) on a ~900-line input.
- `run cold full` is the number people feel when they type `, <something>`: **p50 is about 1.9 s,
  above 1 s**, and the user-facing README must say so. Route-only saves only ~70–120 ms at p50
  (70 ms in the earlier run, 117 ms here): the time is in the routing rounds over the inventory,
  not in argument pointing.
- `rg baseline` sits under hyperfine's 5 ms shell-calibration floor (hyperfine warns about it), so
  read it as "a few milliseconds", not as a precise figure. grevi is not competing with `rg` on
  speed; the row shows what a local tool costs on the same input.
- Warm `pick` (a cache hit) is ~7 ms: the cache makes a repeated question effectively free.

## Prewarm decision: `run` only

Connection prewarm: `Client::prewarm` spawns a `GET /v1/models` before the verb's local work so
the first real request finds a pooled TLS connection. Decided twice, on the same day, on
evidence: removed everywhere (Task 12, measured on `pick`), then re-added for `run` alone
(bead grevi-n2h, measured on `run cold full`). Only `run` calls it today; there is no
`GREVI_NO_PREWARM` switch.

### Round 1 (Task 12): removed, measured on `pick`

grevi used to open the connection early in every verb; `GREVI_NO_PREWARM=1` disabled it. The
decision rule (plan, Task 12 Step 4): keep prewarm only if `pick cold` is at least 50 ms faster
at p50 than `pick cold no-prewarm`.

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
`GREVI_NO_PREWARM` variable (also from `capabilities()`), the four call sites in `is`, `pick`,
`why` and `run`, and the "pick cold no-prewarm" line of `bench.sh`. The 8-row `results.md` that
made that decision was replaced by the 7-row run above when round 2 landed.

### Check after the removal, and a finding on `run`

Same day, same conditions, the rebuilt binary (prewarm code gone), 15 runs each, means ± σ:
`pick cold` 709.6 ± 56.0 ms, `is cold` 405.0 ± 45.0 ms, `why cold` 603.2 ± 53.3 ms — unchanged
within noise — but `run cold full` 2,252 ± 137 ms against 1,989 ± 81 ms in the decision run.

An A/B on the pre-removal binary, `run cold full`, prewarm on vs `GREVI_NO_PREWARM=1`, run in
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

The decision rule is defined on `pick`, so prewarm was removed as the plan required, and the
`run` finding was left for a follow-up decision on a `run cold full` A/B like the one above.

### Round 2 (grevi-n2h): re-added for `run` only, measured on `run cold full`

Same day, same conditions (a typical macOS dev machine, consumer Wi-Fi, `jev-1.13.0`, hyperfine
1.20.0, `--warmup 1 --runs 15 --ignore-failure`, `GREVI_NO_CACHE=1`, the `bench.sh` "run cold
full" command). Two release binaries built from the same tree: the incumbent (`main`, no prewarm)
and the candidate (`Client::prewarm` re-added and called once, at the top of `run`, before the
inventory `spawn_blocking`; no other verb calls it). ABBA order so network drift cancels; the
bar is the one Task 12 used, a p50 improvement of at least 50 ms. p50/p95 are the same
order statistics as the table above; all 60 runs exited 0.

| Arm | prewarm | p50 | p95 | mean ± σ |
|:---|:---|---:|---:|---:|
| A1 candidate | on | 2,034 ms | 2,451 ms | 2,094 ± 210 ms |
| B1 incumbent | off | 2,122 ms | 2,268 ms | 2,139 ± 85 ms |
| B2 incumbent | off | 2,203 ms | 2,584 ms | 2,237 ± 202 ms |
| A2 candidate | on | 1,920 ms | 2,017 ms | 1,924 ± 64 ms |
| candidate pooled (A1+A2, 30 runs) | on | 1,960 ms | 2,255 ms | 2,009 ms |
| incumbent pooled (B1+B2, 30 runs) | off | 2,165 ms | 2,429 ms | 2,188 ms |

Prewarm wins in both orderings (A1 vs B1: 88 ms; A2 vs B2: 283 ms) and by 205 ms at the pooled
p50, four times the bar; p95 improves by 174 ms. The A1 p95 carries one 2.7 s outlier. The
mechanism is the one round 1 found: `run` spends about 1 s of CPU on its inventory before its
first request, and the handshake (~160 ms on this network) now runs underneath it. Kept, for
`run` only: `pick`, `is` and `why` have no local work to overlap and stay as round 1 left them.
The 7-row table at the top of this file is the full `bench.sh` run on the candidate binary,
taken right after the A/B: `run cold full` 1,940/2,251 ms, back at the number round 1's
decision run had measured with prewarm on.
