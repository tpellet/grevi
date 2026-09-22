# Changelog

## 0.8.1

Fixed:

- `fill --dry-run` ends its stderr with `would run …` instead of `exec …`; only a real run
  prints `exec`.
- `fill` and `pick --from branch` name a branch that exists only on one remote by its short name
  (`ticket/TPE-791`), which `git switch` accepts, instead of `origin/ticket/TPE-791`; the ref
  stays in the evidence.
- `why` shows each finalist next to the nearest failure statement and asks for the failing
  step's own line rather than the first loud one; `data.any` is taken over both rounds.
- A `file` or `dir` marker whose names round is undecided sends every name not ruled out to the
  finals (up to 24), not the top three, so a content phrase whose file ranks low on its name
  reaches the excerpt round instead of losing to a related sibling.

## 0.8.0

Added:

- One overall deadline per verb, `JEVIFY_DEADLINE` seconds (600 by default): a retry wait that
  would end past it is not started, a request still queued or in flight at the deadline is
  cancelled, and the verb ends exit 4 `api_unavailable` naming the deadline. No request is sent
  before a server's `Retry-After` ends.
- `meta.usage{attempted, succeeded, waited{count, total_ms}, cache_hits, tokens{input, output}}`
  under `--json`: a cache hit is a hit and not a request, and a token count left unknown by any
  attempt is `null`, never zero. `telemetry.retry_waits` counts the retry waits started.

Fixed:

- `man` (route's synopsis and descriptions) and `pdftotext` (sort's PDF excerpts) run under a
  5 s deadline with stdin at `/dev/null` and bounded output, through the same poll-and-kill
  runner as the man index; a hung converter leaves the verb to go on without its text.
  `pdftotext` reads the file by path, after the regular-file check, instead of from stdin.
- `filter` and `label` send at most 60 records per classifier.dev request, the largest keyless
  request the service accepts: it refuses 75 with HTTP 402 `request_spending_limit` before
  judging anything, so a keyless run over about 70 records ended exit 4 `api_protocol` with the
  raw body. A 402 is reported as exit 4 `api_unavailable` naming the service code, without
  retry. `capabilities.limits.records_per_request.classifier` is 60.
- A marker in the command position (`argv[0]`) is exit 2 with its own message: the command
  must be literal, and `jevify pick --from tool '<description>'` finds it first.
- `fill`'s abstention line names the rival that decided it, `none` included with its
  probability, and never prints an empty field. An empty listing says `no KIND to choose from`
  (reason `no_match`, exit 3, no request).

Changed:

- `dir` finalists carry the names of their first 24 children as round-two evidence; a
  withheld or symlinked directory carries none and counts as withheld.
- `why` and stdin `pick` print their line, candidate and window counts on stderr before the
  first request, like `filter` and `label`.
- `jevify init agents` pairs every situation with a complete command and lists the kinds; the
  skill and the agents guide do the same.
- The keyless quota is measured per verb against the service's own accounting: one
  classification is one record under one question, 3,000 a minute and 20,000 a day per IP.
  `is` costs one per statement, `filter` and `label` one per distinct record, `pick` and `why`
  two per window of 99 lines plus two for the final round, `route` two per window of 99 commands
  plus one per finalist. README, the configuration guide, the FAQ and
  `capabilities.backends` state the free calls a day per verb; `benchmarks/results.md` holds the
  dated measurement. Routing over a PATH of 1,883 commands costs about 52 classifications, not
  one per command.

## 0.7.0

Added:

- `label a,b,c` tags each stdin record with one of the caller's labels and prints
  `LABEL<TAB>RECORD` in input order, `?` for an unsure record. Labels are comma-separated, at
  least two, distinct, at most 99 keyless or 200 on TypeSafe. It shares the record limits and
  batching of `filter`, takes `-0`, `--para` and `--files`, and saves nothing. Capabilities,
  the agent block, the skill and the privacy table name it.
- Marker kinds `commit`, `file`, `dir` and `tool`, coded, and the shipped recipes `pr`, `issue`,
  `ci-run`, `stash`, `process`, `container` and `pod`, read from `src/kinds.jsonl`. `fill` and
  `pick --from` resolve every kind. A recipe is one JSON line (`kind`, `list`, `field` or `key`,
  `ordered`); user recipes come from `kinds.jsonl` under `JEVIFY_CONFIG_DIR` or the platform
  configuration directory, never from a repository, and cannot replace a shipped kind.
- Every lister runs under one 20 s deadline with prompts disabled; a missing or unauthenticated
  tool is `lister_failed` with its own text, and a bad `kinds.jsonl` is `recipe_invalid` with
  its line number. A literal prefix ending in `/` narrows `file` and `dir`. Above the limit an
  ordered kind keeps its newest part and reports `candidates N of M, newest first`; any other
  is `too_many`.
- `capabilities` lists every kind with its lister argv and origin (`coded`, `shipped`, `user`),
  the recipe fields and rules, the withheld path patterns and `JEVIFY_CONFIG_DIR`. The guide
  gains `docs/guide/kinds.md`.

Changed:

- File excerpts drop imports, blank lines and license headers, and keep doc comments. For a
  kind with tier-two evidence, a no-match over names alone gets a second round with excerpts.

Known limits:

- `file` resolves phrases that match a path, and abstains on phrases that describe only a
  file's content (measured 2026-09-22 on jev-1.13.0: "stages hunks" does not reach
  `src/cmd/add.rs`).

## 0.6.0

Added:

- `fill` resolves quoted argument markers and becomes the caller-written command. `branch`
  lists refs, `-` selects from supplied records, and `one` and `flag` judge caller-written
  options against context. `--dry-run` previews argv; machine output requires it.
- All-or-nothing resolution, separate exit-3 marker reasons, and a Jev-only execution guard.
  Missing answering-model names are `unknown` and refuse execution. The command owns its
  output, signals and exit code; consumed stdin becomes empty for it.
- `pick --from branch` returns handles without running a user command; plain `pick` reads stdin.
- Capabilities list marker kinds, exact lister argv, backend capacities, input error kinds
  and abstention reasons. Agent instructions teach quoting, recovery and command permissions.

Changed:

- Selection finalists follow rank within each window: three, two or one as capacity permits.
  `fill` keeps three, accepting 3,267 candidates keyless and 13,200 on TypeSafe; `pick` accepts
  9,801 keyless and 20,000 on TypeSafe. Ordered kinds report retained coverage.

## 0.5.0

Added:

- `filter` selects matching stdin records, preserving bytes and input order. `-v` inverts,
  `-c` counts, and unsure records remain unless `--strict`. Batches hold up to 1,000 records
  on classifier.dev or 20 on TypeSafe; the distinct-record ceiling is 20,000 (`too_many`).
- `is` accepts several statements and `--context FILE`. One statement prints nothing;
  several print verdict-tab-statement lines, with `data.statements` in machine output.
- `pick` and `filter` accept `-0`, `--para` and boolean `--files` reading paths from stdin.
  Hidden and secret-looking file excerpts are withheld. Non-UTF-8 machine records carry
  replacement text, `lossy: true` and `ordinal`.
- `why` and `filter` save full raw inputs under the cache directory's `outputs/`, with
  content-addressed names and private permissions. These files include secrets and are never
  pruned. `--no-save` is separate from `--no-cache`; skipped or failed saves mark incomplete data.
- `init agents` prints at most 25 lines derived from the capabilities command and exit tables.

Changed:

- `route` prints an installed tool, summary and synopsis without starting the user's command.
  Shell integration calls `route`. Capabilities, help and agent documents share this contract.
- `why` reads stdin only, keeps numbered context and accepts no record split option.
- `meta.model` remains a string, joining several answering models with comma and space.
- A `rate_limit_day` HTTP 429 returns exit 4 with no retry. Classifier record batches honour
  numeric `Retry-After` through 60 seconds and refuse longer waits.

Removed:

- `why -- CMD`, `run`, its `--yes`, `--exec`, `--dry-run` and `--no-args` flags, and its
  argument-selection pass. No compatibility aliases are provided.
- `pick --files DIR`; supply paths on stdin instead.
- Global `-v`; use `--verbose` for diagnostics. `filter -v` means inversion.
- The README's obsolete terminal recording reference; the recording files remain available.

Fixed:

- Bind `JEVIFY_BASE_URL` to the active backend's HTTPS host on port 443, with local test endpoints excepted. Reject userinfo and disable redirects for inference, prewarm and health requests so credentials and evidence cannot follow an override to another service.

## 0.4.0 — 2026-09-20

Added:

- `pick --files <DIR> "<intent>"`: choose among the files under a directory by what they are about, and print the path. The first round ranks the path names; the second reads the first 2,000 masked characters of at most 24 finalist files. Inside a git work tree `.gitignore` applies. Hidden entries, symlinks and names that are not UTF-8 are never candidates. `data.source` reports `stdin` or `files`.

Changed:

- The crate, the library, the binary, the `JEVIFY_*` environment variables, the cache directory, the user agent, the agent skill and the repository are named `jevify`. Install with `cargo install jevify --locked`.

## 0.3.4 — 2026-09-20

Changed:

- Releases select a committed version with successful branch CI, scan its changes, and push a signed tag. The shared working tree is never packaged for publication.
- CI verifies the crates.io package and generated cargo-dist workflow. GitHub artifact publication precedes crates.io Trusted Publishing from the same release commit.

## 0.3.3 — 2026-09-20

Fixed:

- Sort apply and undo use atomic no-replace operations. Unique, durable JSONL journals preserve absolute filename bytes and file identity, recover interrupted moves, and report partial failures. Symlink entries are excluded; old TSV journals are rejected. Concurrent source replacement remains unsupported.
- Malformed API decisions return protocol errors instead of panics or false abstentions. Decision-bearing fields are validated before caching and consumption; cache identity includes endpoint and decision-contract version.
- Classifier requests check constructed input, instruction, label and dimension limits locally. Oversized requests fail explicitly rather than silently losing evidence. Clipping includes its marker within the requested limit. Explicit model overrides on classifier are rejected.
- Outbound semantic state and question text share a redaction boundary. Non-secret token-related identifiers retain their meaning. Request telemetry counts attempted inference POSTs, including retries and failures.
- Command displays quote argv for POSIX shells. Closed stdout pipes exit normally instead of panicking.

Changed:

- `run` executes only exact no-argument `true`, `false`, `pwd`, and `ls` forms under the existing confirmation policy. Other grammar stays a proposal with `complete:false` and a blocked reason; trusted PATH is required.
- Oversized `is` input abstains without an API call, with `p:null` and an evidence reason. `add` rejects oversized hunks or batches before classification or staging.
- Documentation and capabilities scope calibration to backend and task. The governing plan separates these repairs from candidate-survival, calibration, recipe, recovery, deadline and record-mode work.

## 0.3.2 — 2026-09-20

Changed:

- Documentation only. The README opens with what jevify is ("`grep` for meaning"), a table of the six commands and a links row, credits Jev from TypeSafe AI and classifier.dev, and the README and guide describe the tool without meta or retrospective language. The package description and keywords match.

## 0.3.1 — 2026-09-20

Changed:

- Help an agent can use without guessing. Bare `jevify` prints a ten-line quick-start card (still exit 2, on stderr) where it printed the full help. Each verb's `--help` has examples, its exit codes and its `--json` fields, and the free-text arguments say how to phrase them. `capabilities` gains `use_when`, `output`, `phrasing`, more `workflows`, and a `when` and an `example` per verb. The handbook (`jevify robot-docs`) opens with when to call jevify, how to phrase, and patterns.
- The Claude Code plugin lives in `plugins/jevify/`, so an install copies the skill and its manifest, not the repository. The skill file is now `plugins/jevify/skills/jevify/SKILL.md`.
- README and guide: plain descriptions of the six verbs, examples run on the keyless backend, animations of `run` and `sort`, and an agent section backed by `benchmarks/agents/`.

## 0.3.0 — 2026-09-19

Added:

- **No key needed.** With no TypeSafe key, jevify asks [classifier.dev](https://classifier.dev), which runs the same Jev model and serves it free, with no key and no account. Same verbs, same calibrated probabilities, same exit codes, same JSON envelope; `meta.backend` and `jevify health` name the backend that answered, and `capabilities.backends` lists both with their limits. `JEVIFY_BACKEND=typesafe|classifier` forces either. A key still gets you your own TypeSafe quota, and is what `typesafe` requires.
- Measured, not assumed: on the two accuracy evals the backends score the same. Routing, hand-written set: 34/39 top-1 on both. NL2Bash held-out: 34/120 on classifier.dev, 33/120 on TypeSafe. Root cause: 14/20 hit@1 and 15/20 hit@3 on both.
- On classifier.dev the free service's own limits apply: a question takes at most 100 options, so the tournament windows at 99 plus NONE; an input takes 32,000 characters; a request takes 20 questions, and jevify splits bigger asks. `meta.input_tokens` and `meta.cost_usd` are `0` there, because nothing is charged. Default concurrency is 4 rather than 8, the user agent is `jevify/<version>`, and `Retry-After` is honoured.

## 0.2.0 — 2026-09-19

Wave 2: two verbs that act on real things, each safe by default. macOS and Linux.

Verbs:

- `add "<topic>"`: score each unstaged hunk of tracked files against a topic and stage the ones about it; `--dry-run` only scores, `--yes` skips the question; works from any subdirectory of the repo; exit 3 when no hunk is about the topic, exit 6 when there are no unstaged changes.
- `sort <dir>`: propose a home among the existing folders under `dir` (or `--into <root>`), up to two levels deep, for each file directly in `dir`; `--apply` moves the files and writes an undo log, `--undo <log>` moves them back; exit 3 when nothing can be placed, exit 6 when there are no folders to sort into.

Safety:

- `add` touches the index only: it never commits, never stages untracked files or binary changes, and in machine mode stages only with `--yes` (otherwise exit 130 and nothing is staged).
- `sort` is a dry run unless `--apply`: it never overwrites a file, never deletes one, moves within one volume only, and `--undo` restores every file whose original path is still free.

Agent surface: `capabilities`, `robot-docs`, README and PRIVACY.md list both verbs; PRIVACY.md says what each sends (`add`: the topic and each unstaged hunk, clipped; `sort`: file names, the first 2,000 characters of each text file or of a PDF's first two pages, and the folder names under the root).

Speed: `run` opens its API connection while it reads the tool inventory, about 200 ms at p50 on `run cold full` (ABBA A/B in `benchmarks/README.md`); the other verbs have no local work to overlap and stay without prewarm.

## 0.1.0

First release. macOS and Linux.

Verbs:

- `pick "<intent>"`: print the stdin line(s) that match an intent; `-n N`, `--index`; exit 3 when nothing fits.
- `why`: point at the root-cause line in failing output, from stdin or by running the command (`why -- <cmd>`); `-C N` context, `-n N` causes.
- `run "<intent>"`: route to an installed tool, point at flags from its man page, confirm, run; `--dry-run`, `--yes`, `--no-args`, `--exec` (machine mode).
- `is "<condition>"`: exit 0 yes, 1 no, 3 unsure; `--band` (default 0.15).

Agent surface:

- `capabilities` (`--json`): commands, flags, exit codes, env, limits, safety rules.
- `robot-docs [guide|commands|exit-codes|examples|privacy]`: the agent handbook.
- `health`: key and API reachability.
- `init zsh|bash`: the `,` alias for `jevify run`, plus an opt-in command-not-found hook (`JEVIFY_CNF=1`).
- `--json` / `--robot` / `--format json|jsonl|toon`: exactly one envelope on stdout (`ok, command, version, exit_code, data, meta, error`), usage errors included.
- Exit codes: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined.

Safety:

- `run` executes only after a TTY confirmation or `--yes`; in machine mode only with `--exec --yes`, the child's stdout redirected to stderr.
- Never-execute list, by tool name: `rm`, `dd`, `mkfs*`, `sudo`, `kill`, `shutdown` and the rest of the destructive set, the shell and process wrappers (`sh`, `bash`, `env`, `xargs`, `find`, `timeout`, ...) and the script interpreters (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`). These are shown, never run.
- Commands run via argv, never a shell; flags come from man pages, no binary is probed with `--help`.
- Obvious secrets are masked before text leaves the machine (best effort; see PRIVACY.md).

Model and answers:

- Default model pinned to `jev-1.13.0`; the 0.5 threshold was calibrated on it. `--model jev-latest` / `JEVIFY_MODEL` allowed and documented as moving.
- One threshold (`-t`, `JEVIFY_THRESHOLD`, default 0.5) on absolute yes/no answers; "which one" answers must beat NONE.
- Tournament past 255 options (windows of 200 + NONE, 3 finalists per window, one finals round), 60,000-character window budget, at most 2 rounds per verb (3 for `run`).
- Disk cache of answers keyed by request hash, 7-day TTL; `--no-cache`, `JEVIFY_NO_CACHE`.
- Retries on 408/429/5xx/timeouts up to 3 times, honouring `retry-after`; 413/422 reported as input errors (`api_rejected_request`, exit 6).

Benchmarks (`benchmarks/`): per-verb p50/p95 with conditions; the connection prewarm was measured (17 ms at p50 on `pick`) and removed.
