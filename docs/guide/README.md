# grevi guide

grevi answers questions about text you already have, from the shell. You describe the line, the file, the tool or the change you want, and grevi finds it by meaning, even when your words do not appear in it. Each answer comes with a calibrated probability. When nothing fits, grevi says so and exits 3.

This guide is the long form of the [README](../../README.md). Every fact here comes from the code, the `--help` text, `grevi capabilities --json` or the repo's own measurements.

| Page | What it covers |
|:---|:---|
| [Getting started](getting-started.md) | Install, get a key, run `grevi health`, first commands, the `,` alias |
| [Verbs](verbs.md) | `pick`, `why`, `is`, `run`, `add`, `sort` and the utility commands: flags, exit codes, `data` fields, examples |
| [Agents](agents.md) | The JSON object every command prints (the envelope), exit codes, `capabilities`, `robot-docs`, machine-mode rules for `run` and `add` |
| [How it works](how-it-works.md) | Why grevi selects and never generates, the "nothing fits" option (NONE), the one threshold, the tournament, the `why` prefilter, the cache, the pinned model, retries |
| [Configuration](configuration.md) | Every environment variable and global flag, with defaults |
| [FAQ](faq.md) | Cost, privacy, why not an LLM, why exit 3, non-English input |

Reference documents elsewhere in the repo:

- [docs/ROBOT_MODE.md](../ROBOT_MODE.md): the agent handbook, also printed by `grevi robot-docs`.
- [PRIVACY.md](../../PRIVACY.md): what each verb sends to the API and what it never sends.
- [CHANGELOG.md](../../CHANGELOG.md): what shipped in each release.
- [benchmarks/README.md](../../benchmarks/README.md): latency per verb, with conditions, and the prewarm decision.
- [evals/why/README.md](../../evals/why/README.md): the 20 root-cause cases and their provenance.

Release state: 0.3.0 ships all six verbs, `pick`, `why`, `is`, `run`, `add` and `sort`, and works without a key.
