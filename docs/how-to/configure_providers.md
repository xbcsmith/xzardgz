# Configure Providers

## Problem

You need XZardgz to connect to a supported AI provider before running plugins.

## Supported Providers

- OpenAI
- Anthropic
- Ollama
- GitHub Copilot

OpenAI is the default first-release provider path. Ollama remains useful for
local model execution when the configured plugin requirements can be met.

## Configure OpenAI

1. Store your key in an environment variable:

```bash
export OPENAI_API_KEY="replace-with-your-key"
```

1. Reference the variable from `config.yaml`:

```yaml
provider:
  default: "openai"

openai:
  api_key_env: "OPENAI_API_KEY"
  model: "gpt-4.1-mini"
  endpoint: "https://api.openai.com/v1"
  allow_insecure_endpoint: false
```

1. Validate credentials:

```bash
xzardgz auth validate
```

## Configure Anthropic

```bash
export ANTHROPIC_API_KEY="replace-with-your-key"
```

```yaml
provider:
  default: "anthropic"

anthropic:
  api_key_env: "ANTHROPIC_API_KEY"
  model: "claude-3-5-sonnet-latest"
```

## Configure Ollama

```yaml
provider:
  default: "ollama"

ollama:
  host: "http://localhost:11434"
  model: "qwen2.5-coder"
  context_length: 32768
```

## Configure Copilot

```yaml
provider:
  default: "copilot"

copilot:
  model: "gpt-4o"
  auth_store: "keychain"
```

Use the `auth` command to manage provider credentials:

```bash
xzardgz auth login copilot
xzardgz auth status
```

## Model Selection

Model selection can choose a configured fallback when the preferred model does
not satisfy plugin requirements.

```yaml
model_selection:
  enabled: true
  auto_fallback: true
  require_tools: true
  require_structured_output: true
  min_context_tokens: 16000
  preferred_models:
    - "gpt-4.1-mini"
  fallback_models:
    - "gpt-4.1"
```

## Security Notes

- Do not place raw API keys in committed configuration files.
- Prefer `api_key_env` or a provider-specific credential store.
- Set `allow_insecure_endpoint` to `true` only for trusted local testing.
- Review transcripts and reports for accidental secret exposure before sharing
  them.
