# Configuration

grevi has no config file. Every setting is a flag or an environment variable. `grevi capabilities --json` prints the list below as data and is the source of truth.

## Environment variables

| Variable | Default | Meaning |
|:---|:---|:---|
| `TYPESAFE_API_KEY` | | The API key. Never printed, never logged. Setting it selects the `typesafe` backend. |
| `TYPESAFE_API_KEY_FILE` | | Path to a file holding the key; read only when a request needs a key. Use it to keep the key out of your environment and shell history: `TYPESAFE_API_KEY_FILE=/path/to/key`. |
| `GREVI_BACKEND` | `typesafe` with a key, `classifier` without one | `typesafe` or `classifier`: which API answers. See [Backends](#backends). |
| `GREVI_BASE_URL` | the active backend's own URL | The API endpoint. grevi sends requests nowhere else. |
| `GREVI_MODEL` | `jev-1.13.0` on TypeSafe | TypeSafe model or alias; explicit overrides on classifier are usage errors because the service selects its model. `jev-latest` moves with TypeSafe releases. |
| `GREVI_THRESHOLD` | `0.5` | Decision threshold on backend yes/no scores; calibration is task- and backend-specific. |
| `GREVI_CONCURRENCY` | `8` on `typesafe`, `4` on `classifier` | Parallel requests within one round (the windows of a tournament). |
| `GREVI_CACHE_DIR` | platform cache dir, `grevi` sub-directory | Where answers, the tool inventory and `sort`'s undo logs live. |
| `GREVI_NO_CACHE` | | Set to `1` to disable the answer cache (entries expire after 7 days anyway). |
| `GREVI_PRICE_PER_MTOK` | `0.042` | Dollars per million input tokens, used for `meta.cost_usd`. Change it if your TypeSafe pricing differs. |
| `GREVI_INVENTORY_FILE` | | A JSON array of `{name, summary}` that replaces the PATH inventory for `run`. Used by the tests and the evals so every machine routes over the same tools. |
| `GREVI_CNF` | | Set to `1` to enable the command-not-found hook printed by `grevi init`. |

A flag beats its variable: `-t 0.7` wins over `GREVI_THRESHOLD=0.5`.

## Backends

grevi asks one of two APIs, and both run the same model, Jev.

| Backend | Selected when | Key | Cost |
|:---|:---|:---|:---|
| `classifier` | no key is set | none needed | free ([classifier.dev](https://classifier.dev) runs Jev and serves it free) |
| `typesafe` | `TYPESAFE_API_KEY` or `TYPESAFE_API_KEY_FILE` is set | yours | billed to your key |

`GREVI_BACKEND=typesafe|classifier` forces a backend. `typesafe` without a key is exit 5. `meta.backend` in the JSON output and `grevi health` both name the backend that answered, and `meta.model` names the build of Jev behind it.

The free service has tighter limits and maps TypeSafe Nouls to binary Choice questions. The same threshold is exposed, but its calibration and the resulting answers are not assumed equivalent across backends. Constructed fields and label counts are checked locally; unsupported requests fail without silently dropping candidates.

| | `typesafe` | `classifier` |
|:---|---:|---:|
| options per question | 255 | 100 |
| tournament window | 200 | 99 + NONE |
| input per request | 32,000 tokens | 32,000 UTF-16 code units |
| questions per request | no limit in practice | 20 (grevi splits bigger asks) |
| rate limit | 1,200 requests/min | 3,000 classifications/min, 20,000/day, per IP |
| `meta.input_tokens`, `meta.cost_usd` | reported tokens, estimated cost | `0`: token usage is unavailable and inference is free; zero is not a measured token count |

Classifier also limits each instruction to 4,000 UTF-16 code units, each label to 200, and each dimension name to 64. The compact JSON of all dimension definitions must fit 16,000 UTF-16 code units, including JSON escaping. An emoji outside the basic multilingual plane counts as two units. grevi checks the complete constructed request before sending it.

Recorded routing and root-cause comparisons are in [evals/](../../evals/). Equal aggregate scores do not establish interchangeable probabilities. `meta.model` names the model reported by the service, when available.

## Global flags

| Flag | Variable | Meaning |
|:---|:---|:---|
| `--json` (alias `--robot`) | | One JSON envelope on stdout, usage errors included |
| `--format human\|json\|jsonl\|toon` | | Output format; overrides `--json` |
| `-t, --threshold <0..1>` | `GREVI_THRESHOLD` | Decision threshold on backend yes/no scores |
| `--model <id>` | `GREVI_MODEL` | TypeSafe model or alias |
| `--no-cache` | `GREVI_NO_CACHE` | Skip the local answer cache |
| `-v, --verbose` | | Probabilities, request count, tokens, cost and timing on stderr |
| `-V, --version` | | Print the version |

Per-verb flags are in [Verbs](verbs.md).

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
| `pick` lines (`pick_lines`) | 20,000 |

Past `stdin_bytes` or `pick_lines` grevi exits 6 before sending anything. The token limits are the API's. A request that exceeds them after grevi's own budgeting comes back as `api_rejected_request`, exit 6.

## Where files live

- Answer cache, tool inventory and undo logs: `GREVI_CACHE_DIR`, by default the platform cache directory (`~/Library/Caches/grevi` on macOS, `$XDG_CACHE_HOME/grevi` or `~/.cache/grevi` on Linux). Safe to delete; grevi rebuilds it.
- `sort --apply` writes `sort-undo-<timestamp>.tsv` there and prints the path (`data.undo_log` in JSON).
- grevi writes nothing else: no config, no logs, no history.

## Shell integration

```sh
eval "$(grevi init zsh)"      # ~/.zshrc
eval "$(grevi init bash)"     # ~/.bashrc
```

The snippet defines `,` as an alias for `grevi run` (`noglob grevi run` in zsh). It also defines a command-not-found handler that passes unknown commands of three or more words to `grevi run`, but only when `GREVI_CNF=1` is exported and no handler exists already.
