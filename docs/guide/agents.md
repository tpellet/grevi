# Agents

Use jevify when a literal search cannot ask the question or the output is too long to read.
Cheap tools narrow the input first. `why` points to a cause, `pick` selects a record, `filter`
keeps a subset, and `is` decides whether the next step should act.

```sh
jevify capabilities --json
jevify init agents
jevify robot-docs
```

`capabilities` is the source of truth for commands, usage, exit codes, data and limits.
`init agents` derives its bounded instruction block from that table. The
[robot handbook](../ROBOT_MODE.md) includes the full telemetry and recovery contract.

## One envelope

`--json` (alias `--robot`) prints one envelope on stdout, usage errors included. `--format jsonl`
prints it on one line; `--format toon` encodes the same envelope as TOON.

```text
{ok, command, version, exit_code, data,
 meta{backend, model, elapsed_ms, requests, cache_hits, input_tokens, cost_usd,
      threshold, request_id, telemetry},
 error{kind, message, hint, example} | null}
```

Branch on `exit_code`, which equals the process status, then read `data`. Error kinds are stable
identifiers. `too_many` is exit 6: narrow records with `grep` or `head`. `error.example` gives
a corrected command. Human output is not a machine protocol.

`meta.model` is a string; several models are joined with `", "`. `meta.backend` names the API.
`meta.requests` counts inference POST attempts, including failures and retries, excluding health
and prewarm GETs. Missing token usage is `null`, not a measured zero. `meta.cost_usd` is zero at
the classifier backend's default zero service price; otherwise incomplete input usage makes the
estimate `null`. Costs are input-token estimates, not billing receipts.

`meta.request_id` identifies the last TypeSafe inference response when available. It is `null`
without a reported request ID, including all-cache answers. `health` does not record one.

## Exit codes

| Code | Meaning |
|---:|:---|
| 0 | yes, found, successful operation |
| 1 | `is`: one no; `filter`: kept none |
| 2 | bad flag, argument or configuration |
| 3 | nothing fits or unsure; `filter`: every record unsure |
| 4 | backend unavailable or quota exhausted |
| 5 | missing or rejected TypeSafe key |
| 6 | empty, too large or unreadable input |
| 7 | reserved |
| 130 | declined at `add` confirmation |

Write the condition so that yes means act. `&&` stops on every nonzero code; use explicit
branches when no, abstention and errors require different handling. Under `git bisect run`,
map an unsure exit 3 to 125. Do not silently retry abstention until it agrees.

## Data per verb

| Verb | Fields |
|:---|:---|
| `pick` | `matches[{line,text,ordinal,p,lossy?}]`, `any`, `source` |
| `why` | `causes[{line,text,p,context[]}]`, `any`, `considered`, `total`, `hint`, `saved_input`, `complete` |
| `filter` | `records[{text,ordinal,p,verdict,lossy?}]`, `kept`, `total`, `unsure`, `saved_input`, `complete`, `excerpts_withheld` |
| `is`, one statement | `p`, `verdict`, `truncated`, `reason` when oversized |
| `is`, several statements | `statements[{statement,verdict,p}]`, aggregate `verdict`, `truncated`, `reason` when oversized |
| `route` | `tool`, `summary`, `synopsis`, `fit`, `alternatives[]` |
| `add` | `hunks[{file,header,p,staged}]` |
| `sort` | `moves[{from,to,p}]`, `skipped[{file,reason}]`, `undo_log`, `applied` |
| `capabilities` | commands, flags, exit codes, environment, limits, backends and safety contract |
| `robot-docs` | `topic`, `text` |
| `health` | `backend`, `base_url`, `key`, `api`, `latency_ms`, `models` |
| `init` | `script` |

Ordinals are 1-based. Non-UTF-8 records carry replacement text with `lossy: true` and `ordinal`;
use human output when exact original bytes matter. `pick` and `filter` preserve those bytes.

## One process for many records

```sh
fd -0 -e txt | jevify filter -0 --files 'asks for a refund'
gh run view --log-failed | jevify why --json
git log --oneline | jevify pick --json 'the commit that renamed the project'
jevify route --json 'keep my mac awake for an hour'
```

`filter` sends up to 1,000 records per request on classifier.dev, each judged alone. On TypeSafe,
20 records share a request and each question names its record; independence is not claimed.
Both `pick` and `filter` cap distinct records at 20,000. `--files` reads paths from stdin and
withholds hidden and secret-looking excerpts; it does not promise whole-file review.

Only `why` and `filter` save raw input, including secrets. The saved path appears on stderr and
in `data.saved_input`; a failed or skipped save sets `complete=false`. The saved-input store is
independent of `--no-cache` and never pruned. [Privacy](../../PRIVACY.md) names its location.

## Permissions and evidence

Output verbs start no user command. `route` prints a tool and synopsis; the caller writes and
authorizes its own command. `add` stages only with `--yes` in machine mode; otherwise it exits
130. `sort` proposes moves unless `--apply` or `--undo` is supplied. Authorization belongs to
the caller, and a high score does not supply it.

Compare `why.considered` with `why.total`; a cause selected from partial evidence is not a
whole-log guarantee. `is` abstains on oversized context before calling the backend. `add`
rejects oversized hunks before staging. `p` requires task- and backend-specific calibration;
the [measurements](how-it-works.md#numbers) do not transfer to every verb.

A `rate_limit_day` HTTP 429 returns exit 4, `daily quota of the free backend reached`, without
retry. Other transient failures may be retried. Never use semantic judgments as security gates
for untrusted text. Outbound redaction is best effort; local saved inputs remain raw.
