# Authentication Reference

## Overview

Authentication in XZardgz covers two distinct concerns:

1. **AI provider authentication**: credentials that allow the pipeline to call
   an AI provider API such as OpenAI or Anthropic.
2. **Kafka authentication**: credentials that allow the watcher to connect to a
   secured Kafka cluster.

XZardgz never stores secret values directly in configuration files. All secrets
are referenced indirectly through environment variable names. The `auth` command
group manages provider authentication state for the current user and host.

## Supported Providers and Authentication Mechanisms

### OpenAI

Authentication uses an API key issued at `platform.openai.com`.

The key is read from the environment variable named in `openai.api_key_env` in
the configuration file. The default variable name is `OPENAI_API_KEY`.

```yaml
openai:
  api_key_env: "OPENAI_API_KEY"
  model: "gpt-4.1-mini"
  endpoint: "https://api.openai.com/v1"
```

Use `xzardgz auth login openai` or `xzardgz auth set-key openai` to store the
key in the system credential store. The key is then automatically exported as
the named environment variable when the pipeline runs.

### Anthropic

Authentication uses an API key issued at `console.anthropic.com`.

The key is read from the environment variable named in `anthropic.api_key_env`.
The default variable name is `ANTHROPIC_API_KEY`.

```yaml
anthropic:
  api_key_env: "ANTHROPIC_API_KEY"
  model: "claude-3-5-sonnet-latest"
```

Use `xzardgz auth login anthropic` or `xzardgz auth set-key anthropic` to store
the key.

### GitHub Copilot

Authentication uses OAuth and reads the token from the operating system
keychain. No API key is issued manually; the login flow exchanges a device code
for an OAuth token.

```yaml
copilot:
  model: "gpt-4o"
  auth_store: "keychain"
```

Use `xzardgz auth login copilot` to start the OAuth device flow. The command
prints a URL and a verification code. Visit the URL, enter the code, and the
token is stored in the keychain automatically.

The `auth_store: "keychain"` setting is the only supported value in this
release.

### Ollama

Ollama runs locally and does not require authentication by default. No API key
or login step is needed.

```yaml
ollama:
  host: "http://localhost:11434"
  model: "qwen2.5-coder"
```

If your Ollama instance is behind an authenticating proxy, set the proxy
credentials through standard HTTP proxy environment variables. XZardgz does not
manage Ollama proxy credentials.

## Auth Command Reference

All auth commands are under the `xzardgz auth` subcommand group.

### `login`

Start an interactive login flow for a provider. For API-key providers this
prompts for the key value and stores it. For Copilot this starts the OAuth
device flow.

```bash
xzardgz auth login openai
xzardgz auth login anthropic
xzardgz auth login copilot
xzardgz auth login ollama
```

For `ollama`, login performs a connectivity check against the configured host
and confirms the requested model is available. No credential is stored.

### `logout`

Remove stored credentials for a provider from the credential store.

```bash
xzardgz auth logout openai
xzardgz auth logout anthropic
xzardgz auth logout copilot
```

Logout does not affect environment variables that have been set independently of
the credential store.

### `status`

Show the authentication status of all providers. Reports which providers have
stored credentials, which are reachable, and which are missing configuration.

```bash
xzardgz auth status
```

Example output:

```text
openai      configured  key present (OPENAI_API_KEY)
anthropic   configured  key present (ANTHROPIC_API_KEY)
copilot     configured  token present in keychain
ollama      configured  host reachable at http://localhost:11434
```

### `validate`

Perform a live connectivity check for each configured provider. Makes a minimal
API call to confirm the credentials are accepted.

```bash
xzardgz auth validate
```

To validate a specific provider:

```bash
xzardgz auth validate openai
xzardgz auth validate anthropic
```

Exits with a nonzero status if any configured provider fails validation.

### `set-key`

Store or update an API key for a provider without going through the full login
flow. The command prompts for the key value.

```bash
xzardgz auth set-key openai
xzardgz auth set-key anthropic
```

Use this to rotate a key without re-authenticating.

### `remove-key`

Remove a stored API key for a provider from the credential store. Equivalent to
`logout` for API-key providers.

```bash
xzardgz auth remove-key openai
xzardgz auth remove-key anthropic
```

## Environment Variables

The following environment variables control provider authentication. All
variable names are configurable in the configuration file; the names below are
the defaults.

| Variable            | Provider  | Description                                               |
| ------------------- | --------- | --------------------------------------------------------- |
| `OPENAI_API_KEY`    | OpenAI    | OpenAI API key. Referenced by `openai.api_key_env`.       |
| `ANTHROPIC_API_KEY` | Anthropic | Anthropic API key. Referenced by `anthropic.api_key_env`. |
| `XZARDGZ_PROVIDER`  | All       | Override the default provider at runtime.                 |
| `XZARDGZ_MODEL`     | All       | Override the default model at runtime.                    |

For Kafka authentication in watcher mode, the variable names are set in the
configuration file and have no fixed default names. See the Watcher
Authentication section below.

Variables set in the shell environment take precedence over values stored in the
credential store. To use the credential store exclusively, ensure the
corresponding environment variable is not exported.

## Security Best Practices

### Never Commit API Keys

API keys and tokens must not appear in configuration files, workflow plan files,
commit messages, log output, or report files. The `api_key_env` field stores a
variable name, not the key value. Review configuration files for literal key
strings before committing them to version control.

### Prefer `api_key_env`

Use the `api_key_env` configuration field to name the environment variable
rather than passing keys on the command line. Command-line arguments can appear
in shell history and process listings.

### Inject Secrets Through the Environment

In CI/CD and container environments, inject secrets through the environment
rather than mounting configuration files that contain them. Use the secret
management feature of your CI provider (for example, GitHub Actions encrypted
secrets or Kubernetes `Secret` objects) to populate the environment variables at
runtime.

### Review Transcripts Before Sharing

When `trace_transcript.enabled: true`, the transcript captures the full
turn-by-turn exchange with the AI provider. Enable
`trace_transcript.redact_secrets: true` to replace likely secret values with
placeholder text before the transcript is written. Never share transcript files
without reviewing their contents first.

### Rotate Keys Periodically

Rotate API keys using `xzardgz auth set-key` when keys have been exposed or as
part of a regular rotation policy. Run `xzardgz auth validate` after rotation to
confirm the new key is accepted.

## Watcher Authentication

### Kafka SASL Credentials

When the watcher connects to a Kafka cluster that requires SASL authentication,
credentials are supplied through environment variables. The configuration file
names the environment variables; it does not contain the credential values.

```yaml
kafka:
  security_protocol: "SASL_SSL"
  sasl_mechanism: "SCRAM-SHA-512"
  sasl_username_env: "KAFKA_USERNAME"
  sasl_password_env: "KAFKA_PASSWORD"
  ssl_ca_location: "/etc/ssl/certs/kafka-ca.pem"
```

With this configuration, the watcher reads the username from `KAFKA_USERNAME`
and the password from `KAFKA_PASSWORD` at startup. Set these variables in the
process environment before starting the watcher:

```bash
export KAFKA_USERNAME=my-service-account
export KAFKA_PASSWORD=my-password
xzardgz watch --config config.yaml
```

In Kubernetes, populate these from a `Secret`:

```yaml
env:
  - name: KAFKA_USERNAME
    valueFrom:
      secretKeyRef:
        name: kafka-credentials
        key: username
  - name: KAFKA_PASSWORD
    valueFrom:
      secretKeyRef:
        name: kafka-credentials
        key: password
```

### Supported Security Protocols

| Value            | Description                                                                 |
| ---------------- | --------------------------------------------------------------------------- |
| `PLAINTEXT`      | No encryption, no authentication. Local development only.                   |
| `SSL`            | TLS encryption, no SASL authentication. Certificate-based trust.            |
| `SASL_PLAINTEXT` | SASL authentication without TLS encryption. Not recommended for production. |
| `SASL_SSL`       | SASL authentication over TLS. Recommended for production deployments.       |

### Provider Credentials in Watcher Mode

The watcher requires valid AI provider credentials in addition to Kafka
credentials. Provider authentication is configured the same way as for direct
`run` invocations: set the `api_key_env` field in the provider section of the
configuration file and export the named environment variable.

## Troubleshooting

### Validate Authentication Before Running

Run `xzardgz auth validate` to perform a live connectivity check before starting
a workflow or deploying the watcher. This catches misconfigured or expired keys
before they cause a mid-run failure.

```bash
xzardgz auth validate
```

### Common Error Messages

| Error                           | Likely Cause                                                                   | Resolution                                                                             |
| ------------------------------- | ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| `API key not found`             | The environment variable named in `api_key_env` is not set.                    | Export the variable or run `xzardgz auth set-key`.                                     |
| `Authentication failed: 401`    | The API key is present but invalid or expired.                                 | Rotate the key with `xzardgz auth set-key` and run `auth validate`.                    |
| `API key not found` for Copilot | The OAuth token has expired or was not stored.                                 | Run `xzardgz auth login copilot` again.                                                |
| `Connection refused` for Ollama | The Ollama server is not running or is listening on a different port.          | Start Ollama or update `ollama.host` in the configuration file.                        |
| `Kafka authentication failed`   | SASL credentials are missing or incorrect.                                     | Verify the environment variables named in `sasl_username_env` and `sasl_password_env`. |
| `SSL handshake failed`          | The CA certificate at `ssl_ca_location` does not match the broker certificate. | Update `ssl_ca_location` to point to the correct CA bundle.                            |

### Checking Credential Store State

Run `xzardgz auth status` to see which providers have credentials in the
credential store and which are relying on environment variables. The output
shows the variable name being used for each provider.

### Environment Variable Precedence

Environment variables set in the shell take precedence over values stored in the
credential store. If a key was rotated via `xzardgz auth set-key` but the old
key is still exported in the shell, the shell value is used. Unset the variable
to use the credential store value:

```bash
unset OPENAI_API_KEY
xzardgz auth validate openai
```
