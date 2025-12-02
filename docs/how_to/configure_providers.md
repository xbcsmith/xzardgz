# Configure Providers

## Supported Providers

- Ollama (Local)
- GitHub Copilot

## Configuration

Create a `config.yaml` file in the root directory:

```yaml
provider:
  provider_type: "ollama"
  model: "qwen2.5-coder"
```

Or use environment variables:

```bash
export XZARDGZ_PROVIDER=ollama
```
