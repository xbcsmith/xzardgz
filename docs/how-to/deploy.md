# Deploy XZardgz

## Problem

You want to deploy XZardgz for production use, either as a one-shot CI tool or
as a long-running watcher service.

## Binary Build

### Development Build

```bash
cargo build
```

The debug binary is written to `target/debug/xzardgz`.

### Production Release Build

```bash
cargo build --release
```

The optimized binary is written to `target/release/xzardgz`.

### Install to System Path

```bash
cargo install --path .
```

This installs `xzardgz` to `~/.cargo/bin/`, which should already be on your
`PATH` if you followed the standard Rust installation.

### Cross-Compilation

To produce a statically linked binary for Linux x86_64:

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

The binary is written to `target/x86_64-unknown-linux-musl/release/xzardgz`.

Note: `rdkafka` links against `librdkafka`, `libssl`, and `libsasl2`. For a
fully static musl build you need musl-compatible builds of those libraries
available at link time. The recommended path for container deployments is the
provided Dockerfile, which builds against glibc and packages the required
runtime libraries in the image.

## Container Build

Build the image from the project root:

```bash
docker build -t xzardgz:latest .
```

The Dockerfile uses a two-stage build. The builder stage compiles the release
binary inside `rust:1.87-slim`. The runtime stage copies only the binary into
`debian:bookworm-slim` and installs `libssl3`, `libsasl2-2`, and
`ca-certificates`. The container runs as the non-root `xzardgz` system user.

### One-Shot Container

Run a single analysis and exit:

```bash
docker run --rm \
  -e OPENAI_API_KEY="${OPENAI_API_KEY}" \
  -v "$(pwd):/workspace" \
  -w /workspace \
  xzardgz:latest \
  xzardgz run --repository . --plugin technical-review --config config.yaml
```

### Watcher Container

Run as a long-running Kafka consumer:

```bash
docker run -d \
  --name xzardgz-watcher \
  -e OPENAI_API_KEY="${OPENAI_API_KEY}" \
  -e KAFKA_SASL_USERNAME="${KAFKA_SASL_USERNAME}" \
  -e KAFKA_SASL_PASSWORD="${KAFKA_SASL_PASSWORD}" \
  -v /path/to/config.yaml:/etc/xzardgz/config.yaml:ro \
  -v xzardgz-workspaces:/workspaces \
  xzardgz:latest \
  xzardgz watch --config /etc/xzardgz/config.yaml
```

Mount `config.yaml` read-only and use a named volume for the workspace directory
so that reports and artifacts persist across restarts.

## GitHub Actions

The workflow at `.github/workflows/security_review.yaml` runs an automated
security review on every push to `main` and on every pull request.

### Setup

1. Add `OPENAI_API_KEY` to your repository secrets under **Settings > Secrets
   and variables > Actions**.
2. The workflow checks out the repository, builds and installs `xzardgz`, runs
   the security-review plugin, and uploads SARIF output to GitHub Code Scanning.

SARIF findings appear in the **Security** tab under **Code scanning alerts**.

### Required Secrets

| Secret           | Description                     |
| ---------------- | ------------------------------- |
| `OPENAI_API_KEY` | API key for the OpenAI provider |

### What the Workflow Does

1. Checks out the repository with `actions/checkout@v4`.
2. Caches the Cargo registry and `target/` directory to speed up repeated
   builds.
3. Installs `xzardgz` with `cargo install --path .`.
4. Writes a minimal `config.yaml` enabling the security-review plugin with SARIF
   output.
5. Runs `xzardgz run --plugin security-review --output sarif-results`.
6. Uploads the SARIF file from `sarif-results/` to GitHub Code Scanning using
   `github/codeql-action/upload-sarif@v3`. The upload step runs even if the
   review step fails (`if: always()`).

## Kubernetes Watcher Deployment

Deploy the watcher as a Kubernetes `Deployment` for long-running event
processing.

### Replicas

Set `replicas: 1`. The watcher maintains a Kafka consumer group membership.
Running multiple replicas requires careful partition assignment planning and a
unique `group_id` per replica if parallel independent processing is needed.

### Configuration

Mount `config.yaml` from a `ConfigMap` volume:

```yaml
volumes:
  - name: config
    configMap:
      name: xzardgz-config
volumeMounts:
  - name: config
    mountPath: /etc/xzardgz
    readOnly: true
```

Pass the config path to the watcher command:

```yaml
command: ["xzardgz", "watch", "--config", "/etc/xzardgz/config.yaml"]
```

### Secrets

Pass API keys and Kafka credentials from a Kubernetes `Secret`:

```yaml
env:
  - name: OPENAI_API_KEY
    valueFrom:
      secretKeyRef:
        name: xzardgz-secrets
        key: openai-api-key
  - name: KAFKA_SASL_USERNAME
    valueFrom:
      secretKeyRef:
        name: xzardgz-secrets
        key: kafka-sasl-username
  - name: KAFKA_SASL_PASSWORD
    valueFrom:
      secretKeyRef:
        name: xzardgz-secrets
        key: kafka-sasl-password
```

The secret key names in `config.yaml` (`kafka.sasl_username_env`,
`kafka.sasl_password_env`) must match the environment variable names set here.

### Workspace Persistence

Attach a `PersistentVolumeClaim` for the workspace directory when reports and
artifacts must survive pod restarts:

```yaml
volumes:
  - name: workspaces
    persistentVolumeClaim:
      claimName: xzardgz-workspaces
volumeMounts:
  - name: workspaces
    mountPath: /workspaces
```

Set `workspace.root: /workspaces` in `config.yaml` to point the watcher at the
mounted path.

### Resource Limits

Analysis workloads are CPU and memory intensive. Suggested starting values:

```yaml
resources:
  requests:
    cpu: "500m"
    memory: "1Gi"
  limits:
    cpu: "1"
    memory: "2Gi"
```

Adjust based on observed usage, the number of concurrent tasks, and the model
being used.

### Probes

```yaml
livenessProbe:
  exec:
    command: ["xzardgz", "--version"]
  initialDelaySeconds: 5
  periodSeconds: 30

readinessProbe:
  exec:
    command:
      ["xzardgz", "watch", "--dry-run", "--config", "/etc/xzardgz/config.yaml"]
  initialDelaySeconds: 10
  periodSeconds: 60
  failureThreshold: 3
```

The liveness probe checks that the binary is executable. The readiness probe
validates the configuration and Kafka connectivity without consuming any
messages.

### CronJob Pattern

Set `watcher.once: true` in `config.yaml` (or pass `--once` on the command line)
to process one batch of available messages and exit. Deploy as a Kubernetes
`CronJob` for scheduled batch reviews instead of a continuously running
`Deployment`.

## Health and Readiness

For watcher deployments, the following checks are available before or during
operation.

### Validate Configuration Without Consuming Messages

```bash
xzardgz watch --dry-run --config config.yaml
```

Exits 0 if the configuration is valid and the matcher is non-empty. Prints a
summary of matcher rules, executor settings, and a warning if the matcher would
reject all tasks.

### Process a Bounded Batch and Exit

```bash
xzardgz watch --once --config config.yaml
```

Processes one batch of available tasks and exits. Useful for smoke testing and
`CronJob` deployments.

### Monitor Log Output

```bash
RUST_LOG=info xzardgz watch --config config.yaml
```

The `info` level logs consumer group activity, task dispatch, plugin results,
and Kafka publish status. Use `RUST_LOG=xzardgz=debug` for detailed per-task
tracing.

### Inspect Publish Failures

If Kafka publishing fails after a plugin run completes, the result is persisted
to `publish_failure/` inside the workspace directory. Inspect those files to
recover results without re-running the analysis.

## Environment Variables Reference

| Variable              | Description                                                             |
| --------------------- | ----------------------------------------------------------------------- |
| `OPENAI_API_KEY`      | API key read by the OpenAI provider. Set `openai.api_key_env` to match. |
| `ANTHROPIC_API_KEY`   | API key read by the Anthropic provider.                                 |
| `XZARDGZ_CONFIG`      | Path to the config file. Overrides `--config` and compiled defaults.    |
| `XZARDGZ_PROVIDER`    | Provider name override: `openai`, `anthropic`, `ollama`, `copilot`.     |
| `XZARDGZ_MODEL`       | Model name override for the selected provider.                          |
| `XZARDGZ_WORKSPACE`   | Workspace root directory override.                                      |
| `KAFKA_SASL_USERNAME` | Kafka SASL username. Configure `kafka.sasl_username_env` to this name.  |
| `KAFKA_SASL_PASSWORD` | Kafka SASL password. Configure `kafka.sasl_password_env` to this name.  |
| `RUST_LOG`            | Tracing log filter, e.g. `info`, `debug`, `xzardgz=debug`.              |
