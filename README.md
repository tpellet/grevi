# grevi

grevi is the semantic toolkit for your shell: small, composable commands that bring judgment to ordinary Unix pipelines. Pick a filename from a description, find the error that broke a build, check whether an email asks for a refund, or match a request to an installed tool. Powered by TypeSafe's Jev, grevi selects from your actual input, tools and man pages rather than generating free-form answers. Pipes and exit codes make it fit into the scripts you already write; structured JSON makes the same commands usable by agents. Uncertainty has its own exit code, so your workflow can handle "unsure" explicitly.

One binary for macOS and Linux, and no key to get started. It ships six verbs — `pick`, `why`, `is`, `run`, `add` and `sort` — plus a `,` shell alias for `run`. Every command takes `--json` and prints one envelope with the answer, a calibrated probability, the request count and its cost.

The evidence is in the repo. [benchmarks/](benchmarks/README.md) holds the latency runs behind the Numbers section, [evals/](evals/) the routing and root-cause sets behind the accuracy tables, and [PRIVACY.md](PRIVACY.md) says, verb by verb, what leaves your machine.

The user guide, from install to the agent envelope, is in [docs/guide/](docs/guide/README.md).

[![CI](https://github.com/tpellet/grevi/actions/workflows/ci.yml/badge.svg)](https://github.com/tpellet/grevi/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/grevi)](https://crates.io/crates/grevi)
[![Release](https://img.shields.io/github/v/release/tpellet/grevi)](https://github.com/tpellet/grevi/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`cargo build 2>&1 | grevi why` points at the line that broke the build: the line itself, with its number and a probability, not a paraphrase of it.

```sh
cargo build 2>&1 | grevi why
ls | grevi pick "last month's electricity bill"
grevi is "asks for a refund" < mail.txt && ./refund
```

![demo](demo.gif)

Excerpts from a terminal, captured 2026-09-19 on `jev-1.13.0`:

```
$ cargo build 2>&1 | grevi why
>     2 │ error[E0425]: cannot find value `conifg` in this scope
      3 │  --> src/main.rs:3:20
      4 │   |
      5 │ 3 |     println!("{}", conifg);

$ grevi run --dry-run "count the lines in notes.txt"
grevi: wc (0.95) — word, line, character, and byte count
  -l  The number of lines in each input file is written to the standard output.
wc -l notes.txt

$ grevi run --yes "remove the file notes.txt"
grevi: not offering to run this (rm is on grevi's never-execute list); check it and run it yourself:
rm notes.txt

$ grevi is "asks for a refund" < mail.txt; echo $?
0
```

`is` prints nothing: the answer is the exit code (0 yes, 1 no, 3 unsure), so it composes with `&&`, `||` and `case`.

## Install

With Rust 1.87 or newer:

```sh
cargo install grevi --locked
```

Or the shell installer, macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/tpellet/grevi/releases/latest/download/grevi-installer.sh | sh
```

Then try it. No key, no account, no signup:

```sh
grevi health
cargo build 2>&1 | grevi why
```

### No key needed

Out of the box grevi asks [classifier.dev](https://classifier.dev), which runs the same Jev model and serves it free, with no key and no account. Same verbs, same probabilities, same exit codes; `meta.backend` in the JSON envelope says `classifier`, and `grevi health` names it too.

Set a TypeSafe key and grevi uses your own quota instead — higher limits, and a `meta.cost_usd` that is not zero:

```sh
export TYPESAFE_API_KEY=...
# or: export TYPESAFE_API_KEY_FILE=/path/to/key
```

grevi is an independent open-source client of TypeSafe's hosted Jev API; a key is yours from https://console.typesafe.ai, and requests on it are billed to you. `GREVI_BACKEND=typesafe|classifier` forces either backend.

Two differences follow from the free service's own limits, not from the model: it takes 100 options per question (so grevi ranks in windows of 99 plus "nothing fits" instead of 200) and 32,000 characters of input per request. On the routing and root-cause evals the two backends score the same — see [evals/](evals/).

Each verb makes at least one API request; `-v` prints how many, and what they cost.

## Quick start

```sh
grevi health                           # ok: classifier reachable in 190 ms (key not needed)
export TYPESAFE_API_KEY=...            # optional; or TYPESAFE_API_KEY_FILE=/path/to/key
grevi health                           # ok: typesafe reachable in 286 ms (key present)
```

Every verb below also takes `--json` for one machine-readable envelope, `-t` to move the decision threshold, `-v` to print probabilities and timing, and `--no-cache` to skip the answer cache. The full flag list per verb is in [docs/guide/verbs.md](docs/guide/verbs.md).

## Verbs

### pick: the line that matches

Stdin lines in, the matching line out. Exit 3 when nothing fits.

```sh
kill $(ps -eo pid,comm,%cpu | grevi pick "eating my battery" | awk '{print $1}')
git switch $(git branch | grevi pick "payment timeout fix")
```

`-n 3` prints up to three lines, each ranked above "nothing fits"; `--index` prints line numbers instead of lines.

### why: the line that broke it

```sh
cargo build 2>&1 | grevi why
grevi why -- cargo build
```

Compilers write errors to stderr, so pipe `2>&1`, or let `grevi why -- <cmd>` run the command and capture both streams. `-C 5` widens the context, `-n 3` reports up to three causes. Stdin with no error-like line exits 3 with a hint about stderr.

### is: a predicate on text

```sh
grevi is "asks for a refund" < mail.txt && ./refund
grevi is "asks for a refund" < mail.txt; case $? in 0) ./refund;; 1) ./archive;; 3) ./ask;; esac
```

Yes at or above 0.65, no below 0.35, unsure in between (`--band` sets the width around the 0.5 threshold).

### run: the installed tool that does it

```sh
grevi run "burn a dvd from this iso"
grevi run --dry-run "count the lines in notes.txt"
```

`run` routes the request to a tool installed on your machine, points at flags from that tool's man page, prints the command and asks before running it. `--dry-run` only proposes; `--yes` skips the question. A comma alias makes it a shell verb:

```sh
eval "$(grevi init zsh)"      # or bash
, "what's using port 8080"
```

Quote requests that contain an apostrophe: an unquoted `, what's using port 8080` opens a quote in both zsh and bash.

### add: the hunks about a topic

New in 0.2.0.

```sh
grevi add --dry-run "the auth fix"
grevi add --yes "the auth fix" && git commit
```

`add` scores each unstaged hunk against your topic and stages the ones that are about it. `--dry-run` only scores; `--yes` skips the question. Tracked files only; stages into the index, never commits; binary changes are never staged.

### sort: a folder for each file

New in 0.2.0.

```sh
grevi sort ~/Downloads                      # dry run: proposes a folder per file
grevi sort ~/Downloads --apply              # moves, writes an undo log
grevi sort ~/Downloads --undo <log>         # moves them back
```

`sort` looks at each file directly in the directory (not recursive, hidden files skipped) and proposes one of the existing folders under it, up to two levels deep, as its home; `--into <root>` picks the folders from another root. Dry-run by default: `--apply` renames the files and writes an undo log (`sort-undo-<timestamp>.tsv` in the cache directory) after every move; `--undo <log>` restores whatever the original path is still free for. It never overwrites a file, never deletes one, and only moves within one volume (`--into` must be on the same volume). Only file names and the first 2,000 characters of text files (or of a PDF's first two pages, when `pdftotext` is installed) are sent, redacted; a file the model cannot place, or places without confidence, stays where it is.

## How it works

grevi points, it does not generate. Every token it prints comes from your stdin, a tool on your PATH, that tool's man page, or your own request. A flag that is not in the man page cannot appear.

Jev, TypeSafe's model, answers two kinds of question. "Which one?" is a choice over the options plus NONE, and a candidate only has to beat NONE. "Is it?" is an absolute yes/no with a calibrated probability, gated by one threshold, 0.5 by default (`-t`). TypeSafe documents that the two kinds are not on one scale, so the threshold never touches a "which one" answer. Exit 3 means grevi abstained: nothing beat NONE, or the yes/no answer fell under the threshold.

The API takes at most 255 options per question. Past that, `pick` and `why` run a tournament: windows of 200 lines plus NONE, 3 finalists per window, then one finals round. A window's items share a 60,000-character budget (each clipped to 200–2,000 characters), which keeps a request under the model's 32k-token state limit for typical text. Every verb finishes in at most 2 rounds of parallel requests; `run` takes 3 (route, fit, arguments).

Answers are cached on disk for 7 days, keyed by a hash of the request; `--no-cache` bypasses the cache. The model is pinned to `jev-1.13.0`, the release the 0.5 threshold was calibrated on. `jev-latest` moves with each TypeSafe release, so the same input would start answering differently without any change here; `--model jev-latest` is allowed and documented as moving.

The longer version, with the `why` prefilter and the retry rules, is in [docs/guide/how-it-works.md](docs/guide/how-it-works.md).

### Why not an LLM shell?

- No invented flags: every flag comes from the man page of a tool that is installed.
- It abstains with a number (exit 3, and `p` in `--json`) instead of guessing.
- Latency is measured per verb, p50 and p95, in the table below.
- Cost is measured per call, in `meta.cost_usd`.
- It runs only what exists on your PATH, via argv, and never the tools on the never-execute list.
- What an LLM does better: composing a long, exact command from scratch. grevi points at one tool and its flags; it does not write pipelines.

## Numbers

Latency, end to end, per verb. Measured 2026-09-19 on a typical macOS dev machine (Apple M4 Pro, 24 GB, macOS 26.6.2) over consumer Wi-Fi (an empty HTTPS round trip to `api.typesafe.ai` took ~240 ms), model `jev-1.13.0`, `hyperfine --warmup 1 --runs 15`; "cold" is `GREVI_NO_CACHE=1`, "warm" is a cache hit. Inputs, script and the full table: [benchmarks/](benchmarks/README.md).

| Run | p50 | p95 | n | failed |
|:---|---:|---:|---:|---:|
| `pick cold` (`ls /usr/bin`, 924 lines) | 739 ms | 804 ms | 15 | 0 |
| `pick warm` (cache hit) | 6 ms | 10 ms | 15 | 0 |
| `is cold` (same 924 lines) | 454 ms | 488 ms | 15 | 0 |
| `why cold` (a 12-line failing `cargo build`) | 638 ms | 732 ms | 15 | 0 |
| `run cold route-only` (`--no-args`) | 1,823 ms | 2,007 ms | 15 | 0 |
| `run cold full` | 1,940 ms | 2,251 ms | 15 | 0 |
| `rg -c compress` on the same 924 lines | 3 ms | 4 ms | 15 | 0 |

`is` is one request, so its p50 is close to the network round trip. `run cold full` is what you feel when you type `, <something>`: p50 is about 1.9 s, above 1 s. Route-only saves about 100 ms at p50, so the time is in routing over the tool inventory, not in argument pointing. `run` opens its connection to the API while it reads the tool inventory; that overlap is worth about 200 ms at p50 on this verb (measured in [benchmarks/](benchmarks/README.md)). `rg` sits under hyperfine's 5 ms floor; the row shows what a local tool costs on the same input, not a race grevi is running.

Cost is computed from the request's input tokens at `GREVI_PRICE_PER_MTOK` (default 0.042 $/Mtok) and reported in `meta.cost_usd`. Three calls from the session that produced the excerpts above: `why` on the 12-line build log, 2 requests, 1,436 tokens, $0.00006; `is` on a 6-line mail, 1 request, 346 tokens, $0.000015; `pick` over 5 file names, 1 request, 487 tokens, $0.00002.

Accuracy, measured 2026-09-19 on `jev-1.13.0` with the release build, an empty cache and the default threshold 0.5: `scripts/eval_run.py` and `scripts/eval_why.py` over the data in [evals/](evals/). The two scripts cost $0.43 in API requests together (`run`: $0.42, `why`: $0.01); errors 0 on every set. Every `run` request routes over the frozen inventory `evals/inventory.json` instead of the machine's PATH, so grevi and BM25 rank the same 1,693 tools. Of those, 1,261 have a man page; 432 undocumented names were kept because they live in a system-wide prefix, and 179 were dropped at freeze time because they came from personal directories.

### run: routing

`--no-args`; top-1 is the routed tool. BM25 ranks the same names and man-page summaries with the request as the query: a free baseline, not a rival.

| Set | n | grevi top-1 | BM25 top-1 | abstained | errors |
|:---|---:|---:|---:|---:|---:|
| hand-written, routable (by the author) | 39 | 36 | 9 | 0 | 0 |
| NL2Bash held-out (not by the author) | 120 | 36 | 4 | 66 | 0 |

The 10 hand-written requests that no installed tool answers ("order a pizza", "mine bitcoin"): 9 abstentions out of 10, errors 0. The tenth, "translate this english text to french", went to `spit` at fit 0.90; its man page reads "translate some text through a Large Language Model", the label says abstain, so it counts as a miss.

NL2Bash requests describe one-line pipelines, mostly around `find`: 54 of the 66 abstentions and 12 of the 18 wrong routes have `find` as the gold utility. 40 of the 120 requests name the utility in their text; 16 of the 36 hits are among those 40, the other 20 hits are not. The prototype scored 35 of 120 on the same requests, with 73 abstentions.

Reliability of the routes grevi acted on, both sets: 93 routes with exit 0. Abstentions and error rows are left out, so the table says nothing about fits below the threshold. The bins are the ones `eval_run.py` prints; the lowest holds fits from 0.5 up.

| fit | routes | correct | accuracy |
|:---|---:|---:|---:|
| 0.4–0.6 | 17 | 12 | 0.71 |
| 0.6–0.8 | 34 | 23 | 0.68 |
| 0.8–1.0 | 42 | 37 | 0.88 |

`fit` is an absolute yes/no answer, "is this command a correct, direct way to do the request", and the threshold gates those answers only. This table is the evidence for the 0.5 default. "Calibrated" means one thing here: a route reported at fit 0.8 or above was right 37 times in 42. The data does not single out 0.5. Accuracy is 0.71 just above the threshold and 0.68 in the next bin; a threshold of 0.6 would have refused 17 routes, 12 of them right, to avoid 5 wrong ones. Fits below 0.5 were not scored, so this run says nothing about lowering it. 20 of the 169 routing decisions are one `--no-cache` re-run from flipping: their fit sits within 0.06 of the threshold, the measured run-to-run jitter of identical requests.

### run: arguments

The 20-request spot check (`evals/run_args.json`), run without `--no-args`: tool right 12/20, all required flags present 5/20, any extra flag 0/20, errors 0. Under half, so read `data.argv` as a proposal and prefer `--no-args`. The failure is a flag left out (`tar -x` without `-f`, `head` without `-n`, `sort` without `-n`, `ls` and `grep` bare), never a flag the request did not ask for.

### why

20 real failing CI logs (`evals/why/`, 136 to 300 lines each, 4 per ecosystem, root-cause ranges labelled by hand), `grevi why -n 3`; abstained 3, errors 0. The baselines take the first and the last line matching the regex grevi's own prefilter uses (`error`, `failed`, `panic`, `not found`, ...).

| method | hit@1 | hit@3 |
|:---|---:|---:|
| grevi | 15/20 | 16/20 |
| first `SIGNAL` match | 4/20 | 10/20 |
| last `SIGNAL` match | 1/20 | 3/20 |

`data.any`, the absolute "does this log hold a failure" answer that the threshold gates, was 0.17 at the minimum and 0.77 at the median over the 20 cases; the 3 abstentions sat at 0.17, 0.43 and 0.46. Every case holds a failure, so all three are misses.

## Agents

```sh
grevi capabilities --json      # commands, flags, exit codes, env, limits, safety rules
grevi robot-docs               # the agent handbook (docs/ROBOT_MODE.md)
```

Every command accepts `--json` (alias `--robot`) or `--format json|jsonl|toon` and then prints exactly one envelope on stdout, usage errors included:

```
{ ok, command, version, exit_code, data, meta{model, elapsed_ms, requests, cache_hits,
  input_tokens, cost_usd, threshold, request_id}, error{kind, message, hint, example} | null }
```

Branch on `exit_code`: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined. In machine mode `run` never executes unless `--exec --yes` is given, and the child's stdout goes to stderr so stdout stays one envelope. `data.blocked` names a tool grevi refuses to run; `data.argv` is still there for you to run under your own rules. Errors carry `error.example`, a corrected command to try next.

The per-verb `data` fields and the workflows are in [docs/guide/agents.md](docs/guide/agents.md).

A short agent skill, [skills/grevi/SKILL.md](skills/grevi/SKILL.md), teaches Claude Code and Codex when and how to call grevi. In Claude Code:

```
/plugin marketplace add tpellet/grevi
/plugin install grevi@grevi
```

For Codex, copy or symlink `skills/grevi` into `~/.agents/skills/` (or `.agents/skills/` in a repository).

## Privacy and safety

What leaves your machine, verb by verb: [PRIVACY.md](PRIVACY.md). Requests go only to the active backend's API — classifier.dev without a key, TypeSafe with one. Before sending, grevi masks obvious secrets (`token=…`, `Bearer …`, `sk-…`, `ghp_…`, `AKIA…`, JWTs) as `[REDACTED]`. That is best effort, not a guarantee: do not pipe secrets into grevi.

`run` executes only after a confirmation on the terminal or `--yes`; without a TTY and without `--yes` it prints the command and stops. Commands run via argv, never through a shell. Flags come from man pages; no binary is ever probed with `--help`.

Some tools are never executed, whatever the confidence or the flags: `rm`, `rmdir`, `dd`, `mkfs*`, `newfs*`, `fdisk`, `diskutil`, `shred`, `srm`, `wipefs`, `sudo`, `su`, `doas`, `kill`, `killall`, `pkill`, `reboot`, `halt`, `shutdown`, `poweroff`, `init`, `telinit`, `launchctl`, `systemctl`; the wrappers that would run another program named in their arguments (`sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `env`, `xargs`, `nohup`, `nice`, `timeout`, `time`, `exec`, `eval`, `command`, `find`, `watch`, `parallel`, `osascript`); and the interpreters that take program text as a flag value (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`, versioned names included). grevi shows the command and leaves it to you. The list is by tool name only: `chmod -R` is not on it.

## Limits

- Hosted API. No network, no grevi (a key is optional). Rate limits are the backend's: TypeSafe's 1,200 requests per minute and 250k tokens per second, or classifier.dev's free 3,000 classifications per minute and 20,000 per day, per IP. grevi retries with the server's `retry-after` and exits 4 when they run out.
- On classifier.dev a question takes at most 100 options (grevi windows at 99 plus NONE) and 32,000 characters of input; a request carries at most 20 questions, so grevi splits bigger ones. Same model, same answers.
- English works best. Ask literal questions: "the line with the failing test", not "what should I do".
- `pick` caps stdin at 20,000 lines; filter first (`rg`, `head`) or split the list.
- The byte budget assumes ~4 characters per token. Dense logs (hashes, paths, JSON) and CJK text tokenize denser and can be refused by the API: exit 6, `api_rejected_request`. Filter the input first.
- Untrusted text. The model reads input as data but is not hardened against instructions embedded in it, so `is` and `pick` are not security gates for text you do not control.
- `-n` past 3 carries no ranking claim. A "which one" answer is reliable at the top only; entries past the third are candidates.
- Identical requests replay the cached answer for 7 days. Without the cache, `p` moves by up to 0.06 between runs (measured on `jev-1.13.0`), so a decision within 0.06 of the threshold can flip. `is --band` is the one dead band; the other verbs have none.
- grevi points at things. It does not judge quality, count, do arithmetic or dates, so an `is` condition of that kind is unreliable.

## What it is bad at

Every item below is a failure row of the eval run above (`evals/out/*.json` after a run).

- Picking the tool people use over the tool whose man page matches. The inventory is flat and the one-line summary decides: "download a file from a url" went to `lwp-download` ("Fetch large files from the web") instead of `curl`, in both sets that asked. Likewise `ahost` over `hostname`, `cjpeg` over `sips`, `sdiff` over `diff`, `nl` over `cat`, `ditto` over `cp`, and `fd` over `find` in 5 of the 18 wrong NL2Bash routes. Several of those commands work; a reader who expects `curl` gets a tool they have never heard of. "generate a random password" went to `slappasswd`, the OpenLDAP password hasher, at fit 0.63.
- Pipelines. A request that needs two tools ("find the .sql files and run a script on each") gets one tool or an abstention: 66 of the 120 NL2Bash requests abstained, and `find` was the gold utility for 54 of them.
- Argument pointing drops flags: all required flags present in 5 of 20, an extra flag in 0 of 20. `tar -x` without `-f foo.tar.gz` never names the archive. Use `--no-args` and write the flags yourself.
- `why` treats some failures as not failures: a Go `--- FAIL` block whose message is "still exists" (`any` 0.43), a `WARNING: DATA RACE` from `go test -race` (0.46) and a ruff `D200` docstring finding under pre-commit (0.17) all exited 3. In a Python traceback it pointed at a library frame 27 lines above the exception line. When a log quotes another failure, as vitest's snapshot diff of the runner's own output does, the quoted failure was pointed at first (a hit@3, not a hit@1).
- Anything within 0.06 of the threshold: 20 of the 169 routing decisions above sit there, and a `--no-cache` re-run can flip them.

## Docs

- [Getting started](docs/guide/getting-started.md): install, key, first commands, the `,` alias.
- [Verbs](docs/guide/verbs.md): every verb with its flags, exit codes, `data` fields and examples.
- [Agents](docs/guide/agents.md): the JSON envelope, exit codes, `capabilities`, `robot-docs`.
- [How it works](docs/guide/how-it-works.md): select-not-generate, NONE, the one threshold, the tournament, the cache, the pinned model.
- [Configuration](docs/guide/configuration.md): every environment variable and global flag.
- [FAQ](docs/guide/faq.md): cost, privacy, why not an LLM, why exit 3.
- Reference: [robot mode](docs/ROBOT_MODE.md), [what leaves your machine](PRIVACY.md), [changelog](CHANGELOG.md), [benchmarks](benchmarks/README.md), [`why` eval cases](evals/why/README.md).

## License

MIT.
