"""Contracts for the media MCP bridge: the tool list, arguments, and forwarding."""
from __future__ import annotations

import base64
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))

import server  # noqa: E402


def rpc(method: str, params: dict | None = None, identifier: int = 1, sandbox: str = "nest"):
    return server.handle_rpc(
        {"jsonrpc": "2.0", "id": identifier, "method": method, "params": params or {}},
        sandbox,
    )[0]


def test_initialize_names_the_three_tools() -> None:
    answer = rpc("initialize", {"protocolVersion": "2025-06-18"})
    assert answer["result"]["serverInfo"]["name"] == "nestbox-media"


def test_tools_list_offers_image_video_and_music_only() -> None:
    names = {tool["name"] for tool in rpc("tools/list")["result"]["tools"]}
    assert names == {"render_image", "render_video", "render_music"}


def test_a_tool_call_forwards_the_prompt_and_kind(monkeypatch) -> None:
    seen: dict = {}

    def fake_control(path, payload=None):
        if payload is not None:
            seen.update(payload)
            return 200, json.dumps({"id": "r1", "status": "done", "model": "flux", "provider_name": "p"}).encode(), ""
        return 200, b"bytes", "image/png"

    monkeypatch.setattr(server, "control", fake_control)
    answer = rpc("tools/call", {"name": "render_image", "arguments": {"prompt": "a cat", "size": "512x512"}})
    assert answer["result"]["isError"] is False
    assert seen["kind"] == "image" and seen["prompt"] == "a cat" and seen["size"] == "512x512"
    assert answer["result"]["content"][1]["mimeType"] == "image/png"


def test_a_video_is_returned_as_a_resource(monkeypatch) -> None:
    def fake_control(path, payload=None):
        if payload is not None:
            return 200, json.dumps({"id": "r2", "status": "done", "model": "veo"}).encode(), ""
        return 200, b"clip", "video/mp4"

    monkeypatch.setattr(server, "control", fake_control)
    answer = rpc("tools/call", {"name": "render_video", "arguments": {"prompt": "waves", "duration_seconds": 4}})
    resource = answer["result"]["content"][1]
    assert resource["type"] == "resource"
    assert resource["resource"]["mimeType"] == "video/mp4"
    assert base64.b64decode(resource["resource"]["blob"]) == b"clip"


def test_an_empty_prompt_is_refused_without_calling_the_control_plane(monkeypatch) -> None:
    def fail(*_args, **_kwargs):
        raise AssertionError("must not reach the control plane")

    monkeypatch.setattr(server, "control", fail)
    answer = rpc("tools/call", {"name": "render_image", "arguments": {"prompt": "   "}})
    assert answer["result"]["isError"] is True


def test_a_failed_render_is_an_error_result_not_an_empty_asset(monkeypatch) -> None:
    monkeypatch.setattr(
        server, "control",
        lambda path, payload=None: (200, json.dumps({"id": "r3", "status": "error", "error": "quota"}).encode(), ""),
    )
    answer = rpc("tools/call", {"name": "render_music", "arguments": {"prompt": "jazz"}})
    assert answer["result"]["isError"] is True
    assert "quota" in answer["result"]["content"][0]["text"]


def test_a_missing_sandbox_is_refused() -> None:
    answer = rpc("tools/call", {"name": "render_image", "arguments": {"prompt": "x"}}, sandbox="")
    assert answer["result"]["isError"] is True


def test_an_unknown_tool_is_a_protocol_error() -> None:
    answer = rpc("tools/call", {"name": "delete_everything", "arguments": {}})
    assert answer["error"]["code"] == -32602


def test_a_missing_token_refuses_every_connection(monkeypatch) -> None:
    monkeypatch.setattr(server, "TOKEN", "")
    handler = server.Handler.__new__(server.Handler)
    handler.headers = {"authorization": "Bearer anything"}
    assert handler._authorised() is False


def test_the_sandbox_can_come_from_the_path_when_a_proxy_drops_the_header() -> None:
    handler = server.Handler.__new__(server.Handler)
    handler.path = "/mcp/claude-01"
    assert handler._sandbox_from_path() == "claude-01"
    handler.path = "/mcp"
    assert handler._sandbox_from_path() == ""
    handler.path = "/mcp/claude-01?x=1"
    assert handler._sandbox_from_path() == "claude-01"


def test_a_render_can_be_served_from_the_path_alone(monkeypatch) -> None:
    seen: dict = {}

    def fake_control(path, payload=None):
        if payload is not None:
            seen.update(payload)
            return 200, json.dumps({"id": "r9", "status": "done", "model": "m"}).encode(), ""
        return 200, b"img", "image/png"

    monkeypatch.setattr(server, "control", fake_control)
    answer = rpc("tools/call", {"name": "render_image", "arguments": {"prompt": "x"}}, sandbox="from-path")
    assert answer["result"]["isError"] is False
    assert seen["sandbox"] == "from-path"
