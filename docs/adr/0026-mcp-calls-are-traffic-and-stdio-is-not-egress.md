# ADR 0026 — An MCP call over HTTP is traffic; a stdio server is not egress

Status: Accepted
Date: 2026-10-27

## Context

The console should show which MCP tools an agent called, with what arguments. That
is only possible for servers the proxy can see. MCP has two transports that matter
here: `stdio`, where the server is a child process inside the sandbox, and
`http`/`sse`, where the server is somewhere else.

An MCP tab that shows an empty tool column for a `stdio` server is technically
correct and completely misleading: it looks like the agent did nothing, when in
fact the agent did plenty that the platform cannot observe.

## Decision

**Transport is a first-class field, and the console says what it means.**
`mcp_registry.transport` decides visibility. An `http`/`sse` server is reached
through the egress proxy, so every call is a `traffic` row with `protocol=mcp`, a
tool name, the (redacted) arguments, the session id, and a duration.
`McpTransport::visible_to_proxy()` is the single place that answers the question,
and the console renders "not visible: runs inside the sandbox" instead of a blank.

**The proxy recognises MCP by shape, not by port.** JSON-RPC 2.0 with a known
method family (`initialize`, `tools/*`, `resources/*`, `prompts/*`, …). A body that
does not match is "some JSON" and is left alone; a wrong tool name in a log is
worse than no tool name.

**We ship our own HTTP MCP server (`nestbox-hub`) so that at least one useful
server is visible.** It exposes the sandbox's own files and shell as tools. The
point is not the tools; the point is that the agent's actions become observable
facts with names.

**Discovery is egress too.** `POST /sandboxes/{id}/mcp/servers/{key}/discover`
goes through the same policy check. A discovery path that bypassed the rules would
be an egress hole with a friendly name.

**Credentials are not in the registry.** A remote server's token lives in the
agent's own home inside the container; `mcp_registry` holds the definition and the
names of the environment variables it needs, never a value.

## Alternatives rejected

**Proxy stdio servers through the hub so everything is visible.** It would require
the hub to spawn arbitrary processes on behalf of the agent, which is a sandbox
escape with extra steps.

**Only support HTTP servers.** Simpler and honest, but `filesystem` and
`playwright` are stdio-only and are the two most useful local tools.

**Guess the tool from the URL.** An MCP endpoint is one URL; the tool name is in
the body. Guessing would produce confident wrong data.

## Consequences

- The MCP tab has two kinds of rows: calls (facts) and servers marked as not
  visible (also a fact).
- The proxy buffers up to 64 KiB of a request body to read the JSON-RPC envelope;
  a larger body is logged as metadata only.
- `mcp_tool_calls` keeps 7 days. The full body, if captured, lives in
  `traffic_bodies` for 24 h.

## What would invalidate this

A standard, authenticated MCP gateway protocol that carries the tool name in a
header would make body inspection unnecessary. Until then the body is where the
fact is.
