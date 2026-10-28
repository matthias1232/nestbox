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

Methods: `initialize`, `ping`, `tools/list`, `tools/call`, and the
`notifications/*` family (answered `202` with no body). Anything else is
`-32601`, including `resources/*` and `prompts/*`, which are not advertised in
`capabilities`.

## Transport decisions

- **Sessions are issued, not kept.** `initialize` returns an `Mcp-Session-Id`
  header (a random id) and accepts it echoed on later requests. The hub holds no
  session state; the header exists so the proxy can log a session id next to
  every call in the traffic view.
- **No server-to-client stream.** `GET /mcp` answers `405` with
  `Allow: POST, DELETE`. A half-open SSE stream that never delivers a message
  would look like a healthy server with a broken tool list, which is worse than
  an explicit refusal. `DELETE /mcp` accepts a session termination with `204`.
- **One request per call.** Batching is not supported; a client that sends an
  array gets `-32600` and can retry one message at a time.

## Security rules

1. Paths are resolved with `realpath` and then compared to the home, so `..` and
   a symlink to `/etc` are both refused. A string check is not enough.
2. `run_command` takes a list, never a string, and runs with `shell=False`.
3. Output is capped and the command has a timeout, so one tool call cannot fill
   the proxy's buffer or hold a session open.
4. Bodies are not logged here. The proxy logs them, once, through the redactor; a
   second copy in this container's stdout is a leak waiting to happen.
5. The token is compared with `hmac.compare_digest`, and the container runs as a
   non-root user with no capabilities.
6. A tool-level refusal (bad path, bad arguments, timeout) is a JSON-RPC **result**
   with `isError: true`, not a protocol error: the call happened and failed,
   which is what the agent has to see.

## Tests

`test_server.py` starts the real server on an ephemeral port against a temporary
home:

```bash
cd services/tool-images/mcp-hub && python3 -m unittest test_server -v
```

It covers the handshake and session header, every tool, both escape routes, the
`shell=False` behaviour, the auth refusal (including an unset token), and the
transport refusals (`405`, `413`, `-32700`).

## Open work

- Server-sent events for server-initiated messages, if a client ever needs them.
  `hermes` does not: it opens the stream only when the server advertises it.
- `resources/*` and `prompts/*`. They would need a capability entry and a
  content model, not just a handler.
