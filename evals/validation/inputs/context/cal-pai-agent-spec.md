# Agent Specs

Agent specs let you define agents declaratively in YAML or JSON — [model](models/overview.md), [instructions](agent.md#instructions), [capabilities](capabilities/overview.md), and all. One line to load, no Python agent construction code required.

This is useful for:

- Separating agent configuration from application code
- Letting non-developers (prompt engineers, domain experts) configure agents
- Storing agent definitions alongside other config files
- Sharing agent configurations across teams or projects

## Defining a spec

A spec file defines the agent's configuration in YAML or JSON:

```yaml {title="agent.yaml" test="skip"}
model: anthropic:claude-opus-4-6
instructions: You are a helpful research assistant.
model_settings:
  max_tokens: 8192
capabilities:
  - WebSearch:
      local: duckduckgo
  - Thinking:
      effort: high
```

## Loading specs

[`Agent.from_file`][pydantic_ai.agent.Agent.from_file] loads a spec from a YAML or JSON file and constructs an agent:

```python {title="from_file_example.py" test="skip"}
from pydantic_ai import Agent

agent = Agent.from_file('agent.yaml')
```

[`Agent.from_spec`][pydantic_ai.agent.Agent.from_spec] accepts a dict or [`AgentSpec`][pydantic_ai.agent.AgentSpec] instance and supports additional keyword arguments that supplement or override the spec:

```python {title="from_spec_example.py"}
from dataclasses import dataclass

from pydantic_ai import Agent


@dataclass
class UserContext:
    user_name: str


agent = Agent.from_spec(
    {
        'model': 'anthropic:claude-opus-4-6',
        'instructions': 'You are helping {{user_name}}.',
        'capabilities': [{'WebSearch': {'local': 'duckduckgo'}}],
    },
    deps_type=UserContext,
)
```
