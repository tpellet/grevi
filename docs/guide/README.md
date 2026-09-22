# jevify guide

jevify gives command-line tools an understanding of meaning. On the input side, `fill` resolves
real arguments and runs the command; on the output side, verbs select records or judge facts.
It selects existing text even when your words do not appear in it. It abstains when
nothing fits. Scores are specific to the backend and task, not universally calibrated probabilities.

| Page | Contents |
|:---|:---|
| [Getting started](getting-started.md) | Install, keyless access, first commands, shell integration |
| [Verbs](verbs.md) | `fill`, its markers, `pick --from`, output verbs and utility commands |
| [Agents](agents.md) | One machine envelope, exit codes, capabilities and permissions |
| [How it works](how-it-works.md) | Selection, NONE, thresholds, batching, cache and measurements |
| [Configuration](configuration.md) | Environment variables, global flags and storage |
| [FAQ](faq.md) | Cost, privacy, abstention and model limits |

- [Robot mode](../ROBOT_MODE.md): the agent contract printed by `jevify robot-docs`.
- [Privacy](../../PRIVACY.md): evidence sent per verb and raw saved inputs.
- [Changelog](../../CHANGELOG.md): release history.
- [Benchmarks](../../benchmarks/README.md): measurement inputs and conditions.
- [Root-cause cases](../../evals/why/README.md): failing logs and labelled cause ranges.
