"""Tests for the Nestbox accounting proxy.

Run with `python3 -m unittest -v` in this folder, or
`python3 -m unittest discover -s services/accounting-proxy -p 'test_*.py'` from
the repository root.

Three real servers are started on ephemeral ports: two stand in for the model
providers, one stands in for the control API, and the proxy under test sits
between them. Nothing is mocked inside the proxy, so what is exercised is the
process that ships.

Accounting rows are posted *after* the client has its answer (that is the
design), so a row can still be in flight when the next test starts. Every test
therefore names its own row — by model, endpoint, or privacy action — instead of
assuming the first row it sees is its own.
"""
from __future__ import annotations

import hashlib
import http.client
import json
import threading
import time
import unittest
from dataclasses import replace
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import server as proxy

ACCOUNTING_TOKEN = "node-token"
SANDBOX = "sbx-0001"
AGENT = "agent-0001"


class _Quiet(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: object) -> None:  # noqa: A002
        pass


def _parsed(body: bytes) -> dict:
    try:
        value = json.loads(body) if body else {}
    except (ValueError, UnicodeDecodeError):
        return {}
    return value if isinstance(value, dict) else {}


class UpstreamHandler(_Quiet):
    """A provider stand-in: records what arrived, answers per path.

    The response names the model the request asked for, so a test can identify
    its own accounting row by model.
    """

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        length = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(length) if length else b""
        self.server.requests.append(  # type: ignore[attr-defined]
            {
                "method": self.command,
                "path": self.path,
                "headers": {name.lower(): value for name, value in self.headers.items()},
                "body": body,
            }
        )
        request = _parsed(body)
        model = request.get("model") if isinstance(request.get("model"), str) else "stub-model"
        path = self.path.split("?")[0]
        if path == "/v1/messages":
            self._json(
                {
                    "id": "msg_1",
                    "model": model,
                    "content": [{"type": "text", "text": "hi"}],
                    "usage": {
                        "input_tokens": 5,
                        "output_tokens": 9,
                        "cache_read_input_tokens": 2,
                    },
                }
            )
            return
        if path == "/v1/chat/completions":
            if request.get("stream") is True:
                self._stream()
                return
            self._json(
                {
                    "id": "chatcmpl-1",
                    "model": model,
                    "choices": [
                        {"index": 0, "message": {"role": "assistant", "content": "hi"}}
                    ],
                    "usage": {
                        "prompt_tokens": 11,
                        "completion_tokens": 7,
                        "total_tokens": 18,
                        "prompt_tokens_details": {"cached_tokens": 3},
                    },
                }
            )
            return
        self._json({"model": model, "ok": True})

    def _json(self, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _stream(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Connection", "close")
        self.end_headers()
        for event in (b'data: {"chunk":1}\n\n', b'data: {"chunk":2}\n\n'):
            self.wfile.write(event)
            self.wfile.flush()
        self.close_connection = True


class ControlHandler(_Quiet):
    """The control API stand-in: records calls, answers inspect from the text."""

    def do_POST(self) -> None:  # noqa: N802 - stdlib naming
        length = int(self.headers.get("content-length") or 0)
        raw = self.rfile.read(length) if length else b""
        payload = json.loads(raw) if raw else {}
        self.server.calls.append(  # type: ignore[attr-defined]
            {
                "path": self.path,
                "authorization": self.headers.get("authorization"),
                "payload": payload,
            }
        )
        if self.path == "/api/v1/internal/privacy/inspect":
            status = self.server.inspect_status  # type: ignore[attr-defined]
            if status != 200:
                self._status(status)
                return
            decide = self.server.inspect  # type: ignore[attr-defined]
            self._json(decide(payload.get("content", "")))
            return
        self._status(204)

    def _json(self, payload: dict) -> None:
        body = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _status(self, status: int) -> None:
        self.send_response(status)
        self.send_header("Content-Length", "0")
        self.end_headers()


class Stub:
    """One stub server on an ephemeral port, with what it saw."""

    def __init__(self, handler: type[BaseHTTPRequestHandler]) -> None:
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.server.daemon_threads = True
        self.server.requests = []  # type: ignore[attr-defined]
        self.server.calls = []  # type: ignore[attr-defined]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    @property
    def base(self) -> str:
        host, port = self.server.server_address[0], self.server.server_address[1]
        return f"http://{host}:{port}"

    @property
    def requests(self) -> list[dict]:
        return self.server.requests  # type: ignore[attr-defined]

    @property
    def calls(self) -> list[dict]:
        return self.server.calls  # type: ignore[attr-defined]

    def stop(self) -> None:
        self.server.shutdown()
        self.server.server_close()


class ProxyTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.anthropic = Stub(UpstreamHandler)
        cls.openai = Stub(UpstreamHandler)
        cls.default = Stub(UpstreamHandler)
        cls.control = Stub(ControlHandler)
        cls.control.server.inspect_status = 200  # type: ignore[attr-defined]
        cls.control.server.inspect = inspect_decision  # type: ignore[attr-defined]

        cls.saved = proxy.CONFIG
        proxy.CONFIG = cls.config()
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), proxy.Handler)
        cls.server.daemon_threads = True
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def config(cls) -> proxy.Config:
        return proxy.Config(
            anthropic_upstream=cls.anthropic.base,
            openai_upstream=cls.openai.base,
            default_upstream=cls.default.base,
            control_api=cls.control.base,
            accounting_token=ACCOUNTING_TOKEN,
            sandbox=SANDBOX,
            agent_instance=AGENT,
            privacy_fail_closed=False,
            port=0,
            upstream_timeout=10.0,
            control_timeout=5.0,
        )

    @classmethod
    def tearDownClass(cls) -> None:
        cls.server.shutdown()
        cls.server.server_close()
        proxy.CONFIG = cls.saved
        cls.anthropic.stop()
        cls.openai.stop()
        cls.default.stop()
        cls.control.stop()

    def setUp(self) -> None:
        for stub in (self.anthropic, self.openai, self.default):
            stub.requests.clear()
        self.control.calls.clear()
        self.control.server.inspect_status = 200  # type: ignore[attr-defined]
        proxy.CONFIG = self.config()

    # -- helpers ------------------------------------------------------------

    def post(
        self,
        path: str,
        payload: object,
        headers: dict[str, str] | None = None,
        raw: bytes | None = None,
    ) -> tuple[int, dict[str, str], bytes]:
        body = raw if raw is not None else json.dumps(payload).encode()
        sent = {"Content-Type": "application/json"}
        sent.update(headers or {})
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=15)
        connection.request("POST", path, body=body, headers=sent)
        response = connection.getresponse()
        data = response.read()
        result = (response.status, dict(response.getheaders()), data)
        connection.close()
        return result

    def get(self, path: str) -> tuple[int, bytes]:
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=15)
        connection.request("GET", path)
        response = connection.getresponse()
        data = response.read()
        connection.close()
        return response.status, data

    def rows(self) -> list[dict]:
        return [
            call["payload"]
            for call in self.control.calls
            if call["path"] == "/api/v1/internal/accounting/record"
        ]

    def inspects(self) -> list[dict]:
        return [
            call["payload"]
            for call in self.control.calls
            if call["path"] == "/api/v1/internal/privacy/inspect"
        ]

    def wait_row(self, **matches: object) -> dict:
        """Wait for the accounting row this test caused.

        A row is posted after the answer, so it lands a moment later — and a
        previous test's row may still be in flight. `matches` names the row by
        fields that only this test can produce.
        """
        deadline = time.monotonic() + 5.0
        while time.monotonic() < deadline:
            for row in self.rows():
                if all(row.get(key) == value for key, value in matches.items()):
                    return row
            time.sleep(0.01)
        self.fail(f"no accounting row matched {matches}; rows: {self.rows()}")

    # -- liveness -----------------------------------------------------------

    def test_healthz_answers_without_a_token(self) -> None:
        status, body = self.get("/healthz")
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["status"], "ok")

    # -- routing ------------------------------------------------------------

    def test_an_anthropic_path_goes_to_the_anthropic_upstream(self) -> None:
        status, _, _ = self.post(
            "/v1/messages",
            {"model": "claude-routing", "messages": [{"role": "user", "content": "hi"}]},
        )
        self.assertEqual(status, 200)
        self.assertEqual(len(self.anthropic.requests), 1)
        self.assertEqual(self.anthropic.requests[0]["path"], "/v1/messages")
        self.assertEqual(self.openai.requests, [])
        row = self.wait_row(model="claude-routing")
        self.assertEqual(row["provider"], "anthropic")

    def test_an_openai_path_goes_to_the_openai_upstream(self) -> None:
        self.post(
            "/v1/chat/completions",
            {"model": "gpt-routing", "messages": [{"role": "user", "content": "hi"}]},
        )
        self.assertEqual([r["path"] for r in self.openai.requests], ["/v1/chat/completions"])
        self.assertEqual(self.anthropic.requests, [])
        self.assertEqual(self.wait_row(model="gpt-routing")["provider"], "openai")

    def test_an_embeddings_path_goes_to_the_openai_upstream(self) -> None:
        self.post("/v1/embeddings", {"model": "text-embedding-3-small", "input": "hi"})
        self.assertEqual([r["path"] for r in self.openai.requests], ["/v1/embeddings"])

    def test_a_gemini_path_goes_to_the_default_upstream(self) -> None:
        path = "/v1beta/models/gemini-2.0-flash:generateContent"
        self.post(path, {"contents": [{"parts": [{"text": "hi"}]}]})
        self.assertEqual([r["path"] for r in self.default.requests], [path])
        row = self.wait_row(endpoint=path)
        self.assertEqual(row["provider"], "gemini")

    def test_the_upstream_header_overrides_the_path(self) -> None:
        self.post(
            "/v1/chat/completions",
            {"model": "gpt-override", "messages": [{"role": "user", "content": "hi"}]},
            headers={"X-Nestbox-Upstream": self.default.base},
        )
        self.assertEqual(len(self.default.requests), 1)
        self.assertEqual(self.openai.requests, [])
        # The override moves the request; the dialect still describes the body.
        self.assertEqual(self.wait_row(model="gpt-override")["provider"], "openai")

    def test_a_bad_upstream_header_is_refused(self) -> None:
        status, _, body = self.post(
            "/v1/chat/completions",
            {"model": "gpt-bad-header", "messages": [{"role": "user", "content": "hi"}]},
            headers={"X-Nestbox-Upstream": "file:///etc/passwd"},
        )
        self.assertEqual(status, 400)
        self.assertEqual(json.loads(body)["error"]["code"], "upstream.invalid")
        self.assertEqual(self.openai.requests, [])

    def test_a_path_with_no_upstream_configured_is_refused(self) -> None:
        proxy.CONFIG = replace(self.config(), default_upstream="")
        status, _, body = self.post(
            "/v1beta/models/gemini-2.0-flash:generateContent",
            {"contents": [{"parts": [{"text": "hi"}]}]},
        )
        self.assertEqual(status, 502)
        self.assertEqual(json.loads(body)["error"]["code"], "upstream.unconfigured")
        self.assertEqual(self.default.requests, [])

    # -- credentials --------------------------------------------------------

    def test_the_clients_token_is_forwarded_unchanged(self) -> None:
        self.post(
            "/v1/messages",
            {"model": "claude-credentials", "messages": [{"role": "user", "content": "hi"}]},
            headers={
                "Authorization": "Bearer client-own-token",
                "x-api-key": "sk-client-own",
                "anthropic-version": "2023-06-01",
            },
        )
        seen = self.anthropic.requests[0]["headers"]
        self.assertEqual(seen["authorization"], "Bearer client-own-token")
        self.assertEqual(seen["x-api-key"], "sk-client-own")
        self.assertEqual(seen["anthropic-version"], "2023-06-01")

    def test_the_upstream_header_is_not_forwarded_on(self) -> None:
        self.post(
            "/v1/chat/completions",
            {"model": "gpt-header-drop", "messages": [{"role": "user", "content": "hi"}]},
            headers={"X-Nestbox-Upstream": self.openai.base},
        )
        seen = self.openai.requests[0]["headers"]
        self.assertNotIn("x-nestbox-upstream", seen)

    # -- privacy ------------------------------------------------------------

    def test_a_blocked_request_is_not_forwarded(self) -> None:
        status, _, body = self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-blocked",
                "messages": [{"role": "user", "content": "ship SECRET-TOKEN upstream"}],
            },
        )
        self.assertEqual(status, 403)
        answer = json.loads(body)["error"]
        self.assertEqual(answer["code"], "privacy.blocked")
        self.assertEqual(answer["detector"], "api_keys")
        self.assertEqual(self.openai.requests, [], "a blocked request must not be forwarded")
        row = self.wait_row(privacy_action="blocked")
        self.assertEqual(row["privacy_detector"], "api_keys")
        self.assertEqual(row["status"], 403)
        self.assertIsNone(row["usage"])
        self.assertIsNone(row["model"])

    def test_the_inspect_call_names_the_destination_host(self) -> None:
        self.post(
            "/v1/chat/completions",
            {"model": "gpt-inspect", "messages": [{"role": "user", "content": "hi"}]},
        )
        inspect = self.inspects()[0]
        self.assertEqual(inspect["sandbox"], SANDBOX)
        self.assertEqual(inspect["direction"], "outbound")
        self.assertEqual(inspect["content"], "hi")
        self.assertEqual(inspect["destination"], "127.0.0.1")
        self.assertEqual(self.control.calls[0]["authorization"], f"Bearer {ACCOUNTING_TOKEN}")

    def test_a_replaced_body_is_forwarded_with_the_replacement(self) -> None:
        self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-replaced",
                "messages": [{"role": "user", "content": "please REDACT this"}],
            },
        )
        forwarded = json.loads(self.openai.requests[0]["body"])
        self.assertEqual(forwarded["messages"][0]["content"], "please [redacted] this")
        row = self.wait_row(model="gpt-replaced")
        self.assertEqual(row["privacy_action"], "replaced")
        self.assertEqual(row["privacy_detector"], "credentials")

    def test_each_message_is_inspected_when_there_are_several(self) -> None:
        self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-multi",
                "messages": [
                    {"role": "system", "content": "be brief"},
                    {"role": "user", "content": "REDACT one"},
                    {"role": "user", "content": "REDACT two"},
                ],
            },
        )
        forwarded = json.loads(self.openai.requests[0]["body"])
        contents = [message["content"] for message in forwarded["messages"]]
        self.assertEqual(contents, ["be brief", "[redacted] one", "[redacted] two"])
        inspected = [inspect["content"] for inspect in self.inspects()]
        self.assertIn("REDACT one", inspected, "each message is inspected on its own")
        self.assertIn("REDACT two", inspected)
        self.assertEqual(self.wait_row(model="gpt-multi")["privacy_action"], "replaced")

    def test_a_blocked_message_among_several_blocks_the_request(self) -> None:
        status, _, _ = self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-multi-blocked",
                "messages": [
                    {"role": "user", "content": "REDACT one"},
                    {"role": "user", "content": "SECRET-TOKEN two"},
                ],
            },
        )
        self.assertEqual(status, 403)
        self.assertEqual(self.openai.requests, [])

    def test_an_allowed_body_is_forwarded_byte_for_byte(self) -> None:
        raw = (
            b'{ "model" : "gpt-allowed",  "messages" : '
            b'[ { "role": "user", "content": "hi" } ] }'
        )
        self.post("/v1/chat/completions", None, raw=raw)
        self.assertEqual(self.openai.requests[0]["body"], raw)
        self.assertEqual(self.wait_row(model="gpt-allowed")["privacy_action"], "allowed")

    def test_a_non_json_body_is_forwarded_and_marked_not_inspectable(self) -> None:
        raw = b"\x89PNG\r\n\x1a\n\x00\x00binary"
        status, _, _ = self.post(
            "/v1/messages",
            None,
            raw=raw,
            headers={"Content-Type": "application/octet-stream"},
        )
        self.assertEqual(status, 200)
        self.assertEqual(self.anthropic.requests[0]["body"], raw)
        self.assertEqual(self.inspects(), [], "there is nothing to inspect")
        row = self.wait_row(privacy_action="not_inspectable", endpoint="/v1/messages")
        self.assertEqual(row["status"], 200)
        self.assertIsNone(row["privacy_detector"])
        self.assertEqual(row["content_digest"], hashlib.sha256(raw).hexdigest())

    def test_a_json_body_without_text_is_not_inspectable(self) -> None:
        self.post("/v1/embeddings", {"model": "text-embedding-3-small", "input": [1, 2, 3]})
        row = self.wait_row(endpoint="/v1/embeddings")
        self.assertEqual(row["privacy_action"], "not_inspectable")

    def test_an_unreachable_control_api_is_open_by_default(self) -> None:
        self.control.server.inspect_status = 500  # type: ignore[attr-defined]
        status, _, _ = self.post(
            "/v1/chat/completions",
            {"model": "gpt-open", "messages": [{"role": "user", "content": "hi"}]},
        )
        self.assertEqual(status, 200)
        self.assertEqual(len(self.openai.requests), 1)
        self.assertEqual(self.wait_row(model="gpt-open")["privacy_action"], "not_inspected")

    def test_an_unreachable_control_api_is_closed_when_asked(self) -> None:
        self.control.server.inspect_status = 500  # type: ignore[attr-defined]
        proxy.CONFIG = replace(self.config(), privacy_fail_closed=True)
        status, _, body = self.post(
            "/v1/chat/completions",
            {"model": "gpt-closed", "messages": [{"role": "user", "content": "hi"}]},
        )
        self.assertEqual(status, 503)
        self.assertEqual(json.loads(body)["error"]["code"], "privacy.unavailable")
        self.assertEqual(self.openai.requests, [], "fail-closed must not forward")

    # -- accounting ---------------------------------------------------------

    def test_an_openai_row_carries_the_usage_numbers(self) -> None:
        raw = json.dumps(
            {"model": "gpt-usage", "messages": [{"role": "user", "content": "hi"}]}
        ).encode()
        self.post("/v1/chat/completions", None, raw=raw)
        row = self.wait_row(model="gpt-usage")
        self.assertEqual(row["sandbox"], SANDBOX)
        self.assertEqual(row["agent_instance"], AGENT)
        self.assertEqual(row["provider"], "openai")
        self.assertEqual(row["direction"], "outbound")
        self.assertEqual(row["endpoint"], "/v1/chat/completions")
        self.assertEqual(row["status"], 200)
        self.assertIsInstance(row["latency_ms"], int)
        self.assertEqual(
            row["usage"],
            {
                "prompt_tokens": 11,
                "completion_tokens": 7,
                "cached_tokens": 3,
                "total_tokens": 18,
            },
        )
        self.assertEqual(row["content_digest"], hashlib.sha256(raw).hexdigest())

    def test_an_anthropic_row_carries_the_usage_numbers(self) -> None:
        self.post(
            "/v1/messages",
            {
                "model": "claude-usage",
                "messages": [{"role": "user", "content": "hi"}],
            },
        )
        row = self.wait_row(model="claude-usage")
        self.assertEqual(
            row["usage"],
            {
                "prompt_tokens": 5,
                "completion_tokens": 9,
                "cached_tokens": 2,
                "total_tokens": 14,
            },
        )
        self.assertEqual(row["privacy_action"], "allowed")

    def test_the_digest_is_of_the_bytes_that_were_forwarded(self) -> None:
        self.post(
            "/v1/chat/completions",
            {"model": "gpt-digest", "messages": [{"role": "user", "content": "REDACT me"}]},
        )
        forwarded = self.openai.requests[0]["body"]
        row = self.wait_row(model="gpt-digest")
        self.assertEqual(row["privacy_action"], "replaced")
        self.assertEqual(row["content_digest"], hashlib.sha256(forwarded).hexdigest())

    def test_a_stream_is_relayed_byte_for_byte_without_usage(self) -> None:
        status, headers, body = self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-stream",
                "stream": True,
                "messages": [{"role": "user", "content": "hi"}],
            },
        )
        self.assertEqual(status, 200)
        self.assertEqual(headers.get("Content-Type"), "text/event-stream")
        self.assertEqual(body, b'data: {"chunk":1}\n\ndata: {"chunk":2}\n\n')
        row = self.wait_row(model="gpt-stream")
        self.assertIsNone(row["usage"])
        self.assertEqual(row["status"], 200)

    def test_no_row_carries_content(self) -> None:
        self.post(
            "/v1/chat/completions",
            {
                "model": "gpt-no-content",
                "messages": [{"role": "user", "content": "a private phrase"}],
            },
        )
        row = self.wait_row(model="gpt-no-content")
        self.assertNotIn("a private phrase", json.dumps(row))
        self.assertEqual(
            set(row),
            {
                "sandbox",
                "agent_instance",
                "provider",
                "model",
                "direction",
                "endpoint",
                "status",
                "latency_ms",
                "usage",
                "content_digest",
                "privacy_action",
                "privacy_detector",
            },
        )


def inspect_decision(content: str) -> dict:
    """The stub's policy: one blocked marker, one rewritten marker."""
    if "SECRET-TOKEN" in content:
        return {
            "permitted": False,
            "action": "blocked",
            "transformed_content": content.replace("SECRET-TOKEN", "*****"),
            "findings": [
                {"detector": "api_keys", "masked_value": "S*****N", "action": "blocked"}
            ],
        }
    if "REDACT" in content:
        return {
            "permitted": True,
            "action": "replaced",
            "transformed_content": content.replace("REDACT", "[redacted]"),
            "findings": [
                {"detector": "credentials", "masked_value": "R****T", "action": "replaced"}
            ],
        }
    return {
        "permitted": True,
        "action": "allowed",
        "transformed_content": content,
        "findings": [],
    }


if __name__ == "__main__":
    unittest.main()
