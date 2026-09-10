# MCP Demo: Validate and Introspect an MCP Server

This demo walks through validating an MCP server configuration, listing
configured servers, and discovering tools exposed by the bundled filesystem
server. All commands are run against the `fixture-repo/` directory in this
folder, so no external repository or live service is required.

## What This Demo Shows

- How to validate an MCP configuration with `xzardgz mcp validate`.
- How to list configured servers with `xzardgz mcp list-servers`.
- How to discover tools a server exposes with `xzardgz mcp list-tools` (requires
  Node.js).

## Prerequisites

- `xzardgz` installed and on your `PATH`:

  ```bash
  cargo install --path .
  ```

- For the `list-tools` step only: Node.js 18 or later with `npx` available. The
  `validate` and `list-servers` steps do not require Node.js.

Run all commands from the **repository root**, not from inside `demo/mcp/`.

## Fixture Repository

The `fixture-repo/` subdirectory is a minimal Python project that serves as the
analysis target:

```text
fixture-repo/
  README.md          Project description
  main.py            Entry point: prints a greeting
  utils.py           Utility module: greet() function
  requirements.txt   Dependency list (empty for this demo)
```

The MCP filesystem server exposes this directory to XZardgz plugins so that they
can read and analyse its files during a workflow run.

## Step 1: Review the Demo Configuration

Open `demo/mcp/config.yaml`. It registers one MCP server named `filesystem` that
uses the `@modelcontextprotocol/server-filesystem` Node.js package to expose
`demo/mcp/fixture-repo/`:

```yaml
mcp:
  servers:
    - name: "filesystem"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-filesystem"
        - "demo/mcp/fixture-repo"
      allowed_tools:
        - "read_file"
        - "list_directory"
        - "get_file_info"
```

The `allowed_tools` list controls which tools plugins are permitted to call.
Tools not in this list cannot be invoked even if the server exposes them.

## Step 2: Validate the Configuration

Check that the configuration is structurally valid without connecting to any
server:

```bash
xzardgz mcp validate --config demo/mcp/config.yaml
```

Expected output:

```text
MCP configuration is valid.
Configured servers (1):
  - filesystem
```

## Step 3: List Configured Servers

Print a summary of every registered server including transport and timeout:

```bash
xzardgz mcp list-servers --config demo/mcp/config.yaml
```

Expected output:

```text
Configured MCP servers (1):
  - filesystem (transport: stdio, timeout: 30s)
```

## Step 4: List Tools (requires Node.js)

Spawn the filesystem MCP server as a subprocess and query its tool manifest.
Node.js 18 or later must be installed and `npx` must be on your `PATH`.

```bash
xzardgz mcp list-tools filesystem --config demo/mcp/config.yaml
```

Expected output (tool descriptions may vary by package version):

```text
Tools on server 'filesystem' (3):
  - read_file - Read the complete contents of a file from the file system.
  - list_directory - Get a listing of all files and directories in a path.
  - get_file_info - Retrieve metadata about a file or directory.
```

If Node.js is not installed, skip this step. Steps 2 and 3 are fully offline.

## What to Try Next

- Open `demo/mcp/fixture-repo/utils.py` and read it through
  `xzardgz mcp test-invoke` to see a live tool call result.
- Add the `git` server from `demo/mcp/mcp_server_config.yaml` to
  `demo/mcp/config.yaml` and run `xzardgz mcp list-servers` again to see both
  servers listed.
- Run `xzardgz run --plan demo/plans/analyze_repo.yaml` with an MCP-aware
  configuration to see plugins calling MCP tools during a full workflow run.

## Further Reading

- [MCP Configuration Reference](../../docs/reference/mcp_configuration.md)
- [CLI Reference: mcp commands](../../docs/reference/cli.md)
