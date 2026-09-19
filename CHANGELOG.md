# Changelog

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
- `init zsh|bash`: the `,` alias for `hunch run`, plus an opt-in command-not-found hook (`HUNCH_CNF=1`).
- `--json` / `--robot` / `--format json|jsonl|toon`: exactly one envelope on stdout (`ok, command, version, exit_code, data, meta, error`), usage errors included.
- Exit codes: 0 ok, 1 no, 2 usage, 3 abstain, 4 unavailable, 5 auth, 6 input, 7 child failed, 130 declined.

Safety:

- `run` executes only after a TTY confirmation or `--yes`; in machine mode only with `--exec --yes`, the child's stdout redirected to stderr.
- Never-execute list, by tool name: `rm`, `dd`, `mkfs*`, `sudo`, `kill`, `shutdown` and the rest of the destructive set, the shell and process wrappers (`sh`, `bash`, `env`, `xargs`, `find`, `timeout`, ...) and the script interpreters (`python*`, `perl*`, `ruby*`, `node*`, `php*`, `lua*`). These are shown, never run.
- Commands run via argv, never a shell; flags come from man pages, no binary is probed with `--help`.
- Obvious secrets are masked before text leaves the machine (best effort; see PRIVACY.md).

Model and answers:

- Default model pinned to `jev-1.13.0`; the 0.5 threshold was calibrated on it. `--model jev-latest` / `HUNCH_MODEL` allowed and documented as moving.
- One threshold (`-t`, `HUNCH_THRESHOLD`, default 0.5) on absolute yes/no answers; "which one" answers must beat NONE.
- Tournament past 255 options (windows of 200 + NONE, 3 finalists per window, one finals round), 60,000-character window budget, at most 2 rounds per verb (3 for `run`).
- Disk cache of answers keyed by request hash, 7-day TTL; `--no-cache`, `HUNCH_NO_CACHE`.
- Retries on 408/429/5xx/timeouts up to 3 times, honouring `retry-after`; 413/422 reported as input errors (`api_rejected_request`, exit 6).

Benchmarks (`benchmarks/`): per-verb p50/p95 with conditions; the connection prewarm was measured (17 ms at p50 on `pick`) and removed.
