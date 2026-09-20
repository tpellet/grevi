# Changelog

## Unreleased

Changed:

- Help an agent can use without guessing. Bare `grevi` prints a ten-line quick-start card (still exit 2, on stderr) where it printed the full help. Each verb's `--help` has examples, its exit codes and its `--json` fields, and the free-text arguments say how to phrase them. `capabilities` gains `use_when`, `output`, `phrasing`, more `workflows`, and a `when` and an `example` per verb. The handbook (`grevi robot-docs`) opens with when to call grevi, how to phrase, and patterns.
- The Claude Code plugin lives in `plugins/grevi/`, so an install copies the skill and its manifest, not the repository. The skill file is now `plugins/grevi/skills/grevi/SKILL.md`.
- README and guide: plain descriptions of the six verbs, examples run on the keyless backend, animations of `run` and `sort`, and an agent section backed by `benchmarks/agents/`.

## 0.3.0 — 2026-09-19

Added:

- **No key needed.** With no TypeSafe key, grevi asks [classifier.dev](https://classifier.dev), which runs the same Jev model and serves it free, with no key and no account. Same verbs, same calibrated probabilities, same exit codes, same JSON envelope; `meta.backend` and `grevi health` name the backend that answered, and `capabilities.backends` lists both with their limits. `GREVI_BACKEND=typesafe|classifier` forces either. A key still gets you your own TypeSafe quota, and is what `typesafe` requires.
- Measured, not assumed: on the two accuracy evals the backends score the same. Routing, hand-written set: 34/39 top-1 on both. NL2Bash held-out: 34/120 on classifier.dev, 33/120 on TypeSafe. Root cause: 14/20 hit@1 and 15/20 hit@3 on both.
- On classifier.dev the free service's own limits apply: a question takes at most 100 options, so the tournament windows at 99 plus NONE; an input takes 32,000 characters; a request takes 20 questions, and grevi splits bigger asks. `meta.input_tokens` and `meta.cost_usd` are `0` there, because nothing is charged. Default concurrency is 4 rather than 8, the user agent is `grevi/<version>`, and `Retry-After` is honoured.

## 0.2.0 — 2026-09-19

Wave 2: two verbs that act on real things, each safe by default. macOS and Linux.

Renamed:

- The project is now `grevi`, not `hunch`: crate, library, binary, `GREVI_*` environment variables, cache directory, user agent, agent skill, and the GitHub repository (`github.com/tpellet/grevi`). The v0.1.0 release assets keep their `hunch-*` names; the next release publishes `grevi-*`.

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
- `init zsh|bash`: the `,` alias for `grevi run`, plus an opt-in command-not-found hook (`GREVI_CNF=1`).
- `--json` / `--robot` / `--format json|jsonl|toon`: exactly one envelope on stdout (`ok, command, version, exit_code, data, meta, error`), usage errors included.
- Exit codes: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined.

Safety:

- `run` executes only after a TTY confirmation or `--yes`; in machine mode only with `--exec --yes`, the child's stdout redirected to stderr.
- Never-execute list, by tool name: `rm`, `dd`, `mkfs*`, `sudo`, `kill`, `shutdown` and the rest of the destructive set, the shell and process wrappers (`sh`, `bash`, `env`, `xargs`, `find`, `timeout`, ...) and the script interpreters (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`). These are shown, never run.
- Commands run via argv, never a shell; flags come from man pages, no binary is probed with `--help`.
- Obvious secrets are masked before text leaves the machine (best effort; see PRIVACY.md).

Model and answers:

- Default model pinned to `jev-1.13.0`; the 0.5 threshold was calibrated on it. `--model jev-latest` / `GREVI_MODEL` allowed and documented as moving.
- One threshold (`-t`, `GREVI_THRESHOLD`, default 0.5) on absolute yes/no answers; "which one" answers must beat NONE.
- Tournament past 255 options (windows of 200 + NONE, 3 finalists per window, one finals round), 60,000-character window budget, at most 2 rounds per verb (3 for `run`).
- Disk cache of answers keyed by request hash, 7-day TTL; `--no-cache`, `GREVI_NO_CACHE`.
- Retries on 408/429/5xx/timeouts up to 3 times, honouring `retry-after`; 413/422 reported as input errors (`api_rejected_request`, exit 6).

Benchmarks (`benchmarks/`): per-verb p50/p95 with conditions; the connection prewarm was measured (17 ms at p50 on `pick`) and removed.
