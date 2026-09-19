# hunch v0 — design

Date: 2026-09-18. Status: approved direction (Thomas), pre-implementation.

## One sentence

`hunch` is a fast Rust CLI that points at the right thing among real things — a line, a tool, a
hunk, a file — using TypeSafe's Jev model, with calibrated confidence and an honest "nothing fits".

## Why it exists

Unix gates test syntax (regex, globs, exact names). Jev returns calibrated probabilities over
options we enumerate, never generated text, in ~0.3–0.5 s for $0.042 per million input tokens.
That makes a pointer over meaning cheap enough for every pipe. See the vault note
"Calibrated semantic gate — Jev as a computational primitive".

Competition (2026-09-18): ~12 thin Jev CLIs exist (`jevi` predicate with exit codes, `jgrep`,
`model-clis/jev`, `jev-axi`); `nli` fills flags for a tool you already named. Nobody ships
cross-`$PATH` routing with abstention, root-cause pointing over logs, sharding past 255 options,
one-wave latency, caching, and published speed/accuracy numbers. That is the product.

## Verbs

v0 (launch):
- `hunch pick "<intent>"` — stdin lines → prints the line(s) that match the intent; exit 3 if none fits.
- `hunch why` — stdin (failing output) → prints the root-cause line with context and line number.
- `hunch run "<intent>"` — routes to an installed tool, points at flags from its real `--help`,
  fills flag values only by copying tokens from the request, shows the command, runs on confirm.
- `hunch is "<condition>"` — stdin → exit 0 yes / 1 no / 3 unsure; the predicate primitive.
- Agent surface: `hunch capabilities --json`, `hunch robot-docs [topic]`, `hunch health`,
  `hunch init zsh|bash` (shell hook: `,` alias for `hunch run`).

v2 (wave 2):
- `hunch add "<topic>"` — stage only the git hunks about a topic (index only; confirm).
- `hunch sort <dir> --into <root>` — propose moving each file into an existing folder; dry-run
  by default, `--apply` with an undo log, never overwrites, never deletes.

Out of scope for the Rust CLI: tab bankruptcy (browser extension), clipboard router (native app).

## Principles

1. Point, never generate. Every output token comes from the input, the machine, or a tool's help.
2. One wave. A command makes at most two sequential rounds of parallel Jev calls.
3. Calibrated policy. One threshold (default 0.5) decides act/abstain; exit 3 = abstain.
4. Unix first. Human mode prints raw results to stdout (pipe-friendly); diagnostics to stderr.
5. Agent first. `--json`/`--format json|jsonl|toon` returns one envelope; stable exit codes;
   errors carry `kind`, `hint`, and a corrected `example` command; `capabilities` self-describes.
6. Safe by default. `run` never executes without a TTY confirmation or `--yes`; robot mode never
   executes without `--exec --yes`; `add` touches the index only; `sort` is dry-run by default.
7. Fast. Local work < 20 ms (Rust, cached inventory); p50 < 0.5 s for `pick`/`is` on ≤ 200
   lines; `why` < 2 s on a 50k-line log; repeated questions ~5 ms from the disk cache.
8. Private by design. Only the intent, the relevant lines or one-line tool descriptions leave the
   machine; documented in PRIVACY.md.

## Jev constraints the design respects

- Choice ≤ 255 options → tournament: windows of ≤ 200 items + `NONE`, then finalists.
- 32k tokens for state + longest question; 64k per request → small windows, pre-filtering.
- 1,200 requests/min (20/s) → concurrency cap 16, retries honoring `retry-after`,
  `why` pre-filters logs so a 50k-line log needs ≤ 20 requests.
- Literal reading → atomic, explicit questions with `NONE` options and absolute fit Nouls.
- No arithmetic, dates, or counting in questions; code does those.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | success: yes / found / executed OK |
| 1 | `is`: no |
| 2 | usage error (bad flags, missing argument) |
| 3 | abstain: nothing fits / unsure |
| 4 | API unavailable (network, 5xx after retries, rate limit exhausted) |
| 5 | authentication (missing or rejected key) |
| 6 | input error (empty stdin, input too large, unreadable file) |
| 7 | executed command failed (`run --yes`; child exit code in envelope) |
| 130 | interrupted / declined at confirmation |

## Configuration (env, overridable by flags)

`TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` (path chosen by the user), `HUNCH_BASE_URL`
(default `https://api.typesafe.ai`), `HUNCH_MODEL` (default `jev-latest`; pin for stable
thresholds), `HUNCH_THRESHOLD` (0.5), `HUNCH_CONCURRENCY` (16), `HUNCH_CACHE_DIR`
(platform cache dir + `/hunch`), `HUNCH_NO_CACHE=1`, `HUNCH_PRICE_PER_MTOK` (0.042).

## Distribution

GitHub releases (macOS arm64/x86_64, Linux x86_64/aarch64, musl), Homebrew tap
`tpellet/tap/hunch`, `cargo install --git`. The crates.io name `hunch` is taken (media filename
parser); crates.io publication is deferred to a name decision.

## Evidence at launch

- `run`: 49 hand-written intents (39 routable, 10 unroutable) + NL2Bash utility-level sample,
  vs BM25 baseline; report top-1, top-5, abstention precision.
- `why`: labeled build logs (LogChunks, Brandt et al. 2020, if license permits; else a curated set
  of 30 real failing logs from public CI) — report root-cause hit@1 and hit@3.
- Speed: hyperfine p50/p95 per verb, cold vs warm cache.
- Prototype evidence (Fable, 2026-09-18): routing top-1 35/39 vs BM25 10/39; 9/10 abstentions;
  one threshold 0.5 separated routable (median fit 0.93) from unroutable (≤ 0.45).
