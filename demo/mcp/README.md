# MCP Server Configuration Example

`mcp_server_config.yaml` contains annotated MCP (Model Context Protocol) server
definitions. Copy the server entries you need into the `mcp.servers` section of
your `config.yaml`.

## What Is MCP

MCP is an open protocol that allows AI agents to call tools hosted by external
processes. XZardgz uses MCP servers to give plugins access to capabilities such
as reading files, querying git history, or running web searches, without
building those capabilities directly into the binary.

All MCP tool calls are opt-in. A tool must appear in the `allowed_tools` list
for the server before any plugin can call it.

## What Is Included

The file defines two example servers:

| Server       | Command                                       | Purpose                                                |
| ------------ | --------------------------------------------- | ------------------------------------------------------ |
| `filesystem` | `npx @modelcontextprotocol/server-filesystem` | Read files and list directories inside a project tree. |
| `git`        | `npx @modelcontextprotocol/server-git`        | Query git log, diff, and status for a repository.      |

## How to Use

1. Ensure Node.js and `npx` are available on your system.
2. Copy the server definition you want into your `config.yaml` under
   `mcp.servers`.
3. Add the tools you want plugins to use to `allowed_tools`.
4. Adjust the `args` list to point at your project directory.
5. Validate the server configuration:

```bash
xzardgz mcp validate --config config.yaml
```

1. List the tools the server exposes:

```bash
xzardgz mcp tools filesystem --config config.yaml
```

## Minimal Configuration

Copy this block into `config.yaml` to enable the filesystem server for the
current directory:

```yaml
mcp:
  timeout_seconds: 30
  servers:
    - name: "filesystem"
      command: "npx"
      args:
        - "-y"
        - "@modelcontextprotocol/server-filesystem"
        - "."
      env: {}
      timeout_seconds: 30
      transport: "stdio"
      allowed_tools:
        - "read_file"
        - "list_directory"
      auth: null
```

Replace `"."` with the absolute path to the directory you want to expose to
plugins.

## Allowing Tools

Tools are gated by an explicit allow list. A tool that is not listed in
`allowed_tools` cannot be called even if the server exposes it.

To discover what tools a server offers before deciding what to allow:

```bash
xzardgz mcp tools filesystem --config config.yaml
```

To test a specific tool call:

```bash
xzardgz mcp test-tool filesystem read_file --config config.yaml
```

## Transport Types

Both servers in this example use `"stdio"` transport: XZardgz spawns the server
process and communicates over its standard input and output. This is the most
common MCP transport and requires no network configuration.

An alternative transport type `"sse"` (Server-Sent Events) is available for
servers that run as persistent HTTP services.

## Security Considerations

- Set `allowed_tools` to the minimum set of tools your plugins actually need.
- Use absolute paths in `args` to avoid ambiguity about which directory is
  accessible.
- Do not expose directories outside the repository being analysed.
- Prefer `"stdio"` transport for local servers; use `"sse"` only for remote
  servers with appropriate network controls.
- MCP server processes inherit the environment of the watcher or `run` process.
  Avoid placing secrets in the `env` map; inject them as environment variables
  in the outer process instead.

## Installing the Example Servers

Both servers are distributed as npm packages and can be run without a permanent
install using `npx -y`. To install them permanently:

```bash
npm install -g @modelcontextprotocol/server-filesystem
npm install -g @modelcontextprotocol/server-git
```

Then change `command` to the installed binary path and remove the `npx -y`
prefix from `args`.

## Further Reading

- [MCP Configuration Reference](../../docs/reference/mcp_configuration.md)
- [CLI Reference: mcp commands](../../docs/reference/cli.md)
