# hunch

[![CI](https://github.com/tpellet/hunch/actions/workflows/ci.yml/badge.svg)](https://github.com/tpellet/hunch/actions/workflows/ci.yml)

`cargo build 2>&1 | hunch why` points at the line that broke the build: the line itself, with its number and a probability, not a paraphrase of it.

```sh
cargo build 2>&1 | hunch why
ls | hunch pick "last month's electricity bill"
hunch is "asks for a refund" < mail.txt && ./refund
```

demo.gif: pending

Excerpts from a terminal, captured 2026-09-19 on `jev-1.13.0`:

```
$ cargo build 2>&1 | hunch why
>     2 │ error[E0425]: cannot find value `conifg` in this scope
      3 │  --> src/main.rs:3:20
      4 │   |
      5 │ 3 |     println!("{}", conifg);

$ hunch run --dry-run "count the lines in notes.txt"
hunch: wc (0.95) — word, line, character, and byte count
  -l  The number of lines in each input file is written to the standard output.
wc -l notes.txt

$ hunch run --yes "remove the file notes.txt"
hunch: not offering to run this (rm is on hunch's never-execute list); check it and run it yourself:
rm notes.txt

$ hunch is "asks for a refund" < mail.txt; echo $?
0
```

`is` prints nothing: the answer is the exit code (0 yes, 1 no, 3 unsure), so it composes with `&&`, `||` and `case`.

## Install

Shell installer, macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tpellet/hunch/releases/latest/download/hunch-installer.sh | sh
```

release binaries: pending

From source, with Rust 1.87 or newer:

```sh
cargo install --git https://github.com/tpellet/hunch --locked
```

The crates.io name `hunch` is taken, so there is no `cargo install hunch`.

Then point hunch at a key and try it:

```sh
export TYPESAFE_API_KEY=...
# or: export TYPESAFE_API_KEY_FILE=/path/to/key
hunch health
cargo build 2>&1 | hunch why
```

## Get a key

hunch is an independent open-source client of TypeSafe's hosted Jev API; you need your own key from https://console.typesafe.ai. Requests are billed to your key.

Each verb makes at least one API request; `-v` prints how many, and what they cost.

## Quick start

```sh
export TYPESAFE_API_KEY=...            # or TYPESAFE_API_KEY_FILE=/path/to/key
hunch health                           # ok: key accepted, API reachable in 286 ms
```

### pick: the line that matches

Stdin lines in, the matching line out. Exit 3 when nothing fits.

```sh
kill $(ps -eo pid,comm,%cpu | hunch pick "eating my battery" | awk '{print $1}')
git switch $(git branch | hunch pick "payment timeout fix")
```

`-n 3` prints up to three lines, each ranked above "nothing fits"; `--index` prints line numbers instead of lines.

### why: the line that broke it

```sh
cargo build 2>&1 | hunch why
hunch why -- cargo build
```

Compilers write errors to stderr, so pipe `2>&1`, or let `hunch why -- <cmd>` run the command and capture both streams. `-C 5` widens the context, `-n 3` reports up to three causes. Stdin with no error-like line exits 3 with a hint about stderr.

### is: a predicate on text

```sh
hunch is "asks for a refund" < mail.txt && ./refund
hunch is "asks for a refund" < mail.txt; case $? in 0) ./refund;; 1) ./archive;; 3) ./ask;; esac
```

Yes at or above 0.65, no below 0.35, unsure in between (`--band` sets the width around the 0.5 threshold).

### run: the installed tool that does it

```sh
hunch run "burn a dvd from this iso"
hunch run --dry-run "count the lines in notes.txt"
```

`run` routes the request to a tool installed on your machine, points at flags from that tool's man page, prints the command and asks before running it. `--dry-run` only proposes; `--yes` skips the question. A comma alias makes it a shell verb:

```sh
eval "$(hunch init zsh)"      # or bash
, "what's using port 8080"
```

Quote requests that contain an apostrophe: an unquoted `, what's using port 8080` opens a quote in both zsh and bash.

## How it works

hunch points, it does not generate. Every token it prints comes from your stdin, a tool on your PATH, that tool's man page, or your own request. A flag that is not in the man page cannot appear.

Jev, TypeSafe's model, answers two kinds of question. "Which one?" is a choice over the options plus NONE, and a candidate only has to beat NONE. "Is it?" is an absolute yes/no with a calibrated probability, gated by one threshold, 0.5 by default (`-t`). TypeSafe documents that the two kinds are not on one scale, so the threshold never touches a "which one" answer. Exit 3 means hunch abstained: nothing beat NONE, or the yes/no answer fell under the threshold.

The API takes at most 255 options per question. Past that, `pick` and `why` run a tournament: windows of 200 lines plus NONE, 3 finalists per window, then one finals round. A window's items share a 60,000-character budget (each clipped to 200–2,000 characters), which keeps a request under the model's 32k-token state limit for typical text. Every verb finishes in at most 2 rounds of parallel requests; `run` takes 3 (route, fit, arguments).

Answers are cached on disk for 7 days, keyed by a hash of the request; `--no-cache` bypasses the cache. The model is pinned to `jev-1.13.0`, the release the 0.5 threshold was calibrated on. `jev-latest` moves with each TypeSafe release, so the same input would start answering differently without any change here; `--model jev-latest` is allowed and documented as moving.

## Numbers

Latency, end to end, per verb. Measured 2026-09-19 on a typical macOS dev machine (Apple M4 Pro, 24 GB, macOS 26.6.2) over consumer Wi-Fi (an empty HTTPS round trip to `api.typesafe.ai` took ~240 ms), model `jev-1.13.0`, `hyperfine --warmup 1 --runs 15`; "cold" is `HUNCH_NO_CACHE=1`, "warm" is a cache hit. Inputs, script and the full table: [benchmarks/](benchmarks/README.md).

| Run | p50 | p95 | n | failed |
|:---|---:|---:|---:|---:|
| `pick cold` (`ls /usr/bin`, 924 lines) | 721 ms | 799 ms | 15 | 0 |
| `pick warm` (cache hit) | 7 ms | 12 ms | 15 | 0 |
| `is cold` (same 924 lines) | 434 ms | 512 ms | 15 | 0 |
| `why cold` (a 12-line failing `cargo build`) | 602 ms | 676 ms | 15 | 0 |
| `run cold route-only` (`--no-args`) | 1,879 ms | 2,003 ms | 15 | 0 |
| `run cold full` | 1,946 ms | 2,110 ms | 15 | 0 |
| `rg -c compress` on the same 924 lines | 2 ms | 3 ms | 15 | 0 |

`is` is one request, so its p50 is close to the network round trip. `run cold full` is what you feel when you type `, <something>`: p50 is about 1.9 s, above 1 s. Route-only saves about 70 ms at p50, so the time is in routing over the tool inventory, not in argument pointing. `rg` sits under hyperfine's 5 ms floor; the row shows what a local tool costs on the same input, not a race hunch is running.

Cost is computed from the request's input tokens at `HUNCH_PRICE_PER_MTOK` (default 0.042 $/Mtok) and reported in `meta.cost_usd`. Three calls from the session that produced the excerpts above: `why` on the 12-line build log, 2 requests, 1,436 tokens, $0.00006; `is` on a 6-line mail, 1 request, 346 tokens, $0.000015; `pick` over 5 file names, 1 request, 487 tokens, $0.00002.

Accuracy: pending. The eval set is committed (`evals/`: 49 routing requests, 120 held-out NL2Bash requests, 20 real failing CI logs with hand-labelled root-cause lines, and a 20-request argument-pointing spot check); the live run and its tables, with a BM25 baseline for routing and two regex baselines for `why`, are the next step. Until then no accuracy claim is made here.

## For agents

```sh
hunch capabilities --json      # commands, flags, exit codes, env, limits, safety rules
hunch robot-docs               # the agent handbook (docs/ROBOT_MODE.md)
```

Every command accepts `--json` (alias `--robot`) or `--format json|jsonl|toon` and then prints exactly one envelope on stdout, usage errors included:

```
{ ok, command, version, exit_code, data, meta{model, elapsed_ms, requests, cache_hits,
  input_tokens, cost_usd, threshold, request_id}, error{kind, message, hint, example} | null }
```

Branch on `exit_code`: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined. In machine mode `run` never executes unless `--exec --yes` is given, and the child's stdout goes to stderr so stdout stays one envelope. `data.blocked` names a tool hunch refuses to run; `data.argv` is still there for you to run under your own rules. Errors carry `error.example`, a corrected command to try next.

## Privacy

What leaves your machine, verb by verb: [PRIVACY.md](PRIVACY.md). Requests go only to the TypeSafe API. Before sending, hunch masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`, JWTs) as `[REDACTED]`. That is best effort, not a guarantee: do not pipe secrets into hunch.

## Safety

`run` executes only after a confirmation on the terminal or `--yes`; without a TTY and without `--yes` it prints the command and stops. Commands run via argv, never through a shell. Flags come from man pages; no binary is ever probed with `--help`.

Some tools are never executed, whatever the confidence or the flags: `rm`, `rmdir`, `dd`, `mkfs*`, `newfs*`, `fdisk`, `diskutil`, `shred`, `srm`, `wipefs`, `sudo`, `su`, `doas`, `kill`, `killall`, `pkill`, `reboot`, `halt`, `shutdown`, `poweroff`, `init`, `telinit`, `launchctl`, `systemctl`; the wrappers that would run another program named in their arguments (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `env`, `xargs`, `nohup`, `nice`, `timeout`, `time`, `exec`, `eval`, `command`, `find`, `watch`, `parallel`, `osascript`); and the interpreters that take program text as a flag value (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`, versioned names included). hunch shows the command and leaves it to you. The list is by tool name only: `chmod -R` is not on it.

## Limits

- Hosted API. No key, no network, no hunch. Rate limits are TypeSafe's: 1,200 requests per minute and 250k tokens per second; hunch retries with the server's `retry-after` and exits 4 when they run out.
- English works best. Ask literal questions: "the line with the failing test", not "what should I do".
- `pick` caps stdin at 20,000 lines; filter first (`rg`, `head`) or split the list.
- The byte budget assumes ~4 characters per token. Dense logs (hashes, paths, JSON) and CJK text tokenize denser and can be refused by the API: exit 6, `api_rejected_request`. Filter the input first.
- Untrusted text. The model reads input as data but is not hardened against instructions embedded in it, so `is` and `pick` are not security gates for text you do not control.
- `-n` past 3 carries no ranking claim. A "which one" answer is reliable at the top only; entries past the third are candidates.
- Identical requests replay the cached answer for 7 days. Without the cache, `p` moves by up to 0.06 between runs (measured on `jev-1.13.0`), so a decision within 0.06 of the threshold can flip. `is --band` is the one dead band; the other verbs have none.
- hunch points at things. It does not judge quality, count, do arithmetic or dates, so an `is` condition of that kind is unreliable.

## Why not an LLM shell?

- No invented flags: every flag comes from the man page of a tool that is installed.
- It abstains with a number (exit 3, and `p` in `--json`) instead of guessing.
- Latency is measured per verb, p50 and p95, in the table above.
- Cost is measured per call, in `meta.cost_usd`.
- It runs only what exists on your PATH, via argv, and never the tools on the never-execute list.
- What an LLM does better: composing a long, exact command from scratch. hunch points at one tool and its flags; it does not write pipelines.

## What it is bad at

pending: the failure tables come from the eval run above.

## License

MIT.
