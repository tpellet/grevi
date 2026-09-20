# grevi guide

grevi points at the right line, file or tool from your shell, by meaning, with a calibrated probability, and says "nothing fits" (exit 3) when nothing does. This guide is the long form of the [README](../../README.md); every fact here comes from the code, the `--help` text, `grevi capabilities --json` or the repo's own measurements.

| Page | What it covers |
|:---|:---|
| [Getting started](getting-started.md) | Install, get a key, run `grevi health`, first commands, the `,` alias |
| [Verbs](verbs.md) | `pick`, `why`, `is`, `run`, `add`, `sort` and the utility commands: flags, exit codes, `data` fields, examples |
| [Agents](agents.md) | The JSON envelope, exit codes, `capabilities`, `robot-docs`, machine-mode rules for `run` and `add` |
| [How it works](how-it-works.md) | Select-not-generate, NONE, the one threshold, the tournament, the `why` prefilter, the cache, the pinned model, retries |
| [Configuration](configuration.md) | Every environment variable and global flag, with defaults |
| [FAQ](faq.md) | Cost, privacy, why not an LLM, why exit 3, non-English input |

Reference documents elsewhere in the repo:

- [docs/ROBOT_MODE.md](../ROBOT_MODE.md): the agent handbook, also printed by `grevi robot-docs`.
- [PRIVACY.md](../../PRIVACY.md): what each verb sends to the API and what it never sends.
- [CHANGELOG.md](../../CHANGELOG.md): what shipped in each release.
- [benchmarks/README.md](../../benchmarks/README.md): latency per verb, with conditions, and the prewarm decision.
- [evals/why/README.md](../../evals/why/README.md): the 20 root-cause cases and their provenance.

Release state: 0.2.0 ships all six verbs — `pick`, `why`, `is`, `run`, `add` and `sort`.
