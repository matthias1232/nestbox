"""Nestbox MCP hub — the sandbox's files and shell as MCP tools over HTTP.

Skeleton with real tools. The transport is JSON-RPC 2.0 over HTTP POST to `/mcp`,
which is what Hermes and every HTTP MCP client speak; the streaming variant
(server-sent events for server-initiated messages) is not implemented, and a
client that requires it gets a clear 400 rather than a half-open stream.

Three tools, three rules:

* ``read_file`` and ``write_file`` are confined to `NESTBOX_HOME` (default
  `/home/nestbox`). A path is resolved and then compared, so `..` and a symlink
  are both refused — checking the string would not be enough.
* ``run_command`` takes an argument vector as a list and never a shell string.
  `shell=False` is the whole defence against `; rm -rf /`, and it is not
  negotiable.
* the caller must present `NESTBOX_MCP_TOKEN`. Unset means refuse everything: a
  missing secret must never mean "no check".

Package `feat/mcp-registry` owns this file and replaces the transport with a
complete MCP implementation; the tool behaviour and its tests are worth keeping.
"""
from __future__ import annotations

import json
import os
import subprocess
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

HOME = os.path.realpath(os.getenv("NESTBOX_HOME", "/home/nestbox"))
TOKEN = os.getenv("NESTBOX_MCP_TOKEN", "")
PORT = int(os.getenv("NESTBOX_MCP_PORT", "8080"))
MAX_BYTES = 256 * 1024
COMMAND_TIMEOUT = 60

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


def handle_rpc(request: dict[str, Any]) -> dict[str, Any]:
    """One JSON-RPC request to one response. Never raises."""
    method = request.get("method")
    identifier = request.get("id")
    params = request.get("params") or {}

    if method == "initialize":
        result: Any = {
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {"name": "nestbox-hub", "version": "0.1.0"},
        }
    elif method == "tools/list":
        result = {"tools": TOOLS}
    elif method == "tools/call":
        name = params.get("name")
        handler = HANDLERS.get(str(name))
        if handler is None:
            return error(identifier, -32602, f"unknown tool: {name}")
        try:
            text = handler(params.get("arguments") or {})
            result = {"content": [{"type": "text", "text": text}], "isError": False}
        except Refused as exc:
            result = {"content": [{"type": "text", "text": str(exc)}], "isError": True}
        except subprocess.TimeoutExpired:
            result = {"content": [{"type": "text", "text": "command timed out"}], "isError": True}
        except OSError as exc:
            result = {"content": [{"type": "text", "text": f"failed: {exc}"}], "isError": True}
    elif method in ("notifications/initialized", "ping"):
        return {}
    else:
        return error(identifier, -32601, f"method not found: {method}")

    return {"jsonrpc": "2.0", "id": identifier, "result": result}


def error(identifier: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": identifier, "error": {"code": code, "message": message}}


class Handler(BaseHTTPRequestHandler):
    server_version = "nestbox-mcp-hub/0.1"

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002 - signature fixed by stdlib
        # One line per request, no bodies: the proxy logs the bodies, and a
        # second copy in this container's stdout is a secret leak waiting to
        # happen.
        print(f"{self.address_string()} {format % args}", flush=True)

    def _authorised(self) -> bool:
        if not TOKEN:
            return False
        header = self.headers.get("authorization", "")
        return header == f"Bearer {TOKEN}"

    def _reply(self, status: int, payload: dict[str, Any] | None) -> None:
        body = json.dumps(payload).encode() if payload is not None else b""
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        if body:
            self.wfile.write(body)

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        if self.path == "/healthz":
            self._reply(200, {"status": "ok", "home": HOME, "tools": len(TOOLS)})
            return
        # A streaming GET is the one part of the transport that is not here yet.
        self._reply(400, {"error": "use POST /mcp; this server does not stream yet"})

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        if self.path != "/mcp":
            self._reply(404, {"error": "not found"})
            return
        if not self._authorised():
            self._reply(401, {"error": "bearer token required"})
            return
        length = int(self.headers.get("content-length") or 0)
        if length <= 0 or length > 1024 * 1024:
            self._reply(413, {"error": "body must be between 1 byte and 1 MiB"})
            return
        try:
            request = json.loads(self.rfile.read(length))
        except (ValueError, OSError):
            self._reply(400, error(None, -32700, "parse error"))
            return
        if not isinstance(request, dict):
            self._reply(400, error(None, -32600, "expected one JSON-RPC request"))
            return
        answer = handle_rpc(request)
        if not answer:
            # A notification has no response.
            self._reply(202, None)
            return
        self._reply(200, answer)


def main() -> None:
    if not TOKEN:
        print("NESTBOX_MCP_TOKEN is unset: every request will be refused", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
