"""Tests for the Nestbox MCP hub.

Run with `python3 -m unittest discover -s services/tool-images/mcp-hub -p 'test_*.py'`
from the repository root, or `python3 -m unittest test_server` in this folder.

They start the real `ThreadingHTTPServer` on an ephemeral port against a
temporary home, so what is tested is the process that ships, not a mock of it.
"""
from __future__ import annotations

import http.client
import json
import os
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer

import server as hub


class HubTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.home = tempfile.TemporaryDirectory()
        cls.saved_home = hub.HOME
        cls.saved_token = hub.TOKEN
        hub.HOME = os.path.realpath(cls.home.name)
        hub.TOKEN = "test-token"
        with open(os.path.join(hub.HOME, "notes.txt"), "w", encoding="utf-8") as handle:
            handle.write("inside the home")
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), hub.Handler)
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls) -> None:
        cls.server.shutdown()
        cls.server.server_close()
        hub.HOME = cls.saved_home
        hub.TOKEN = cls.saved_token
        cls.home.cleanup()

    def request(
        self,
        method: str,
        path: str,
        payload: dict | None = None,
        token: str | None = "test-token",
    ) -> tuple[int, dict[str, str], bytes]:
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        body = json.dumps(payload).encode() if payload is not None else None
        headers = {"Content-Type": "application/json"}
        if token:
            headers["Authorization"] = f"Bearer {token}"
        connection.request(method, path, body=body, headers=headers)
        response = connection.getresponse()
        data = response.read()
        result = (response.status, dict(response.getheaders()), data)
        connection.close()
        return result

    def rpc(self, payload: dict, token: str | None = "test-token") -> tuple[int, dict, dict[str, str]]:
        status, headers, body = self.request("POST", "/mcp", payload, token=token)
        return status, (json.loads(body) if body else {}), headers

    # -- liveness and authentication ---------------------------------------

    def test_healthz_needs_no_token(self) -> None:
        status, _, body = self.request("GET", "/healthz", token=None)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)["status"], "ok")

    def test_a_request_without_a_token_is_refused(self) -> None:
        status, _, _ = self.request("POST", "/mcp", {"jsonrpc": "2.0", "id": 1}, token=None)
        self.assertEqual(status, 401)

    def test_a_wrong_token_is_refused(self) -> None:
        status, _, _ = self.request(
            "POST",
            "/mcp",
            {"jsonrpc": "2.0", "id": 1, "method": "tools/list"},
            token="wrong",
        )
        self.assertEqual(status, 401)

    def test_an_unset_server_token_refuses_everything(self) -> None:
        saved = hub.TOKEN
        hub.TOKEN = ""
        try:
            status, _, _ = self.request("POST", "/mcp", {"jsonrpc": "2.0", "id": 1})
            self.assertEqual(status, 401)
        finally:
            hub.TOKEN = saved

    # -- the handshake ------------------------------------------------------

    def test_initialize_assigns_a_session_and_announces_tools(self) -> None:
        status, answer, headers = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "test", "version": "1"},
                },
            }
        )
        self.assertEqual(status, 200)
        self.assertEqual(answer["result"]["protocolVersion"], "2025-06-18")
        self.assertIn("tools", answer["result"]["capabilities"])
        self.assertTrue(headers.get("Mcp-Session-Id"), "a session id is issued so the proxy can log it")

    def test_an_unknown_protocol_version_falls_back_to_ours(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "1999-01-01"},
            }
        )
        self.assertEqual(answer["result"]["protocolVersion"], hub.PROTOCOL_VERSION)

    def test_a_notification_has_no_body(self) -> None:
        status, _, body = self.request(
            "POST", "/mcp", {"jsonrpc": "2.0", "method": "notifications/initialized"}
        )
        self.assertEqual(status, 202)
        self.assertEqual(body, b"")

    def test_ping_answers(self) -> None:
        _, answer, _ = self.rpc({"jsonrpc": "2.0", "id": 9, "method": "ping"})
        self.assertEqual(answer["result"], {})

    def test_tools_list_names_the_three_tools(self) -> None:
        _, answer, _ = self.rpc({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
        names = {tool["name"] for tool in answer["result"]["tools"]}
        self.assertEqual(names, {"read_file", "write_file", "run_command"})

    def test_an_unknown_method_is_a_protocol_error(self) -> None:
        _, answer, _ = self.rpc({"jsonrpc": "2.0", "id": 3, "method": "resources/list"})
        self.assertEqual(answer["error"]["code"], -32601)

    # -- the tools ----------------------------------------------------------

    def test_read_file_reads_inside_the_home(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {"name": "read_file", "arguments": {"path": "notes.txt"}},
            }
        )
        result = answer["result"]
        self.assertFalse(result["isError"])
        self.assertEqual(result["content"][0]["text"], "inside the home")

    def test_read_file_refuses_a_traversal(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "read_file", "arguments": {"path": "../../etc/passwd"}},
            }
        )
        result = answer["result"]
        self.assertTrue(result["isError"])
        self.assertNotIn("root:", result["content"][0]["text"])

    def test_read_file_refuses_a_symlink_out_of_the_home(self) -> None:
        link = os.path.join(hub.HOME, "escape")
        if os.path.lexists(link):
            os.unlink(link)
        os.symlink("/etc", link)
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 6,
                "method": "tools/call",
                "params": {"name": "read_file", "arguments": {"path": "escape/passwd"}},
            }
        )
        self.assertTrue(answer["result"]["isError"])
        os.unlink(link)

    def test_write_file_round_trips(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 7,
                "method": "tools/call",
                "params": {
                    "name": "write_file",
                    "arguments": {"path": "out/hello.txt", "content": "hello"},
                },
            }
        )
        self.assertFalse(answer["result"]["isError"])
        with open(os.path.join(hub.HOME, "out", "hello.txt"), encoding="utf-8") as handle:
            self.assertEqual(handle.read(), "hello")

    def test_run_command_is_an_argument_vector_not_a_shell_string(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 8,
                "method": "tools/call",
                "params": {
                    "name": "run_command",
                    "arguments": {"argv": ["printf", "%s", "one; two && three"]},
                },
            }
        )
        result = answer["result"]
        self.assertFalse(result["isError"])
        self.assertIn("one; two && three", result["content"][0]["text"])

    def test_run_command_refuses_a_string_instead_of_a_vector(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 9,
                "method": "tools/call",
                "params": {"name": "run_command", "arguments": {"argv": "echo hi"}},
            }
        )
        self.assertTrue(answer["result"]["isError"])

    def test_an_unknown_tool_is_a_protocol_error(self) -> None:
        _, answer, _ = self.rpc(
            {
                "jsonrpc": "2.0",
                "id": 10,
                "method": "tools/call",
                "params": {"name": "rm_rf", "arguments": {}},
            }
        )
        self.assertEqual(answer["error"]["code"], -32602)

    # -- the transport ------------------------------------------------------

    def test_a_streaming_get_is_refused_with_the_allowed_methods(self) -> None:
        status, headers, _ = self.request("GET", "/mcp")
        self.assertEqual(status, 405)
        self.assertIn("POST", headers.get("Allow", ""))

    def test_a_session_can_be_terminated(self) -> None:
        status, _, _ = self.request("DELETE", "/mcp")
        self.assertEqual(status, 204)

    def test_an_oversized_body_is_refused_by_length(self) -> None:
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        connection.putrequest("POST", "/mcp")
        connection.putheader("Content-Type", "application/json")
        connection.putheader("Authorization", "Bearer test-token")
        connection.putheader("Content-Length", str(hub.MAX_REQUEST_BYTES + 1))
        connection.endheaders()
        response = connection.getresponse()
        self.assertEqual(response.status, 413)
        connection.close()

    def test_a_bad_json_body_is_a_parse_error(self) -> None:
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        connection.request(
            "POST",
            "/mcp",
            body=b"{not json",
            headers={"Content-Type": "application/json", "Authorization": "Bearer test-token"},
        )
        response = connection.getresponse()
        answer = json.loads(response.read())
        self.assertEqual(response.status, 400)
        self.assertEqual(answer["error"]["code"], -32700)
        connection.close()


if __name__ == "__main__":
    unittest.main()
