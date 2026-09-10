# Demo Directories: Phase 2 Implementation

## Overview

Phase 2 of the demo directories plan adds four new capability-specific demo
directories alongside the feature plans that land them, and upgrades the
existing `demo/watcher/` from a static message reference to a full end-to-end
walkthrough. The `demo/README.md` index is updated to reflect the complete demo
set.

## What Changed

### 2.1 New Demo Directories

#### `demo/scan/`

Demonstrates the `xzardgz scan` command. This demo is entirely offline and
requires no AI API key, making it the lowest-friction first step for a new
contributor.

| File                            | Purpose                                                                        |
| ------------------------------- | ------------------------------------------------------------------------------ |
| `README.md`                     | Four-step walkthrough: dry-run, scan, inspect artifact, custom correlation ID  |
| `config.yaml`                   | Minimal scanner configuration                                                  |
| `fixture-repo/README.md`        | Description of the bundled Python data pipeline project                        |
| `fixture-repo/pipeline.py`      | Main pipeline runner: reads JSON, filters `None`-valued records, writes output |
| `fixture-repo/models.py`        | `Record` and `TransformResult` dataclasses                                     |
| `fixture-repo/requirements.txt` | Empty dependency list                                                          |

**Key commands** documented with exact expected output:

```bash
# Dry-run (no files written)
xzardgz scan --repository demo/scan/fixture-repo \
  --output /tmp/xzardgz-demo-scan.json \
  --config demo/scan/config.yaml --dry-run

# Full scan producing a JSON artifact
xzardgz scan --repository demo/scan/fixture-repo \
  --output /tmp/xzardgz-demo-scan.json \
  --config demo/scan/config.yaml
```

#### `demo/security-review/`

Demonstrates the `xzardgz run --plugin security-review` command across three
scenarios of increasing complexity. Satisfies the Phase 2.6 success criterion by
including a live run against a real public GitHub repository.

| File                            | Purpose                                                              |
| ------------------------------- | -------------------------------------------------------------------- |
| `README.md`                     | Three-scenario walkthrough: offline dry-run, live local, live GitHub |
| `config.yaml`                   | Security review configuration shared across all scenarios            |
| `fixture-repo/README.md`        | Description of the bundled Python auth service                       |
| `fixture-repo/app.py`           | HTTP handler: login dispatch                                         |
| `fixture-repo/auth.py`          | SHA-256 token verification and session helpers                       |
| `fixture-repo/db.py`            | In-memory user store helpers                                         |
| `fixture-repo/requirements.txt` | Empty dependency list                                                |

**Scenario C** targets `https://github.com/pallets/jinja` directly, satisfying
the Phase 2.6 criterion. Jinja was chosen because it is a well-maintained Python
project with a clear security surface (sandboxed template rendering, known CVE
history) and is large enough for meaningful findings without exceeding a single
model context window.

#### `demo/git-pr/`

Demonstrates the git write primitives (`create_branch`, `commit_paths`,
`push_branch`) and GitHub pull request creation. Staged in three complexity
levels: dry-run, full workflow without PR, full workflow with PR.

| File                            | Purpose                                                             |
| ------------------------------- | ------------------------------------------------------------------- |
| `README.md`                     | Three-stage walkthrough: dry-run, workflow run, PR creation         |
| `config.yaml`                   | Demo config with `pr.enabled: false` guard (must opt in explicitly) |
| `workflow.yaml`                 | Technical review plan used as the PR source workflow                |
| `fixture-repo/README.md`        | Description of the placeholder repository                           |
| `fixture-repo/findings.md`      | Placeholder file XZardgz would overwrite with generated reports     |
| `fixture-repo/requirements.txt` | Empty dependency list                                               |

The README documents: branch naming (`xzardgz/<ulid>`), the safety guard
preventing PRs targeting the default branch, and the full credential lookup
order (`XZARDGZ_GITHUB_TOKEN` env var → OS keyring → SSH agent → libgit2
default).

### 2.2 Upgraded Demo: `demo/watcher/`

The existing `demo/watcher/` contained only task message JSON files with no
walkthrough. Phase 2 adds a `config.yaml` and rewrites `README.md` as a full
three-stage end-to-end demo.

**Added**: `demo/watcher/config.yaml` — demo Kafka configuration with two event
types, two plugins, and a named demo topic pair (`xzardgz.demo.tasks`,
`xzardgz.demo.results`).

**Rewritten**: `demo/watcher/README.md` now covers:

1. Offline dry-run validation (`xzardgz watch --dry-run`) with exact expected
   output.
2. Live consumer loop: starting a local Kafka broker with Docker, creating
   topics, starting the watcher, publishing a `CloudEventMessage` task, and
   reading results.
3. Reference task message schema explaining each payload field.
4. Correlation ID threading: how `data.events[0].payload.correlation_id` is
   carried from the inbound message through to the published result.
5. Troubleshooting table for the five most common failure modes.

The existing `technical_review_task.json` and `security_review_task.json`
reference files were not modified.

### 2.3 Updated Index: `demo/README.md`

`demo/README.md` was updated to include entries for all five capability demos
(`scan/`, `security-review/`, `git-pr/`, `mcp/`, `watcher/`), with a one-line
description of each demo, links to its `README.md`, and an updated prerequisites
section noting the per-demo requirements (Kafka, GitHub token, Node.js, OpenAI
API key).

## Design Decisions

### Why pallets/jinja for the Live GitHub Demo

The Phase 2.6 criterion requires at least one demo targeting a real public
GitHub repository. `pallets/jinja` was chosen because:

- It is publicly accessible without authentication.
- It has a clear security surface (template sandboxing, `str.format_map` bypass
  CVE history) that makes a security review demo meaningful.
- It is large enough to produce realistic findings but small enough to fit in a
  single model context window.
- It has no runtime service dependencies, so the demo is fully reproducible.

### Why `demo/scan/` Has Its Own Fixture Repo

The scan demo's fixture repo (`pipeline.py`, `models.py`) is distinct from the
MCP demo's fixture repo (`main.py`, `utils.py`) to avoid confusion about which
demo they belong to. Each demo is self-contained.

### Why the git-pr Demo Has No AI-Free Path

The `create_branch`, `commit_paths`, and `push_branch` primitives operate on any
git repository, but the most natural entry point for a user is the full workflow
(scan + review + branch + commit + push + PR). A standalone CLI command for the
git primitives does not exist — they are workflow executor internals. The demo
therefore starts with a dry-run of the workflow plan as the offline-accessible
first step.

## Success Criteria

- Phase 2.5: Full demo coverage across the companion plans (scan,
  security-review, git-pr, watcher). All four are delivered.
- Phase 2.6: The `demo/security-review/` Scenario C targets
  `https://github.com/pallets/jinja`, a real public GitHub repository.
