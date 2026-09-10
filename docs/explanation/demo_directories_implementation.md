# Demo Directories: Phase 1 Implementation

## Overview

Phase 1 of the demo directories plan establishes the `demo/` tree, fixes broken
documentation references to a nonexistent `examples/` directory, and delivers
the first end-to-end, self-contained demo targeting the MCP client capability.

## What Changed

### 1.1 Broken Reference Fixes

`docs/README.md` previously contained an "Examples" section (lines 122-131) that
linked to `examples/plans/`, `examples/watcher/`, `examples/kafka/`,
`examples/mcp/`, and `examples/prompts/` -- none of which exist in the
repository. That section was replaced with a "Demos" section that links to the
corresponding subdirectories under `demo/`.

`docs/how-to/setup_watcher.md` referenced
`examples/watcher/technical_review_task.json` in the "Send a Test Task" section.
This was updated to `demo/watcher/technical_review_task.json` and the `source`
field in the accompanying JSON snippet was updated from `"xzardgz/examples"` to
`"xzardgz/demo"`.

### 1.2 MCP Demo

#### `demo/mcp/fixture-repo/`

A minimal Python project was added as a bundled analysis target:

| File               | Purpose                                                       |
| ------------------ | ------------------------------------------------------------- |
| `README.md`        | Project description with run instructions                     |
| `main.py`          | Entry point; calls `greet("world")` and prints the result     |
| `utils.py`         | `greet(name)` function with full docstring and error handling |
| `requirements.txt` | Empty dependency list with explanatory comment                |

The fixture repository is deliberately small so that the demo runs quickly and
offline. It demonstrates the kind of project xzardgz analyses without requiring
access to any external repository.

#### `demo/mcp/config.yaml`

A demo-specific xzardgz configuration file was added alongside the existing
`mcp_server_config.yaml`. It registers one MCP server named `filesystem` that
exposes `demo/mcp/fixture-repo/` using
`npx @modelcontextprotocol/server-filesystem`. All non-MCP config sections use
xzardgz defaults, so the file is minimal and focused.

#### `demo/mcp/README.md`

The previous README was a general config explanation. It was replaced with a
four-step end-to-end walkthrough:

1. Review the demo configuration.
2. Validate the configuration (`xzardgz mcp validate`).
3. List configured servers (`xzardgz mcp list-servers`).
4. List tools exposed by the server (`xzardgz mcp list-tools`, requires
   Node.js).

Each step documents the exact command and the expected terminal output. Steps 2
and 3 are fully offline and do not require Node.js or any running service.

### 1.3 Demo Index

`demo/README.md` was updated to:

- Describe each subdirectory as a runnable demo rather than a static example
  file collection.
- Highlight that `demo/mcp/` contains a `fixture-repo/` for offline use.
- List general prerequisites separately from per-demo prerequisites.

## Design Decisions

### Why `demo/` Rather Than `examples/`

The repository already used `demo/` as the directory name before this plan was
written. The plan formalises this convention and removes the stale `examples/`
references from documentation.

### Why MCP as the First Demo

MCP client support is fully functional and independent of in-progress feature
plans. It requires no AI provider key for the `validate` and `list-servers`
steps, making it the most accessible first demo for new contributors.

### Why a Python Fixture Repository

A Python project has fewer lines and dependencies than an equivalent Rust
project while still providing meaningful content for file-reading tools such as
`read_file` and `list_directory`. The fixture is intentionally small so the demo
runs in seconds.

## Success Criteria

A new contributor can:

1. Clone the repository.
2. Run `cargo install --path .` to install `xzardgz`.
3. Follow `demo/mcp/README.md` from top to bottom.
4. Observe the documented expected output for `mcp validate` and
   `mcp list-servers` without any additional setup.
5. Observe the documented expected output for `mcp list-tools` after installing
   Node.js.
