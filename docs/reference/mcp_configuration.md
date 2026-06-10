# MCP Configuration Reference

## Overview

MCP (Model Context Protocol) is a JSON-RPC 2.0 based protocol that allows the
XZardgz pipeline to discover and invoke tools exposed by external server
processes. An MCP server is a separate process, started on demand by the
pipeline, that advertises a set of named tools. Plugins can call those tools
during analysis to retrieve external information such as file listings, git
history, web search results, or CVE database lookups.

MCP servers are optional. The pipeline functions without any MCP configuration.
Servers are only started when a plugin requests a tool provided by that server.

### Security Model

XZardgz enforces an explicit allow-list before any plugin can call an MCP tool.
A tool that is discoverable from a server but not present in `allowed_tools` is
unavailable to plugins. This means that adding a new MCP server to the
configuration does not automatically grant plugins access to its tools. Each
tool must be listed by name in the configuration before it can be used.

---

## Server Configuration

MCP servers are defined in the `mcp.servers` list. Each entry is a server
definition object.

```yaml
mcp:
  servers:
    - name: "filesystem"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-filesystem"
        - "/path/to/allowed/directory"
      env: {}
      timeout_seconds: 30
      transport: "stdio"
      allowed_tools:
        - "read_file"
        - "list_directory"
      auth: null
  allowed_tools:
    filesystem:
      - "read_file"
      - "list_directory"
  timeout_seconds: 30
```

### Server Definition Fields

| Field             | Type                      | Required | Default               | Description                                             |
| ----------------- | ------------------------- | -------- | --------------------- | ------------------------------------------------------- |
| `name`            | String                    | Yes      | -                     | Unique server identifier used in CLI commands and logs  |
| `command`         | String                    | Yes      | -                     | Executable to launch; resolved via `PATH`               |
| `args`            | List of strings           | No       | `[]`                  | Arguments passed to `command` in order                  |
| `env`             | Map of string to string   | No       | `{}`                  | Additional environment variables for the server process |
| `timeout_seconds` | Integer (greater than 0)  | No       | `mcp.timeout_seconds` | Per-server timeout for tool discovery and invocation    |
| `transport`       | String (`stdio` or `sse`) | No       | `"stdio"`             | Transport type for communicating with the server        |
| `allowed_tools`   | List of strings           | No       | `[]`                  | Tool names this server may expose to plugins            |
| `auth`            | Auth object or null       | No       | `null`                | Optional authentication for server startup              |

### `mcp` Top-level Fields

| Field             | Type                            | Default | Description                                                   |
| ----------------- | ------------------------------- | ------- | ------------------------------------------------------------- |
| `servers`         | List of server definitions      | `[]`    | All configured MCP servers                                    |
| `allowed_tools`   | Map of server name to tool list | `{}`    | Top-level allow-list; merged with per-server `allowed_tools`  |
| `timeout_seconds` | Integer (greater than 0)        | `30`    | Default timeout applied to any server without its own timeout |

The `allowed_tools` map at the top level and the `allowed_tools` list inside
each server definition are both consulted. A tool is allowed only if it appears
in at least one of these locations for its server.

---

## Transport Types

### `stdio`

The default transport. The pipeline starts the server as a child process and
communicates with it over the child process's stdin and stdout pipes using
newline-delimited JSON-RPC 2.0 messages. The child process is terminated when
the pipeline exits.

Most publicly available MCP servers use `stdio`. This transport requires no
network configuration and is the most secure option for local servers.

```yaml
transport: "stdio"
```

### `sse`

Server-Sent Events transport for connecting to a remotely hosted MCP server. The
pipeline connects to an HTTP endpoint and sends requests as JSON-RPC 2.0
messages over SSE. Use this transport when the MCP server is a persistent
service rather than a per-run subprocess.

```yaml
transport: "sse"
```

When using `sse`, the `command` and `args` fields are not used for process
spawning. The `command` field instead holds the base URL of the SSE endpoint.

---

## Tool Allow-listing

Every MCP tool must be explicitly allowed before plugins can call it. The
pipeline enforces this at the registry level: when `list_tools` is called for a
server, the result is filtered to include only tools that appear in the
effective allow-list for that server.

The effective allow-list for a server is the union of:

- The `allowed_tools` list inside the server definition
- The tool list for that server name in the top-level `mcp.allowed_tools` map

### Why Explicit Allow is Required

MCP servers can expose tools with broad capabilities such as filesystem write
access, shell execution, or network requests. An explicit allow-list prevents a
server update from silently granting plugins access to new or changed tools.
Operators control exactly which tools are available at the configuration level.

### Example Allow-list Configuration

To allow only the `read_file` and `list_directory` tools from a server named
`filesystem`:

```yaml
mcp:
  allowed_tools:
    filesystem:
      - "read_file"
      - "list_directory"
```

Or equivalently, on the server definition:

```yaml
mcp:
  servers:
    - name: "filesystem"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-filesystem"
        - "/repo"
      allowed_tools:
        - "read_file"
        - "list_directory"
```

Both forms produce the same behavior. The top-level map form is convenient when
allow-lists are managed separately from server definitions.

---

## The `mcp` CLI Commands

### `mcp validate`

Validate all configured MCP server definitions without connecting to any server.
Checks that each server has a non-empty name and command, that the transport
value is recognized, and that timeout values are positive.

```bash
xzardgz mcp validate --config config.yaml
```

Exits nonzero if any validation error is found. Use this command in CI to catch
configuration errors before a pipeline run.

### `mcp servers`

List all configured servers and their basic metadata.

```bash
xzardgz mcp servers --config config.yaml
```

Output includes the server name, command, transport type, and the number of
configured allowed tools.

### `mcp tools`

Connect to a named server, run the initialization handshake, discover its
exposed tools, and print the list filtered by the allow-list.

```bash
xzardgz mcp tools <SERVER> --config config.yaml
```

Example:

```bash
xzardgz mcp tools filesystem --config config.yaml
```

This command starts the server subprocess, performs the MCP protocol handshake,
calls `tools/list`, applies the configured allow-list filter, and prints the
resulting tool names and descriptions. Tools that exist on the server but are
not in the allow-list are not shown.

Use this command to confirm that a newly configured server is reachable and that
its tools are visible through the allow-list.

### `mcp test-tool`

Connect to a named server and perform a test invocation of a specific tool.

```bash
xzardgz mcp test-tool <SERVER> <TOOL> --config config.yaml
```

Example:

```bash
xzardgz mcp test-tool filesystem list_directory --config config.yaml
```

The test invocation uses a safe default argument set appropriate for the tool.
The command prints the raw result returned by the server. Use this command to
verify that a tool is callable and returns expected output before relying on it
in a plugin run.

---

## Example: Filesystem MCP Server

The `@modelcontextprotocol/server-filesystem` package provides read and write
access to a directory tree. The following configuration restricts the server to
read-only operations on the repository directory.

```yaml
mcp:
  servers:
    - name: "filesystem"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-filesystem"
        - "/workspace/repo"
      env: {}
      timeout_seconds: 30
      transport: "stdio"
      allowed_tools:
        - "read_file"
        - "list_directory"
        - "get_file_info"
  allowed_tools:
    filesystem:
      - "read_file"
      - "list_directory"
      - "get_file_info"
  timeout_seconds: 30
```

The path argument (`/workspace/repo`) restricts the server to that directory
tree. The allow-list further restricts the tools to read-only operations.
Write-capable tools such as `write_file` and `move_file` are excluded.

Validate the configuration:

```bash
xzardgz mcp validate --config config.yaml
```

Confirm that tools are discoverable:

```bash
xzardgz mcp tools filesystem --config config.yaml
```

---

## Example: Git MCP Server

The `@modelcontextprotocol/server-git` package provides git operations as MCP
tools. The following configuration allows the pipeline to read git log and diff
information during analysis.

```yaml
mcp:
  servers:
    - name: "git"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-git"
        - "--repository"
        - "/workspace/repo"
      env: {}
      timeout_seconds: 30
      transport: "stdio"
      allowed_tools:
        - "git_log"
        - "git_diff"
        - "git_show"
  allowed_tools:
    git:
      - "git_log"
      - "git_diff"
      - "git_show"
  timeout_seconds: 30
```

Test the git log tool before running the pipeline:

```bash
xzardgz mcp test-tool git git_log --config config.yaml
```

---

## Security Considerations

### Allow-list Discipline

Review the allow-list carefully before adding any tool that can modify files,
execute shell commands, or make outbound network requests. The allow-list is the
primary defense against accidental or malicious tool misuse.

Prefer minimal allow-lists. Add tools one at a time and verify each with
`mcp test-tool` before including them in a production configuration.

### Tool Discovery Before Use

Use `mcp tools <SERVER>` to discover what tools a server actually exposes after
filtering. Never assume that a tool is available based on documentation alone.
Server versions may differ from what is documented, and the allow-list may
exclude tools that the server exposes.

### Timeout Configuration

Set `timeout_seconds` conservatively. A tool that hangs waiting for a network
resource can stall an entire pipeline run. The default is 30 seconds. For
servers that perform network lookups, consider reducing this to 10 or 15 seconds
to fail fast.

Per-server timeout values override the global `mcp.timeout_seconds` for that
server only:

```yaml
mcp:
  timeout_seconds: 30
  servers:
    - name: "web-search"
      command: "npx"
      args: ["-y", "@modelcontextprotocol/server-brave-search"]
      env:
        BRAVE_API_KEY_ENV: "BRAVE_API_KEY"
      timeout_seconds: 10
      transport: "stdio"
      allowed_tools:
        - "brave_web_search"
```

### Secret Injection

Do not place secret values directly in the `env` map. Use environment variable
name indirection: set the key to a variable name whose value is read from the
process environment at runtime.

```yaml
env:
  API_KEY: "MY_SERVICE_API_KEY"
```

In this example, `MY_SERVICE_API_KEY` is the name of an environment variable in
the pipeline's process environment, not the secret value itself.

---

## Troubleshooting

### Validation Errors

| Error Message                      | Cause                                                | Resolution                                          |
| ---------------------------------- | ---------------------------------------------------- | --------------------------------------------------- |
| `server name is empty`             | `name` field is missing or blank                     | Add a non-empty `name` to the server definition     |
| `server command is empty`          | `command` field is missing or blank                  | Set `command` to the executable path or name        |
| `unsupported transport`            | `transport` is not `stdio` or `sse`                  | Change `transport` to `stdio` or `sse`              |
| `timeout_seconds must be positive` | `timeout_seconds` is zero or negative                | Set `timeout_seconds` to a positive integer         |
| `server not found`                 | Server name passed to a CLI command is not in config | Add the server to `mcp.servers` or correct the name |

### Tool Discovery Failures

If `mcp tools <SERVER>` returns an empty list or fails:

1. Run `mcp validate` to confirm the server configuration is syntactically
   valid.
2. Check that the `command` is installed and on `PATH`. For `npx`-based servers,
   run `npx -y <package> --help` manually to confirm the package installs
   correctly.
3. Confirm that the allow-list contains at least one tool name that the server
   actually exposes. An allow-list that does not match any server tool produces
   an empty result without an error.
4. Increase `timeout_seconds` if the server takes longer than the default 30
   seconds to start and respond.
5. Check `RUST_LOG=debug` output for the raw JSON-RPC messages exchanged during
   initialization to diagnose protocol-level failures.

### Protocol Version Mismatch

XZardgz uses MCP protocol version `2024-11-05`. If a server responds with a
different protocol version, the connection is rejected with a
`McpProtocolVersionMismatch` error. Update the server package to a version that
supports `2024-11-05`.
