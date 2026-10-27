"""Tests for the terminal pump, without FastAPI and without Docker.

The pump is the part that can corrupt a session: it decides what is input, what
is a control frame, when the idle clock runs, and why a terminal closed. So it
is tested with a fake socket on each side — a stdlib-only test that runs
anywhere, including in a container that never had a Docker socket.

`app.terminal` imports FastAPI at module scope, so the import below installs
minimal stand-ins first. That is deliberate: the module under test is the real
file, and only the framework it is mounted on is replaced.
"""
from __future__ import annotations

import asyncio
import json
import os
import queue
import sys
import tempfile
import threading
import types
import unittest

# --- framework stand-ins ---------------------------------------------------


class _WebSocket:
    """Structural stand-in; the pump only calls receive/send_text/close."""


class _WebSocketDisconnect(Exception):
    """Stand-in for the exception the router would import."""


class _APIRouter:
    def websocket(self, _path: str):
        return lambda handler: handler


_fastapi = types.ModuleType("fastapi")
_fastapi.APIRouter = _APIRouter
_fastapi.WebSocket = _WebSocket
_fastapi.WebSocketDisconnect = _WebSocketDisconnect
sys.modules.setdefault("fastapi", _fastapi)

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from app import terminal as tm  # noqa: E402


# --- fakes ----------------------------------------------------------------


class FakeWebSocket:
    """A browser end that the test drives."""

    def __init__(self) -> None:
        self.incoming: asyncio.Queue = asyncio.Queue()
        self.sent: list[str] = []
        self.closed: tuple[int, str] | None = None

    def push(self, message: dict) -> None:
        self.incoming.put_nowait(message)

    def type(self, text: str) -> None:
        self.push({"type": "websocket.receive", "text": text, "bytes": None})

    def send_bytes(self, data: bytes) -> None:
        self.push({"type": "websocket.receive", "text": None, "bytes": data})

    def disconnect(self) -> None:
        self.push({"type": "websocket.disconnect", "code": 1000})

    async def receive(self) -> dict:
        return await self.incoming.get()

    async def send_text(self, text: str) -> None:
        self.sent.append(text)

    async def close(self, code: int = 1000, reason: str = "") -> None:
        self.closed = (code, reason)

    def output(self) -> str:
        """Everything that was not a control frame."""
        return "".join(text for text in self.sent if not text.startswith('{"type":"close"'))


class FakePty:
    """A command end that the test drives, with a blocking read like a socket."""

    def __init__(self) -> None:
        self.queue: queue.Queue[bytes] = queue.Queue()
        self.written: list[bytes] = []
        self.closed = False
        self.waiting = 0
        self.lock = threading.Lock()

    def emit(self, data: bytes) -> None:
        self.queue.put(data)

    def read(self, size: int = 8192) -> bytes:
        with self.lock:
            self.waiting += 1
        try:
            return self.queue.get()
        finally:
            with self.lock:
                self.waiting -= 1

    def release(self) -> None:
        """Unblock every read still waiting, the way closing the socket does.

        Each cancelled pump iteration leaves one reader thread inside `get`.
        Closing a real PTY releases all of them with EOF; the fake does the same
        with an EOF sentinel per possible waiter, so no thread outlives a test.
        """
        for _ in range(64):
            self.queue.put(b"")

    def write(self, data: bytes) -> None:
        self.written.append(data)

    def close(self) -> None:
        self.closed = True
        self.queue.put(b"")  # unblock a read still waiting


# --- tests ----------------------------------------------------------------


class AuthorisationTests(unittest.TestCase):
    def setUp(self) -> None:
        os.environ.pop("NESTBOX_NODE_TOKEN", None)
        os.environ.pop("NESTBOX_NODE_TOKEN_FILE", None)
        self.token_file = tempfile.NamedTemporaryFile("w", delete=False)
        self.token_file.write("s3cret\n")
        self.token_file.close()

    def tearDown(self) -> None:
        os.environ.pop("NESTBOX_NODE_TOKEN", None)
        os.environ.pop("NESTBOX_NODE_TOKEN_FILE", None)
        os.unlink(self.token_file.name)

    def _socket(self, query: str = "", protocol: str = "") -> FakeWebSocket:
        headers = {"sec-websocket-protocol": protocol} if protocol else {}
        socket = FakeWebSocket()
        socket.query_params = (
            types.SimpleNamespace(get=lambda key: "s3cret" if key == "token" else None)
            if query
            else types.SimpleNamespace(get=lambda key: None)
        )
        socket.headers = headers
        return socket

    def test_an_unset_token_refuses_everything(self) -> None:
        # A missing secret must never mean "no check".
        self.assertFalse(tm.authorised(self._socket(query="1")))

    def test_the_token_is_read_from_a_file_first(self) -> None:
        os.environ["NESTBOX_NODE_TOKEN_FILE"] = self.token_file.name
        os.environ["NESTBOX_NODE_TOKEN"] = "from-the-environment"
        self.assertEqual(tm.node_token(), "s3cret")
        self.assertTrue(tm.authorised(self._socket(query="1")))

    def test_an_unreadable_token_file_refuses_everything(self) -> None:
        os.environ["NESTBOX_NODE_TOKEN_FILE"] = "/nonexistent/node_token"
        os.environ["NESTBOX_NODE_TOKEN"] = "s3cret"
        self.assertEqual(tm.node_token(), "")
        self.assertFalse(tm.authorised(self._socket(query="1")))

    def test_the_token_in_the_query_is_accepted(self) -> None:
        os.environ["NESTBOX_NODE_TOKEN"] = "s3cret"
        self.assertTrue(tm.authorised(self._socket(query="1")))

    def test_the_token_in_the_subprotocol_is_accepted(self) -> None:
        os.environ["NESTBOX_NODE_TOKEN"] = "s3cret"
        self.assertTrue(tm.authorised(self._socket(protocol="nestbox-terminal, s3cret")))
        self.assertFalse(tm.authorised(self._socket(protocol="nestbox-terminal, wrong")))
        # Offering only the subprotocol name is not a credential.
        self.assertFalse(tm.authorised(self._socket(protocol="nestbox-terminal")))


class FrameTests(unittest.TestCase):
    def test_a_resize_frame_is_a_control_frame(self) -> None:
        frame = tm.control_frame('{"type":"resize","cols":120,"rows":40}')
        self.assertIsNotNone(frame)
        self.assertEqual(tm.resize_dimensions(frame or {}), (120, 40))

    def test_ordinary_typing_is_not_a_control_frame(self) -> None:
        for line in ("ls -la\n", "{", "{}", "{not json", '{"cols":80}', "\x1b[A"):
            self.assertIsNone(tm.control_frame(line), line)

    def test_a_non_object_json_frame_is_not_a_control_frame(self) -> None:
        self.assertIsNone(tm.control_frame('{"type":1}'))
        self.assertIsNone(tm.control_frame('["resize"]'))

    def test_dimensions_are_clamped(self) -> None:
        self.assertEqual(tm.resize_dimensions({"cols": 0, "rows": -4}), (1, 1))
        self.assertEqual(tm.resize_dimensions({"cols": 99_999, "rows": 50}), (500, 50))
        self.assertEqual(tm.resize_dimensions({}), (80, 24))
        self.assertEqual(tm.resize_dimensions({"cols": "abc", "rows": "30"}), (80, 30))

    def test_the_close_frame_names_the_reason(self) -> None:
        self.assertEqual(json.loads(tm.close_frame("idle")), {"type": "close", "reason": "idle"})
        with_message = json.loads(tm.close_frame("error", "boom"))
        self.assertEqual(with_message["reason"], "error")
        self.assertEqual(with_message["message"], "boom")


class IdleTimeoutTests(unittest.TestCase):
    def test_the_default_is_thirty_minutes(self) -> None:
        os.environ.pop("NESTBOX_TERMINAL_IDLE_SECONDS", None)
        self.assertEqual(tm.idle_seconds(), 1800.0)

    def test_the_timeout_is_configurable_and_clamped(self) -> None:
        os.environ["NESTBOX_TERMINAL_IDLE_SECONDS"] = "7"
        self.assertEqual(tm.idle_seconds(), 7.0)
        os.environ["NESTBOX_TERMINAL_IDLE_SECONDS"] = "0"
        self.assertEqual(tm.idle_seconds(), tm.MIN_IDLE_SECONDS)
        os.environ["NESTBOX_TERMINAL_IDLE_SECONDS"] = "nonsense"
        self.assertEqual(tm.idle_seconds(), 1800.0)
        os.environ.pop("NESTBOX_TERMINAL_IDLE_SECONDS", None)


class PumpTests(unittest.IsolatedAsyncioTestCase):
    async def run_pump(self, coroutine, pty: FakePty):
        """Await the pump and then unblock the reader thread.

        A real terminal closes its PTY on every exit path, which is what
        releases a read still waiting in the socket. The test does the same with
        an EOF sentinel, so no thread outlives its test.
        """
        try:
            return await asyncio.wait_for(coroutine, timeout=5)
        finally:
            pty.release()

    async def test_input_reaches_the_pty_and_output_reaches_the_browser(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        resizes: list[tuple[int, int]] = []

        async def drive() -> None:
            # One step at a time: a queue of ready messages would let the
            # disconnect win the race against the output it came after.
            socket.type("ls\n")
            await asyncio.sleep(0.05)
            pty.emit(b"$ ")
            await asyncio.sleep(0.05)
            socket.disconnect()

        driver = asyncio.create_task(drive())
        reason = await self.run_pump(
            tm.pump(socket, pty, lambda c, r: resizes.append((c, r)), 30.0), pty
        )
        await driver

        self.assertEqual(reason, "user")
        self.assertEqual(pty.written, [b"ls\n"])
        self.assertEqual(socket.output(), "$ ")

    async def test_a_resize_frame_resizes_and_is_not_typed(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        resizes: list[tuple[int, int]] = []
        socket.type('{"type":"resize","cols":80,"rows":24}')
        socket.type('{"type":"close"}')

        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: resizes.append((c, r)), 30.0), pty)

        self.assertEqual(reason, "user")
        self.assertEqual(resizes, [(80, 24)])
        self.assertEqual(pty.written, [])

    async def test_an_idle_terminal_closes_with_a_named_reason(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 0.2), pty)
        self.assertEqual(reason, "idle")

    async def test_output_does_not_postpone_the_idle_close(self) -> None:
        # ADR 0023 measures idleness in input. A repainting `htop` is still an
        # unattended session.
        socket, pty = FakeWebSocket(), FakePty()

        async def sparkle() -> None:
            for _ in range(40):
                pty.emit(b"tick")
                await asyncio.sleep(0.02)

        sparkler = asyncio.create_task(sparkle())
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 0.2), pty)
        sparkler.cancel()

        self.assertEqual(reason, "idle")

    async def test_input_postpones_the_idle_close(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()

        async def typing() -> None:
            for _ in range(4):
                await asyncio.sleep(0.1)
                socket.type("x")

        typist = asyncio.create_task(typing())
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 0.25), pty)
        typist.cancel()

        self.assertEqual(reason, "idle")
        self.assertEqual(pty.written, [b"x", b"x", b"x", b"x"])

    async def test_a_closed_pty_ends_with_sandbox_stopped(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        pty.emit(b"bye")
        pty.emit(b"")
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 30.0), pty)
        self.assertEqual(reason, "sandbox_stopped")
        # What reached the browser before the stream ended is kept.
        self.assertEqual(socket.output(), "bye")

    async def test_a_disconnect_ends_the_session(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        socket.disconnect()
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 30.0), pty)
        self.assertEqual(reason, "user")

    async def test_a_multibyte_character_split_across_reads_survives(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        encoded = "grüß".encode()
        pty.emit(encoded[:2])
        pty.emit(encoded[2:])

        # No input and no disconnect: the session ends on the idle clock, so
        # every byte that was ready has been forwarded.
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 0.3), pty)

        self.assertEqual(reason, "idle")
        self.assertEqual(socket.output(), "grüß")

    async def test_binary_input_is_forwarded_unchanged(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        socket.send_bytes(b"\x1b[1;5C")
        socket.disconnect()

        await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 30.0), pty)

        self.assertEqual(pty.written, [b"\x1b[1;5C"])

    async def test_an_oversized_frame_closes_the_session(self) -> None:
        socket, pty = FakeWebSocket(), FakePty()
        socket.type("x" * (tm.MAX_INPUT_BYTES + 1))
        reason = await self.run_pump(tm.pump(socket, pty, lambda c, r: None, 30.0), pty)
        self.assertEqual(reason, "error")
        self.assertEqual(pty.written, [])


class ContainerNameTests(unittest.TestCase):
    def test_the_prefix_matches_the_runner_contract(self) -> None:
        self.assertEqual(tm.container_name("hermes-01"), "nestbox-sandbox-hermes-01")


if __name__ == "__main__":
    unittest.main()
