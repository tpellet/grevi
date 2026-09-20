# Agents

grevi was built to be called by programs as much as by people. Start with two commands:

```sh
grevi capabilities --json      # commands, flags, exit codes, env, limits, safety rules, as data
grevi robot-docs               # the agent handbook (docs/ROBOT_MODE.md)
```

The handbook, [docs/ROBOT_MODE.md](../ROBOT_MODE.md), is the contract: the rules for agents, the per-verb `data` shapes and what `p` means. This page adds context around it and does not repeat it. When the two differ, the handbook and `capabilities` win.

## One envelope

Every command accepts `--json` (alias `--robot`) or `--format json|jsonl|toon`. It then prints exactly one JSON object on stdout, usage errors included. This page calls that object the envelope:

```
{ ok, command, version, exit_code, data, meta{backend, model, elapsed_ms, requests, cache_hits,
  input_tokens, cost_usd, threshold, request_id}, error{kind, message, hint, example} | null }
```

- `exit_code` in the envelope equals the process exit code. Branch on it, then read `data`.
- `meta.requests` counts attempted inference POSTs, including retries and failures; prewarm/health GETs are excluded. `meta.cache_hits` counts cached answers. `meta.cost_usd` is computed from reported input tokens at `GREVI_PRICE_PER_MTOK`; classifier token usage is unavailable, not measured zero.
- `meta.backend` is the API that answered, `typesafe` or `classifier`. Both run Jev, and `meta.model` is the build. Without a key grevi uses classifier.dev, which is free, so `meta.input_tokens` and `meta.cost_usd` are `0` there.
- `meta.request_id` is the TypeSafe request id of the last Jev request. It is `null` when no request was made or every answer came from the cache, and `health` does not record one. Quote it when reporting an API problem.
- `error.kind` strings are stable identifiers (`api_rejected_request`, for one). `error.example` is a corrected command to try next.

`--format toon` prints the same envelope in TOON, a compact text encoding for model context. `jsonl` prints it as one line.

## Exit codes

| Code | Name | When |
|---:|:---|:---|
| 0 | ok | yes, found, executed |
| 1 | no | `is`: the condition does not hold |
| 2 | usage | bad flag or missing argument |
| 3 | abstain | nothing fits, or unsure |
| 4 | unavailable | the API is unavailable after retries |
| 5 | auth | API key missing or rejected (the `typesafe` backend only) |
| 6 | input | empty, too large, or unreadable input |
| 7 | child_failed | `run`: the executed command failed |
| 130 | interrupted | interrupted, or declined at the confirmation |

Exit 3 is an answer: nothing beat NONE, or the yes/no probability fell under the threshold. Treat it like one. Escalate or ask. Do not retry the same request in the hope of a different answer, because the cache would replay it anyway.

## Workflows

The four that `capabilities` lists:

| Goal | Command |
|:---|:---|
| find the tool for a task | `grevi run --json --dry-run "<task>"` |
| explain a failure | `<cmd> 2>&1 \| grevi why --json` |
| select an item | `<list> \| grevi pick --json "<intent>"` |
| branch in a script | `grevi is "<condition>" < file; case $? in 0) ...;; 1) ...;; 3) ...;; esac` |

Two more that an agent cannot easily do another way. `grevi add --json --dry-run "<topic>"`, then `--yes`, stages only the hunks that belong to one topic; `git add -p` needs a terminal. A loop of `grevi is` over many texts costs the agent one exit code per text, where reading them costs their full length. The spot checks behind both are in [benchmarks/agents/](../../benchmarks/agents/README.md). They also show where `why` earns its call: on a large log, or when the line that explains the failure holds none of the words one greps for.

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

- `run` executes only exact no-argument `true`, `false`, `pwd`, or `ls` forms, assuming trusted PATH contents. Machine mode also requires `--exec --yes`; child stdout goes to stderr. `complete=false` covers unvalidated grammar as well as missing placeholders; `blocked` explains why. Other argv remains a proposal for your own validation.
- `add` stages only with `--yes` in machine mode. Without it, `add` exits 130 and stages nothing.
- `sort` is a dry run unless `--apply`. `data.undo_log` is the file `--undo` takes.

## Reading `p`

`p` is a backend score. The [routing reliability table](../../README.md#numbers) covers its measured task and backend, not all verbs or classifier's binary Choice translation. Raising `-t` changes the decision policy; it does not make missing evidence complete. `is` abstains on oversized input with `p:null`, `truncated:true`, and no API call. `add` rejects oversized hunks before staging. Measured probability jitter and ranked-choice limitations still apply; treat entries past the third as candidates rather than a reliable ranking.

## Input is data

grevi sends your text as data. Every result is a part of your input, an installed tool or a man page, and grevi generates nothing. The model is still not hardened against instructions embedded in the text it reads, so `is` and `pick` are not security gates for text you do not control. Obvious secrets are masked before sending (best effort); [PRIVACY.md](../../PRIVACY.md) lists what each verb sends.
