# Configuration

hunch has no config file. Everything is a flag or an environment variable; `hunch capabilities --json` prints the list below as data and is the source of truth.

## Environment variables

| Variable | Default | Meaning |
|:---|:---|:---|
| `TYPESAFE_API_KEY` | | The API key. Never printed, never logged. |
| `TYPESAFE_API_KEY_FILE` | | Path to a file holding the key; read only when a request needs a key. Use it to keep the key out of your environment and shell history: `TYPESAFE_API_KEY_FILE=/path/to/key`. |
| `HUNCH_BASE_URL` | `https://api.typesafe.ai` | The API endpoint. hunch sends requests nowhere else. |
| `HUNCH_MODEL` | `jev-1.13.0` | Model or alias. The default is pinned; `jev-latest` moves with each TypeSafe release and may shift probabilities against the threshold. |
| `HUNCH_THRESHOLD` | `0.5` | Decision threshold on absolute yes/no answers. |
| `HUNCH_CONCURRENCY` | `8` | Parallel requests within one round (the windows of a tournament). |
| `HUNCH_CACHE_DIR` | platform cache dir, `hunch` sub-directory | Where answers, the tool inventory and `sort`'s undo logs live. |
| `HUNCH_NO_CACHE` | | Set to `1` to disable the answer cache (entries expire after 7 days anyway). |
| `HUNCH_PRICE_PER_MTOK` | `0.042` | Dollars per million input tokens, used for `meta.cost_usd`. Change it if your TypeSafe pricing differs. |
| `HUNCH_INVENTORY_FILE` | | A JSON array of `{name, summary}` that replaces the PATH inventory for `run`. Used by the tests and the evals so every machine routes over the same tools. |
| `HUNCH_CNF` | | Set to `1` to enable the command-not-found hook printed by `hunch init`. |

A flag beats its variable: `-t 0.7` wins over `HUNCH_THRESHOLD=0.5`.

## Global flags

| Flag | Variable | Meaning |
|:---|:---|:---|
| `--json` (alias `--robot`) | | One JSON envelope on stdout, usage errors included |
| `--format human\|json\|jsonl\|toon` | | Output format; overrides `--json` |
| `-t, --threshold <0..1>` | `HUNCH_THRESHOLD` | Decision threshold on calibrated probability |
| `--model <id>` | `HUNCH_MODEL` | TypeSafe model or alias |
| `--no-cache` | `HUNCH_NO_CACHE` | Skip the local answer cache |
| `-v, --verbose` | | Probabilities, request count, tokens, cost and timing on stderr |
| `-V, --version` | | Print the version |

Per-verb flags are in [Verbs](verbs.md).

## Limits

From `capabilities.limits`:

| Limit | Value |
|:---|---:|
| options per question (`choice_options`) | 255 |
| tournament window (`window`) | 200 |
| model state tokens (`state_tokens`) | 32,000 |
| request tokens (`request_tokens`) | 64,000 |
| requests per minute (TypeSafe) | 1,200 |
| tokens per second (TypeSafe) | 250,000 |
| stdin bytes (`stdin_bytes`) | 67,108,864 (64 MiB) |
| `pick` lines (`pick_lines`) | 20,000 |

Past `stdin_bytes` or `pick_lines` hunch exits 6 before sending anything. The token limits are the API's; a request that exceeds them after hunch's own budgeting comes back as `api_rejected_request`, exit 6.

## Where files live

- Answer cache, tool inventory and undo logs: `HUNCH_CACHE_DIR`, by default the platform cache directory (`~/Library/Caches/hunch` on macOS, `$XDG_CACHE_HOME/hunch` or `~/.cache/hunch` on Linux). Safe to delete; hunch rebuilds it.
- `sort --apply` writes `sort-undo-<timestamp>.tsv` there and prints the path (`data.undo_log` in JSON).
- hunch writes nothing else: no config, no logs, no history.

## Shell integration

```sh
eval "$(hunch init zsh)"      # ~/.zshrc
eval "$(hunch init bash)"     # ~/.bashrc
```

Defines `,` as an alias for `hunch run` (`noglob hunch run` in zsh) and, only when `HUNCH_CNF=1` is exported and no handler exists already, a command-not-found handler that routes unknown commands of three or more words to `hunch run`.
