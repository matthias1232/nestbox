"""The desktop of one sandbox, as a WebSocket carrying the VNC byte stream.

A sandbox with the desktop pack runs a VNC server on `127.0.0.1:5900` *inside*
its own network namespace. Nothing publishes that port and nothing outside the
container can reach it. This module is the only way out, and it works the way
the terminal does:

* the control API connects here with the node token (the same one the terminal
  uses) and has already checked who the user is and whether they may use this
  sandbox;
* a fixed program is started in the container with `docker exec`; it connects
  to `127.0.0.1:5900` and copies bytes in both directions;
* the runner moves those bytes to and from the WebSocket and looks at none of
  them. No screen content is recorded here.

The in-container program is Python because the desktop image already has it.
It is a constant, never built from the request.
"""
from __future__ import annotations

import asyncio
import logging
import struct
from typing import Any

from fastapi import APIRouter, WebSocket, WebSocketDisconnect

from app.gateway_exec import GatewayStream
from app.terminal import CLOSE_INTERNAL, CLOSE_UNAUTHORIZED, get_container, node_token
from app.workload import openshell_name

router = APIRouter()
logger = logging.getLogger("nestbox.runner.vnc")

SUBPROTOCOL = "nestbox-vnc"
CLOSE_NORMAL = 1000
READ_CHUNK = 65_536
VNC_PORT = 5900

BRIDGE = (
    "import os,socket,threading\n"
    f"s=socket.create_connection(('127.0.0.1',{VNC_PORT}),timeout=5)\n"
    "s.settimeout(None)\n"
    "def up():\n"
    "    while True:\n"
    "        d=os.read(0,65536)\n"
    "        if not d: break\n"
    "        s.sendall(d)\n"
    "    s.shutdown(socket.SHUT_WR)\n"
    "threading.Thread(target=up,daemon=True).start()\n"
    "while True:\n"
    "    d=s.recv(65536)\n"
    "    if not d: break\n"
    "    os.write(1,d)\n"
)


def authorised(websocket: WebSocket) -> bool:
    """Whether the handshake carries the node token (see the terminal)."""
    expected = node_token()
    if not expected:
        return False
    for protocol in websocket.headers.get("sec-websocket-protocol", "").split(","):
        candidate = protocol.strip()
        if candidate and candidate != SUBPROTOCOL and candidate == expected:
            return True
    return False


class ExecStream:
    """The raw, hijacked socket of a non-TTY exec, demultiplexed.

    Without a TTY Docker frames the output as `[stream, 0, 0, 0, size:u32]` plus
    payload. Only stdout carries the VNC stream; stderr is logged and dropped.
    """

    def __init__(self, raw: Any) -> None:
        self._socket = getattr(raw, "_sock", raw)
        settimeout = getattr(self._socket, "settimeout", None)
        if settimeout is not None:
            settimeout(None)
        self._buffer = b""

    def _take(self, size: int) -> bytes:
        while len(self._buffer) < size:
            chunk = self._socket.recv(READ_CHUNK)
            if not chunk:
                raise EOFError
            self._buffer += chunk
        out, self._buffer = self._buffer[:size], self._buffer[size:]
        return out

    def read(self) -> bytes:
        """The next stdout payload; raises `EOFError` when the exec ends."""
        while True:
            header = self._take(8)
            stream, size = header[0], struct.unpack(">I", header[4:])[0]
            payload = self._take(size) if size else b""
            if stream == 1:
                return payload
            if stream == 2 and payload:
                logger.warning("vnc bridge: %s", payload.decode(errors="replace").strip()[:200])

    def write(self, data: bytes) -> None:
        self._socket.sendall(data)

    def close(self) -> None:
        try:
            self._socket.close()
        except OSError:
            pass


def open_bridge(container: Any) -> Any:
    gateway = openshell_name(container.name)
    if gateway:
        # The desktop's loopback port lives inside the sandbox's own boundary, so
        # the bridge is started through the gateway, not beside it.
        return GatewayStream.open(gateway, ["python3", "-c", BRIDGE])
    client = container.client
    created = client.api.exec_create(
        container.id, ["python3", "-c", BRIDGE], tty=False, stdin=True, stdout=True, stderr=True
    )
    return ExecStream(client.api.exec_start(created["Id"], tty=False, socket=True))


@router.websocket("/v1/sandboxes/{name}/vnc")
async def vnc(websocket: WebSocket, name: str) -> None:
    """Carry the sandbox's VNC stream to the control API."""
    if not authorised(websocket):
        await websocket.close(code=CLOSE_UNAUTHORIZED, reason="node token required")
        return

    import docker
    from docker.errors import DockerException, NotFound

    client = docker.from_env()
    try:
        container = await asyncio.to_thread(get_container, client, name)
    except NotFound:
        await websocket.close(code=CLOSE_INTERNAL, reason="sandbox not found")
        return
    except DockerException as exc:
        await websocket.close(code=CLOSE_INTERNAL, reason=f"docker unavailable: {exc}")
        return
    if container.status != "running":
        await websocket.close(code=CLOSE_INTERNAL, reason="sandbox is not running")
        return

    await websocket.accept(subprotocol=SUBPROTOCOL)
    try:
        stream = await asyncio.to_thread(open_bridge, container)
    except DockerException as exc:
        await websocket.close(code=CLOSE_INTERNAL, reason=f"could not open the desktop: {exc}")
        return

    async def to_browser() -> None:
        while True:
            try:
                data = await asyncio.to_thread(stream.read)
            except (EOFError, OSError):
                return
            await websocket.send_bytes(data)

    async def to_desktop() -> None:
        while True:
            message = await websocket.receive()
            if message.get("type") == "websocket.disconnect":
                return
            data = message.get("bytes")
            if data:
                await asyncio.to_thread(stream.write, data)

    tasks = [asyncio.create_task(to_browser()), asyncio.create_task(to_desktop())]
    try:
        await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
    except WebSocketDisconnect:
        pass
    finally:
        for task in tasks:
            task.cancel()
        stream.close()
        try:
            await websocket.close(code=CLOSE_NORMAL)
        except RuntimeError:
            pass
