"""Nestbox media MCP server — image, video and music rendering as MCP tools.

A text model cannot draw. This server is how an agent in a sandbox asks for a
picture, a clip or a track without ever holding a provider key or choosing a
model: it forwards the request to the control plane, which resolves the model
from the environment's default or the sandbox's own choice and calls the
provider with the credential that lives there.

Three rules, and each is why a piece of the code looks the way it does:

* **The sandbox is named by its config, not by the tool arguments.** The agent's
  MCP entry carries ``X-Nestbox-Sandbox``; a tool call cannot name another
  sandbox, so a prompt cannot be used to render on someone else's quota.
* **The model is never in the request.** ``render_image`` takes a prompt and a
  size, nothing else. What answers is an operator decision, not an agent one.
* **Transport is HTTP on purpose.** The call crosses the egress proxy, so every
  tool call appears in the traffic log with its (redacted) arguments — the same
  reason the Nestbox hub is HTTP (ADR 0026).

Standard library only: this image has no package manager step, so a rebuild
cannot be affected by a registry outage.
"""
from __future__ import annotations

import base64
import hmac
import json
import os
import urllib.error
import urllib.request
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

TOKEN = os.getenv("NESTBOX_MCP_TOKEN", "")
BRIDGE_TOKEN = os.getenv("NESTBOX_MEDIA_BRIDGE_TOKEN", "")
CONTROL_URL = os.getenv("NESTBOX_CONTROL_URL", "http://control-api:8000").rstrip("/")
PORT = int(os.getenv("NESTBOX_MCP_PORT", "8080"))
PROTOCOL_VERSION = "2025-06-18"
SUPPORTED_PROTOCOLS = {"2025-06-18", "2025-03-26", "2024-11-05"}
# A large asset is reported by reference rather than pushed through a JSON-RPC
# body: an agent that receives 40 MB of base64 has spent its context on bytes.
MAX_INLINE_ASSET = 8 * 1024 * 1024
REQUEST_TIMEOUT = 200

TOOLS = [
    {
        "name": "render_image",
        "description": (
            "Render an image from a text prompt using the image model this sandbox is configured "
            "with. Use it when the answer should be a picture (diagram, illustration, photo-like "
            "scene) rather than text. The model is chosen by the operator, not by this call."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "What the image should show."},
                "size": {"type": "string", "description": "Optional size such as 1024x1024."},
            },
            "required": ["prompt"],
        },
    },
    {
        "name": "render_video",
        "description": (
            "Render a short video clip from a text prompt using the video model this sandbox is "
            "configured with. Use it for motion, a sequence of shots, or an animated explainer."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "What the clip should show."},
                "duration_seconds": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 60,
                    "description": "Optional clip length; the provider may clamp it.",
                },
            },
            "required": ["prompt"],
        },
    },
    {
        "name": "render_music",
        "description": (
            "Render a music track or sound bed from a text prompt using the music model this sandbox "
            "is configured with. Use it for a jingle, a loop, or background audio."
        ),
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": {"type": "string", "description": "Style, instruments, mood, tempo."},
                "duration_seconds": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 60,
                    "description": "Optional length; the provider may clamp it.",
                },
            },
            "required": ["prompt"],
        },
    },
]

KIND_BY_TOOL = {"render_image": "image", "render_video": "video", "render_music": "music"}

NOTIFICATIONS = {
    "notifications/initialized",
    "notifications/cancelled",
    "notifications/progress",
    "notifications/roots/list_changed",
}


class Refused(Exception):
    """A request this server will not carry out, with a message safe to return."""


def control(path: str, payload: dict[str, Any] | None = None) -> tuple[int, bytes, str]:
    """One request to the control plane, returning (status, body, content type)."""
    if not BRIDGE_TOKEN:
        raise Refused("This server has no control-plane token; rendering is unavailable.")
    url = f"{CONTROL_URL}{path}"
    body = json.dumps(payload).encode() if payload is not None else None
    request = urllib.request.Request(
        url,
        data=body,
        method="POST" if payload is not None else "GET",
        headers={
            "Authorization": f"Bearer {BRIDGE_TOKEN}",
            "Content-Type": "application/json",
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT) as response:  # noqa: S310
            return response.status, response.read(), response.headers.get("content-type", "")
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", "replace")[:500]
        raise Refused(f"the control plane refused the render ({error.code}): {detail}") from error
    except (urllib.error.URLError, OSError) as error:
        raise Refused(f"the control plane is unreachable: {error}") from error


def render(arguments: dict[str, Any], kind: str, sandbox: str) -> list[dict[str, Any]]:
    """Forward one tool call and turn the result into MCP content."""
    prompt = arguments.get("prompt")
    if not isinstance(prompt, str) or not prompt.strip():
        raise Refused("a render needs a non-empty prompt")
    body: dict[str, Any] = {"sandbox": sandbox, "kind": kind, "prompt": prompt}
    size = arguments.get("size")
    if isinstance(size, str) and size:
        body["size"] = size
    duration = arguments.get("duration_seconds")
    if isinstance(duration, int) and not isinstance(duration, bool):
        body["duration_seconds"] = max(1, min(duration, 60))

    _, raw, _ = control("/api/v1/internal/media/render", body)
    try:
        render_row = json.loads(raw)
    except ValueError as error:
        raise Refused("the control plane answered with an unreadable render record") from error
    if render_row.get("status") != "done":
        raise Refused(f"the render failed: {render_row.get('error') or 'no reason reported'}")

    summary = (
        f"{kind} rendered with {render_row.get('model')} via {render_row.get('provider_name') or 'the configured provider'} "
        f"in {render_row.get('duration_ms') or 0} ms. render id: {render_row.get('id')}."
    )
    content: list[dict[str, Any]] = [{"type": "text", "text": summary}]
    _, asset, mime = control(f"/api/v1/internal/media/renders/{render_row['id']}/asset")
    if len(asset) > MAX_INLINE_ASSET:
        content.append(
            {
                "type": "text",
                "text": (
                    f"The asset is {len(asset)} bytes, above the {MAX_INLINE_ASSET}-byte inline limit. "
                    f"Open it in Nestbox under this sandbox's media renders (id {render_row['id']})."
                ),
            }
        )
        return content
    encoded = base64.b64encode(asset).decode()
    if kind == "image":
        content.append({"type": "image", "data": encoded, "mimeType": mime or "image/png"})
    else:
        # Video and audio are not an MCP content type every client renders, so
        # they travel as an embedded resource with the right mime type.
        content.append(
            {
                "type": "resource",
                "resource": {
                    "uri": f"nestbox-media://renders/{render_row['id']}",
                    "mimeType": mime or ("video/mp4" if kind == "video" else "audio/mpeg"),
                    "blob": encoded,
                },
            }
        )
    return content


def ok(identifier: Any, result: Any) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": identifier, "result": result}


def tool_error(identifier: Any, message: str) -> dict[str, Any]:
    return ok(identifier, {"content": [{"type": "text", "text": message}], "isError": True})


def error(identifier: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": identifier, "error": {"code": code, "message": message}}


def handle_rpc(request: dict[str, Any], sandbox: str) -> tuple[dict[str, Any] | None, str | None]:
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
            "serverInfo": {"name": "nestbox-media", "version": "1.0.0"},
        }
        return ok(identifier, answer), uuid.uuid4().hex

    if method == "ping":
        return ok(identifier, {}), None

    if method == "tools/list":
        return ok(identifier, {"tools": TOOLS}), None

    if method == "tools/call":
        name = str(params.get("name"))
        kind = KIND_BY_TOOL.get(name)
        if kind is None:
            return error(identifier, -32602, f"unknown tool: {name}"), None
        arguments = params.get("arguments")
        if not isinstance(arguments, dict):
            arguments = {}
        if not sandbox:
            return tool_error(identifier, "this connection does not name a sandbox"), None
        try:
            return ok(identifier, {"content": render(arguments, kind, sandbox), "isError": False}), None
        except Refused as exc:
            return tool_error(identifier, str(exc)), None
        except (OSError, ValueError) as exc:
            return tool_error(identifier, f"failed: {exc}"), None

    return error(identifier, -32601, f"method not found: {method}"), None


class Handler(BaseHTTPRequestHandler):
    server_version = "nestbox-media-mcp/1.0"

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002 - stdlib signature
        print(f"{self.address_string()} {format % args}", flush=True)

    def _authorised(self) -> bool:
        if not TOKEN:
            return False
        header = self.headers.get("authorization", "")
        if not header.startswith("Bearer "):
            return False
        return hmac.compare_digest(header[len("Bearer "):], TOKEN)

    def _sandbox(self) -> str:
        return (self.headers.get("x-nestbox-sandbox") or "").strip()

    def _reply(self, status: int, payload: dict[str, Any] | None, session_id: str | None = None) -> None:
        body = json.dumps(payload).encode() if payload is not None else b""
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if session_id:
            self.send_header("Mcp-Session-Id", session_id)
        self.end_headers()
        if body:
            self.wfile.write(body)

    def _sandbox_from_path(self) -> str:
        """The sandbox named in `/mcp/<sandbox>`, if the path carries one.

        The path is used because an egress proxy may drop a custom header: a
        request through it arrives without `X-Nestbox-Sandbox`, and a server that
        only read the header would refuse every call while logging nothing wrong.
        The header stays as a fallback for a direct call.
        """
        path = self.path.split("?", 1)[0].rstrip("/")
        if path.startswith("/mcp/"):
            name = path[len("/mcp/"):]
            if name and "/" not in name:
                return name
        return ""

    def _is_mcp(self) -> bool:
        path = self.path.split("?", 1)[0].rstrip("/")
        return path == "/mcp" or path.startswith("/mcp/")

    def do_GET(self) -> None:  # noqa: N802 - stdlib naming
        if self.path == "/healthz":
            self._reply(200, {"status": "ok", "tools": len(TOOLS), "bridge": bool(BRIDGE_TOKEN)})
            return
        if self._is_mcp():
            self.send_response(405)
            self.send_header("Allow", "POST, DELETE")
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self._reply(404, {"error": "not found"})

    def do_DELETE(self) -> None:  # noqa: N802 - stdlib naming
        if not self._is_mcp():
            self._reply(404, {"error": "not found"})
            return
        if not self._authorised():
            self._reply(401, {"error": "bearer token required"})
            return
        self.send_response(204)
        self.send_header("Content-Length", "0")
        self.end_headers()

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        if not self._is_mcp():
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
        answer, session_id = handle_rpc(request, self._sandbox_from_path() or self._sandbox())
        if answer is None:
            self.send_response(202)
            self.send_header("Content-Length", "0")
            self.end_headers()
            return
        self._reply(200, answer, session_id)


def main() -> None:
    if not TOKEN:
        print("NESTBOX_MCP_TOKEN is unset: every request will be refused", flush=True)
    if not BRIDGE_TOKEN:
        print("NESTBOX_MEDIA_BRIDGE_TOKEN is unset: renders will be refused", flush=True)
    ThreadingHTTPServer(("0.0.0.0", PORT), Handler).serve_forever()


if __name__ == "__main__":
    main()
