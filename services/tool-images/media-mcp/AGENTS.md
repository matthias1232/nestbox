# media-mcp — rendering tools

## Purpose

The MCP server an agent calls for `render_image`, `render_video` and
`render_music`. It is a thin bridge: it forwards the prompt to the control plane
and returns the bytes as MCP content.

## Dependencies

- Standard library only. The image has no package install step.
- One outbound dependency: the control API (`NESTBOX_CONTROL_URL`).

## Rules

1. **No provider credential.** The control plane resolves it. This server holds
   only its own bridge token.
2. **The model is not an argument.** The tool takes a prompt (and an optional
   size/duration). Which model answers is an operator decision stored in the
   control plane.
3. **The sandbox comes from the connection.** `X-Nestbox-Sandbox` is written by
   the control plane into the agent's MCP config; a tool call cannot name another
   sandbox.
4. **Fail closed.** No token means every request is refused, and an unreachable
   control plane becomes an `isError` tool result rather than an invented asset.
5. **No log of prompts.** The handler logs the request line only; the prompt is
   already stored, bounded, on the render row the control plane wrote.

## Files

| File | Responsibility |
|---|---|
| `server.py` | JSON-RPC over HTTP, the three tools, the bridge call |
| `test_server.py` | Tool list, argument validation, forwarding, auth |
