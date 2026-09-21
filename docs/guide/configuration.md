# Configuration

jevify has no config file. Every setting is a flag or an environment variable. `jevify capabilities --json` prints the list below as data and is the source of truth.

## Environment variables

| Variable | Default | Meaning |
|:---|:---|:---|
| `TYPESAFE_API_KEY` | | The API key. Never printed, never logged. Setting it selects the `typesafe` backend. |
| `TYPESAFE_API_KEY_FILE` | | Path to a file holding the key; read only when a request needs a key. Use it to keep the key out of your environment and shell history: `TYPESAFE_API_KEY_FILE=/path/to/key`. |
| `JEVIFY_BACKEND` | `typesafe` with a key, `classifier` without one | `typesafe` or `classifier`: which API answers. See [Backends](#backends). |
| `JEVIFY_BASE_URL` | the active backend's own URL | HTTPS at `api.typesafe.ai` for TypeSafe or `classifier.dev` for classifier, on port 443. Local test endpoints are also accepted; see below. |
| `JEVIFY_MODEL` | `jev-1.13.0` on TypeSafe | TypeSafe model or alias; explicit overrides on classifier are usage errors because the service selects its model. `jev-latest` moves with TypeSafe releases. |
| `JEVIFY_THRESHOLD` | `0.5` | Decision threshold on backend yes/no scores; calibration is task- and backend-specific. |
| `JEVIFY_CONCURRENCY` | `8` on `typesafe`, `4` on `classifier` | Parallel requests within one round (the windows of a tournament). |
| `JEVIFY_CACHE_DIR` | platform cache dir, `jevify` sub-directory | Where answers, the tool inventory, `sort`'s recovery journals and raw saved inputs in `outputs/` live. |
| `JEVIFY_NO_CACHE` | | Set to `1` to disable the answer cache (entries expire after 7 days anyway). |
| `JEVIFY_PRICE_PER_MTOK` | `0.042` | Dollars per million input tokens, used for `meta.cost_usd`. Change it if your TypeSafe pricing differs. |
| `JEVIFY_INVENTORY_FILE` | | A JSON array of `{name, summary}` that replaces the PATH inventory for `route`. Used by the tests and the evals so every machine routes over the same tools. |
| `JEVIFY_CNF` | | Set to `1` to enable the command-not-found hook printed by `jevify init`. |

A flag beats its variable: `-t 0.7` wins over `JEVIFY_THRESHOLD=0.5`.

`JEVIFY_BASE_URL` accepts only the active backend's host, compared case-insensitively,
with HTTPS and no port or explicit port 443. Trailing dots, other hosts, other ports,
userinfo (`user:password@host`) and malformed URLs are configuration errors before any
request. An empty or blank value uses the backend's default URL. For local testing,
`localhost` and `127.0.0.1` accept any scheme and port, without userinfo.
Inference, prewarm and health requests never follow redirects.

## Backends

jevify asks one of two APIs. TypeSafe accepts a model selection; classifier controls its answering model.

| Backend | Selected when | Key | Cost |
|:---|:---|:---|:---|
| `classifier` | no key is set | none needed | free ([classifier.dev](https://classifier.dev) runs Jev and serves it free) |
| `typesafe` | `TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` is set | yours | billed to your key |

`JEVIFY_BACKEND=typesafe|classifier` forces a backend. `typesafe` without a key is exit 5. `meta.backend` in the JSON output and `jevify health` both name the backend that answered, and `meta.model` names the build of Jev behind it.

The free service has tighter limits and maps TypeSafe Nouls to binary Choice questions. The same threshold is exposed, but its calibration and the resulting answers are not assumed equivalent across backends. Constructed fields and label counts are checked locally; unsupported requests fail without silently dropping candidates.

| | `typesafe` | `classifier` |
|:---|---:|---:|
| options per question | 255 | 100 |
| tournament window | 200 | 99 + NONE |
| input per request | 32,000 tokens | 32,000 UTF-16 code units |
| questions per request | bounded by request evidence | 20 dimensions (jevify splits bigger asks) |
| records per `filter` request | 20 sharing one state | up to 1,000, each judged alone |
| rate limit | 1,200 requests/min | 3,000 classifications/min, 20,000/day, per IP |
| `meta.input_tokens`, `meta.cost_usd` | complete reported input tokens or null, estimated input cost or null | tokens may be null; cost is 0 at the default zero service price |

Classifier also limits each instruction to 4,000 UTF-16 code units, each label to 200, and each dimension name to 64. The compact JSON of all dimension definitions must fit 16,000 UTF-16 code units, including JSON escaping. An emoji outside the basic multilingual plane counts as two units. jevify checks the complete constructed request before sending it.

Routing and root-cause comparisons are in [evals/](../../evals/). Equal aggregate scores do not establish interchangeable probabilities. `meta.model` is a string; several reported answering models are joined with `", "`.

A `rate_limit_day` HTTP 429 returns exit 4, `daily quota of the free backend reached`, without retry.
Filter batches honour numeric `Retry-After` through 60 seconds and refuse longer delays.

## Global flags

| Flag | Variable | Meaning |
|:---|:---|:---|
| `--json` (alias `--robot`) | | One JSON envelope on stdout, usage errors included |
| `--format human\|json\|jsonl\|toon` | | Output format; overrides `--json` |
| `-t, --threshold <0..1>` | `JEVIFY_THRESHOLD` | Decision threshold on backend yes/no scores |
| `--model <id>` | `JEVIFY_MODEL` | TypeSafe model or alias |
| `--no-cache` | `JEVIFY_NO_CACHE` | Skip the local answer cache |
| `--verbose` | | Probabilities, request count, tokens, cost and timing on stderr; no short flag |
| `-V, --version` | | Print the version |

`filter -v` means inversion. Per-verb flags are in [Verbs](verbs.md).

## Limits

From `capabilities.limits` (the `typesafe` figures; `capabilities.backends` lists both backends):

| Limit | Value |
|:---|---:|
| options per question (`choice_options`) | 255 |
| tournament window (`window`) | 200 |
| model state tokens (`state_tokens`) | 32,000 |
| request tokens (`request_tokens`) | 64,000 |
| requests per minute (TypeSafe) | 1,200 |
| tokens per second (TypeSafe) | 250,000 |
| classifications per minute (classifier.dev, free, per IP) | 3,000 |
| stdin bytes (`stdin_bytes`) | 67,108,864 (64 MiB) |
| distinct records in `pick` and `filter` | 20,000 |

Past the byte or distinct-record ceiling jevify exits 6 before inference; the record ceiling uses
`error.kind=too_many`. The token limits are the API's. A request that exceeds them after jevify's
own budgeting comes back as `api_rejected_request`, exit 6.

## Where files live

- Base directory: `JEVIFY_CACHE_DIR`, otherwise the platform cache directory (`~/Library/Caches/jevify` on macOS, `$XDG_CACHE_HOME/jevify` or `~/.cache/jevify` on Linux).
- Answers expire after seven days and use hashes of redacted requests; `--no-cache` disables this cache. Expiry does not reclaim old files.
- Only `why` and `filter` save raw input, secrets included, as `outputs/<blake3-16>.log` under the base directory, never pruned. `--no-save` disables saving independently of `--no-cache`; skipped or failed saves set `data.complete=false`.
- `sort --apply` writes a unique JSONL recovery journal and prints its path (`data.undo_log`). Preserve journals needed for undo. Tool inventory also lives under the base directory.

## Shell integration

```sh
eval "$(jevify init zsh)"      # ~/.zshrc
eval "$(jevify init bash)"     # ~/.bashrc
```

The snippet defines `,` as an alias for `jevify route` (`noglob jevify route` in zsh). It also defines a command-not-found handler that passes unknown commands of three or more words to `jevify route`, but only when `JEVIFY_CNF=1` is exported and no handler exists already. Routing prints a tool and starts no user command.
