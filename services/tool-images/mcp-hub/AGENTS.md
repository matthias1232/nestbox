# mcp-hub — an MCP server we ship, so tool calls are visible

## Purpose

The sandbox's own files and shell, exposed as MCP tools over HTTP. It exists so
that what an agent does inside its sandbox is an **observable fact**: an HTTP MCP
server is reached through the egress proxy, so every `tools/call` appears in the
traffic log with a tool name, redacted arguments, and a duration.

A `stdio` MCP server would be simpler and would be invisible by construction —
it runs as a child process inside the container and never crosses the proxy. That
is fine for third-party servers, and it has to be said plainly in the console
rather than shown as "no activity".

## Contract

`POST /mcp` with one JSON-RPC 2.0 request. `GET /healthz` for liveness. Bearer
token from `NESTBOX_MCP_TOKEN`; unset means refuse everything, because a missing
secret must never mean "no check".

| Tool | Does |
|---|---|
| `read_file` | Read a UTF-8 file inside `NESTBOX_HOME` |
| `write_file` | Write a UTF-8 file inside `NESTBOX_HOME` |
| `run_command` | Run an argument vector inside `NESTBOX_HOME` |

## Security rules

1. Paths are resolved with `realpath` and then compared to the home, so `..` and
   a symlink to `/etc` are both refused. A string check is not enough.
2. `run_command` takes a list, never a string, and runs with `shell=False`.
3. Output is capped and the command has a timeout, so one tool call cannot fill
   the proxy's buffer or hold a session open.
4. Bodies are not logged here. The proxy logs them, once, through the redactor; a
   second copy in this container's stdout is a leak waiting to happen.
5. The container runs as a non-root user with no capabilities.

## Open work (package `feat/mcp-registry`)

- The streaming half of the MCP transport (server-sent events for
  server-initiated messages); today a `GET /mcp` is a clear 400.
- `resources/*` and `prompts/*`, and the `initialize` capability negotiation
  beyond tools.
- Tool discovery results written into `sandbox_mcp_servers.tools`, and the
  `Nestbox hub` entry in the compose stack.
