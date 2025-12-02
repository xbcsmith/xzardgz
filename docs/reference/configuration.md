# Configuration Reference

## Configuration File

Create a `config.yaml` file in your project root:

```yaml
provider:
  provider_type: "ollama"  # or "copilot"
  model: "qwen2.5-coder"   # optional, provider-specific

agent:
  max_turns: 10
  timeout_seconds: 600

repository:
  ignore_patterns:
    - "target"
    - ".git"
    - "node_modules"

documentation:
  output_dir: "docs"
```

## Environment Variables

- `XZARDGZ_PROVIDER`: Override provider type
- `RUST_LOG`: Set logging level (trace, debug, info, warn, error)

## Defaults

If no configuration file is present, the following defaults are used:
- Provider: Ollama (local)
- Model: qwen2.5-coder
- Max turns: 10
- Timeout: 600 seconds
- Output directory: docs/
