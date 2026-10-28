"""Nestbox MCP hub — the sandbox's files and shell as MCP tools over HTTP.

The transport is JSON-RPC 2.0 over HTTP POST to `/mcp` (Streamable HTTP without
the optional server-to-client stream). That choice is the whole point: an HTTP
MCP server is reached through the egress proxy, so every `tools/call` appears in
the traffic log with a tool name, its (redacted) arguments, and a duration. A
stdio server would be invisible by construction (ADR 0026).

Three tools, three rules:

* ``read_file`` and ``write_file`` are confined to `NESTBOX_HOME` (default
  `/home/nestbox`). A path is resolved and then compared, so `..` and a symlink
  are both refused — checking the string would not be enough.
* ``run_command`` takes an argument vector as a list and never a shell string.
  `shell=False` is the whole defence against `; rm -rf /`, and it is not
  negotiable.
* the caller must present `NESTBOX_MCP_TOKEN`. Unset means refuse everything: a
  missing secret must never mean "no check".

Sessions: `initialize` assigns an `Mcp-Session-Id` and returns it in the response
header; a client that echoes it on later requests is honoured, and a client that
does not is still served. The header is what lets the proxy log a session id on
every call in the traffic view, so it is issued even though this server keeps no
state between requests.

The streaming half (`GET /mcp`, server-initiated messages) is deliberately not
implemented: a client that needs it gets `405` with `Allow: POST, DELETE` rather
than a half-open stream that never delivers. ``DELETE /mcp`` accepts a session
termination with `204`.
"""
from __future__ import annotations

import hmac
import json
import os
import subprocess
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

HOME = os.path.realpath(os.getenv("NESTBOX_HOME", "/home/nestbox"))
TOKEN = os.getenv("NESTBOX_MCP_TOKEN", "")
PORT = int(os.getenv("NESTBOX_MCP_PORT", "8080"))
MAX_BYTES = 256 * 1024
MAX_REQUEST_BYTES = 1024 * 1024
COMMAND_TIMEOUT = 60
PROTOCOL_VERSION = "2025-06-18"
SUPPORTED_PROTOCOLS = {"2025-06-18", "2025-03-26", "2024-11-05"}

TOOLS = [
    {
        "name": "read_file",
        "description": "Read a UTF-8 text file from the sandbox home.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string"}},
            "required": ["path"],
        },
    },
    {
        "name": "write_file",
        "description": "Write a UTF-8 text file inside the sandbox home.",
        "inputSchema": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
            "required": ["path", "content"],
        },
    },
    {
        "name": "run_command",
        "description": "Run a command in the sandbox home. An argument vector, never a shell string.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "argv": {"type": "array", "items": {"type": "string"}},
                "cwd": {"type": "string"},
            },
            "required": ["argv"],
        },
    },
]

# Methods that are a notification by definition: no id, no response body.
NOTIFICATIONS = {
    "notifications/initialized",
    "notifications/cancelled",
    "notifications/progress",
    "notifications/roots/list_changed",
}


class Refused(Exception):
    """A request the hub will not carry out, with a message safe to return."""


def resolve(path: str) -> str:
    """Resolve a path inside the home, or refuse it.

    `realpath` follows symlinks before the comparison, so a link to `/etc` cannot
    be used to escape. The home itself is allowed; a sibling with the same prefix
    (`/home/nestbox2`) is not.
    """
    candidate = os.path.realpath(os.path.join(HOME, path))
    if candidate != HOME and not candidate.startswith(HOME + os.sep):
        raise Refused(f"path is outside the sandbox home: {path}")
    return candidate


def tool_read_file(arguments: dict[str, Any]) -> str:
    target = resolve(str(arguments.get("path", "")))
    with open(target, encoding="utf-8", errors="replace") as handle:
        return handle.read(MAX_BYTES)


def tool_write_file(arguments: dict[str, Any]) -> str:
    target = resolve(str(arguments.get("path", "")))
    content = str(arguments.get("content", ""))
    if len(content.encode()) > MAX_BYTES:
        raise Refused(f"content exceeds {MAX_BYTES} bytes")
    os.makedirs(os.path.dirname(target) or HOME, exist_ok=True)
    with open(target, "w", encoding="utf-8") as handle:
        handle.write(content)
    return f"wrote {len(content.encode())} bytes to {target}"


def tool_run_command(arguments: dict[str, Any]) -> str:
    argv = arguments.get("argv")
    if not isinstance(argv, list) or not argv or not all(isinstance(item, str) for item in argv):
        raise Refused("argv must be a non-empty list of strings")
    cwd = resolve(str(arguments.get("cwd", ".")))
    if not os.path.isdir(cwd):
        cwd = HOME
    done = subprocess.run(
        argv,
        cwd=cwd,
        capture_output=True,
        text=True,
        timeout=COMMAND_TIMEOUT,
        check=False,
        shell=False,
    )
    output = (done.stdout + done.stderr)[-MAX_BYTES:]
    return f"exit {done.returncode}\n{output}"


HANDLERS = {
    "read_file": tool_read_file,
    "write_file": tool_write_file,
    "run_command": tool_run_command,
}


def handle_rpc(request: dict[str, Any]) -> tuple[dict[str, Any] | None, str | None]:
    """One JSON-RPC request to (response, new session id). Never raises."""
    method = request.get("method")
    identifier = request.get("id")
    params = request.get("params") or {}
    if not isinstance(params, dict):
        params = {}

    if method in NOTIFICATIONS:
        return None, None

    if method == "initialize":
        requested = params.get("protocolVersion")
        version = requested if requested in SUPPORTED_PROTOCOLS else PROTOCOL_VERSION
        answer = {
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "nestbox-hub", "version": "0.2.0"},
        }
        return ok(identifier, answer), uuid.uuid4().hex

    if method == "ping":
        return ok(identifier, {}), None

    if method == "tools/list":
        return ok(identifier, {"tools": TOOLS}), None

    if method == "tools/call":
        name = params.get("name")
        handler = HANDLERS.get(str(name))
        if handler is None:
            return error(identifier, -32602, f"unknown tool: {name}"), None
        arguments = params.get("arguments")
        if not isinstance(arguments, dict):
            arguments = {}
        try:
            text = handler(arguments)
            return ok(
                identifier, {"content": [{"type": "text", "text": text}], "isError": False}
            ), None
        except Refused as exc:
            return tool_error(identifier, str(exc)), None
        except subprocess.TimeoutExpired:
            return tool_error(identifier, "command timed out"), None
        except OSError as exc:
            return tool_error(identifier, f"failed: {exc}"), None

    # `resources/*` and `prompts/*` are not advertised in `capabilities`, so a
    # client that asks for them gets the standard "method not found" instead of
    # an empty list that would look like a server without content.
    return error(identifier, -32601, f"method not found: {method}"), None


def ok(identifier: Any, result: Any) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": identifier, "result": result}


def tool_error(identifier: Any, message: str) -> dict[str, Any]:
    """A tool that ran and failed: an MCP result with `isError`, not a protocol error."""
    return ok(identifier, {"content": [{"type": "text", "text": message}], "isError": True})


def error(identifier: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": identifier, "error": {"code": code, "message": message}}


class Handler(BaseHTTPRequestHandler):
    server_version = "nestbox-mcp-hub/0.2"

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002 - signature fixed by stdlib
        # One line per request, no bodies: the proxy logs the bodies, and a
        # second copy in this container's stdout is a secret leak waiting to
        # happen.
        print(f"{self.address_string()} {format % args}", flush=True)

    def _authorised(self) -> bool:
        if not TOKEN:
            return False
        header = self.headers.get("authorization", "")
        prefix = "Bearer "
        if not header.startswith(prefix):
            return False
        return hmac.compare_digest(header[len(prefix):], TOKEN)

    def _reply(
        self,
        status: int,
        payload: dict[str, Any] | None,
        session_id: str | None = None,
    ) -> None:
        body = json.dumps(payload).encode() if payload is not None else b""
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if session_id:
            self.send_header("Mcp-Session-Id", session_id)
        self.end_headers()
        if body:
            self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        if self.path == "/healthz":
            self._reply(200, {"status": "ok", "tools": len(TOOLS)})
            return
        if self.path == "/mcp":
            # The server-to-client stream is not offered. A 405 with the allowed
            # methods is the honest answer; a stream that never delivers would
            # look like a healthy server with a broken tool list.
            self.send_response(405)
            self.send_header("Allow", "POST, DELETE")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self._reply(404, {"error": "not found"})

    def do_DELETE(self) -> None:  # noqa: N802 - stdlib naming
        if self.path != "/mcp":
            self._reply(404, {"error": "not found"})
            return
        if not self._authorised():
            self._reply(401, {"error": "bearer token required"})
            return
        # Sessions hold no state, so terminating one is a no-op with the
        # status the specification asks for.
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        if self.path != "/mcp":
            self._reply(404, {"error": "not found"})
            return
        if not self._authorised():
            self._reply(401, {"error": "bearer token required"})
            return
        try:
            length = int(self.headers.get("content-length") or 0)
        except ValueError:
            self._reply(400, error(None, -32700, "invalid content-length"))
            return
        if length <= 0 or length > MAX_REQUEST_BYTES:
            self._reply(413, {"error": f"body must be between 1 byte and {MAX_REQUEST_BYTES}"})
            return
        try:
            request = json.loads(self.rfile.read(length))
        except (ValueError, OSError):
            self._reply(400, error(None, -32700, "parse error"))
            return
        if not isinstance(request, dict):
            self._reply(400, error(None, -32600, "expected one JSON-RPC request"))
            return
        answer, session_id = handle_rpc(request)
        if answer is None:
            # A notification has no response body.
            self.send_response(202)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self._reply(200, answer, session_id)


def main() -> None:
    if not TOKEN:
        print("NESTBOX_MCP_TOKEN is unset: every request will be refused", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
