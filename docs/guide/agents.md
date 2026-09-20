# Agents

grevi was built to be called by programs as much as by people. Start with two commands:

```sh
grevi capabilities --json      # commands, flags, exit codes, env, limits, safety rules, as data
grevi robot-docs               # the agent handbook (docs/ROBOT_MODE.md)
```

The handbook, [docs/ROBOT_MODE.md](../ROBOT_MODE.md), is the contract: the rules for agents, the per-verb `data` shapes and what `p` means. This page adds context around it and does not repeat it; when the two differ, the handbook and `capabilities` win.

## One envelope

Every command accepts `--json` (alias `--robot`) or `--format json|jsonl|toon` and then prints exactly one envelope on stdout, usage errors included:

```
{ ok, command, version, exit_code, data, meta{model, elapsed_ms, requests, cache_hits,
  input_tokens, cost_usd, threshold, request_id}, error{kind, message, hint, example} | null }
```

- `exit_code` in the envelope equals the process exit code. Branch on it, then read `data`.
- `meta.requests` and `meta.cache_hits` say how much work the call did; `meta.cost_usd` is computed from `meta.input_tokens` at `GREVI_PRICE_PER_MTOK`.
- `meta.request_id` is the TypeSafe request id of the last Jev request (`null` when none was made or every answer came from the cache; `health` does not record one). Quote it when reporting an API problem.
- `error.kind` strings are stable identifiers (`api_rejected_request`, for one). `error.example` is a corrected command to try next.

`--format toon` prints the same envelope in TOON, a compact text encoding for model context; `jsonl` prints it as one line.

## Exit codes

| Code | Name | When |
|---:|:---|:---|
| 0 | ok | yes, found, executed |
| 1 | no | `is`: the condition does not hold |
| 2 | usage | bad flag or missing argument |
| 3 | abstain | nothing fits, or unsure |
| 4 | unavailable | TypeSafe API unavailable after retries |
| 5 | auth | API key missing or rejected |
| 6 | input | empty, too large, or unreadable input |
| 7 | child_failed | `run`: the executed command failed |
| 130 | interrupted | interrupted, or declined at the confirmation |

Exit 3 is an answer, not an error: nothing beat NONE, or the yes/no probability fell under the threshold. Escalate or ask; do not retry the same request hoping for a different answer (the cache would replay it anyway).

## Workflows

The four that `capabilities` lists:

| Goal | Command |
|:---|:---|
| find the tool for a task | `grevi run --json --dry-run "<task>"` |
| explain a failure | `<cmd> 2>&1 \| grevi why --json` |
| select an item | `<list> \| grevi pick --json "<intent>"` |
| branch in a script | `grevi is "<condition>" < file; case $? in 0) ...;; 1) ...;; 3) ...;; esac` |

## What `data` holds

| Verb | `data` |
|:---|:---|
| `pick` | `matches[{line, text, p}]`, `any` |
| `why` | `causes[{line, text, p, context[]}]`, `any`, `considered`, `total`, `hint`, `child_exit` |
| `run` | `tool`, `fit`, `argv[]`, `flags[]`, `complete`, `blocked`, `executed`, `child_exit`, `alternatives[]` |
| `is` | `p`, `verdict`, `truncated` |
| `add` | `hunks[{file, header, p, staged}]` |
| `sort` | `moves[{from, to, p}]`, `skipped[{file, reason}]`, `undo_log`, `applied` |

`line` values are 1-based line numbers into the input as grevi read it.

## Machine mode is safe by default

- `run` never executes in machine mode unless `--exec --yes` is given. With `--exec --yes`, the child's stdout is redirected to stderr so stdout stays one envelope. `data.blocked` names a tool grevi refuses to run (the never-execute list in [Verbs](verbs.md#run)); `data.argv` is still there for you to run under your own rules. `complete=false` means a `<VALUE>` placeholder remains in `argv`.
- `add` stages only with `--yes` in machine mode; otherwise it exits 130 and stages nothing.
- `sort` is a dry run unless `--apply`; `data.undo_log` is the file `--undo` takes.

## Reading `p`

`p` is a calibrated probability: across many calls, answers reported at 0.8 were right about 80% of the time (the measured table is in [README, Numbers](../../README.md#numbers)). Raise `-t` for costly actions. Without the cache, the same request moves `p` by up to 0.06 between runs, so a value within 0.06 of the threshold can flip on a `--no-cache` re-run; `is` has `--band` for that, the other verbs do not. `-n N` on `pick` and `why` is ranked by a "which one" answer that is reliable at the top only: entries past the third are candidates, not a ranking.

## Input is data, not instructions

grevi sends your text as data, and results always point into your input, the installed tools or a man page: nothing is generated. The model is still not hardened against instructions embedded in the text it reads, so `is` and `pick` are not security gates for text you do not control. Obvious secrets are masked before sending (best effort); [PRIVACY.md](../../PRIVACY.md) lists what each verb sends.
