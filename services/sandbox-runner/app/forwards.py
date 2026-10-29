"""Published ports: the part that makes a reserved port actually answer.

The control plane reserves a port per sandbox from the node's pool and records
it in `forwards`. Reserving is bookkeeping; nothing listened. This module is the
listener. For every forward it opens one TCP socket in the runner, which is
published on the node by compose, and:

* `ssh`   — speaks SSH itself and opens the sandbox (`ssh_gateway.py`);
* `agent` and `custom` — relays bytes to `<sandbox address>:<target port>` on the
  sandbox network, which is how a web UI or an agent API inside a sandbox is
  reached from the LAN without the sandbox ever holding a published port.

The control plane sends the complete set for one sandbox (`PUT`), so a retry is
the same call and a withdrawn port is simply absent from the next one. The set
is persisted in the runner's state directory and restored at start, because the
runner is recreated on every deploy and the database should not have to push
every sandbox again.

A forward reports two separate facts, and the console shows both:

* `listening` — the runner holds the node-side socket (the port is open);
* `target_up` — something answers behind it, inside the sandbox. A port can be
  open with nothing behind it, which is the normal state of an agent that has
  not been started yet; saying "active" for that would be a lie.
"""
from __future__ import annotations

import asyncio
import json
import logging
import os
import socket
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Literal

import docker
from docker.errors import DockerException, NotFound
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from app import ssh_gateway

logger = logging.getLogger("nestbox.runner.forwards")
router = APIRouter()

CONTAINER_PREFIX = "nestbox-sandbox-"
STATE_DIR = os.getenv("NESTBOX_RUNNER_STATE", "/var/lib/nestbox-runner")
BIND = os.getenv("NESTBOX_FORWARD_BIND", "0.0.0.0")
NETWORK = os.getenv("NESTBOX_RUNTIME_NETWORK", "nestbox-runtime")
PROBE_SECONDS = 0.6
CONNECT_SECONDS = 5.0
RELAY_CHUNK = 65_536
NAME_PATTERN = r"^[a-z0-9-]{1,63}$"


def published_range() -> tuple[int, int]:
    """The port range compose publishes on the node, `start-end`.

    A listener outside it would open inside the runner and be unreachable from
    outside, so it is refused with a sentence instead of reporting success.
    """
    raw = os.getenv("NESTBOX_FORWARD_RANGE", "22200-22299")
    try:
        start, end = (int(part) for part in raw.split("-", 1))
    except ValueError:
        return 22200, 22299
    return start, end


class ForwardSpec(BaseModel):
    port: int = Field(ge=1024, le=65535)
    target_port: int = Field(ge=1, le=65535)
    purpose: Literal["ssh", "agent", "custom"] = "custom"


class ForwardSet(BaseModel):
    forwards: list[ForwardSpec] = Field(default_factory=list)
    # OpenSSH `authorized_keys` lines. Public keys only.
    authorized_keys: list[str] = Field(default_factory=list)


@dataclass
class Listener:
    sandbox: str
    spec: ForwardSpec
    server: Any = None
    error: str | None = None
    connections: int = 0
    bytes_in: int = 0
    bytes_out: int = 0


@dataclass
class Manager:
    listeners: dict[int, Listener] = field(default_factory=dict)
    sets: dict[str, ForwardSet] = field(default_factory=dict)
    lock: asyncio.Lock = field(default_factory=asyncio.Lock)

    # -- state ------------------------------------------------------------------------------
    def _state_file(self) -> Path:
        return Path(STATE_DIR) / "forwards.json"

    def _persist(self) -> None:
        directory = Path(STATE_DIR)
        try:
            directory.mkdir(parents=True, exist_ok=True)
            document = {name: value.model_dump() for name, value in self.sets.items()}
            handle, temp = tempfile.mkstemp(dir=directory, prefix=".forwards-")
            with os.fdopen(handle, "w", encoding="utf-8") as out:
                json.dump(document, out)
            os.chmod(temp, 0o600)
            os.replace(temp, self._state_file())
        except OSError as failure:
            logger.warning("could not persist forwards: %s", failure)

    def _load(self) -> dict[str, ForwardSet]:
        try:
            raw = json.loads(self._state_file().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return {}
        sets: dict[str, ForwardSet] = {}
        for name, value in raw.items():
            try:
                sets[name] = ForwardSet.model_validate(value)
            except ValueError:
                logger.warning("ignoring a stored forward set for %s that no longer validates", name)
        return sets

    # -- keys -------------------------------------------------------------------------------
    def keys_for(self, sandbox: str) -> str:
        current = self.sets.get(sandbox)
        return "\n".join(current.authorized_keys) if current else ""

    # -- listeners --------------------------------------------------------------------------
    async def _open(self, sandbox: str, spec: ForwardSpec) -> Listener:
        listener = Listener(sandbox=sandbox, spec=spec)
        low, high = published_range()
        if not low <= spec.port <= high:
            listener.error = (
                f"Port {spec.port} is outside the range the runner publishes ({low}-{high}); "
                "set NESTBOX_FORWARD_RANGE and the compose port range to match the node's pool."
            )
            return listener
        try:
            if spec.purpose == "ssh":
                listener.server = await ssh_gateway.serve(
                    client=docker_client(),
                    sandbox=sandbox,
                    bind=BIND,
                    port=spec.port,
                    keys_for=self.keys_for,
                    host_key=ssh_gateway.ensure_host_key(STATE_DIR),
                )
            else:
                listener.server = await asyncio.start_server(
                    lambda reader, writer: relay(listener, reader, writer), BIND, spec.port
                )
        except OSError as failure:
            listener.error = f"Cannot listen on {BIND}:{spec.port}: {failure.strerror or failure}"
        except Exception as failure:  # noqa: BLE001
            listener.error = f"Cannot start the {spec.purpose} listener: {failure}"
        return listener

    async def _close(self, listener: Listener) -> None:
        server = listener.server
        listener.server = None
        if server is None:
            return
        server.close()
        try:
            await asyncio.wait_for(server.wait_closed(), timeout=2)
        except (asyncio.TimeoutError, Exception):  # noqa: BLE001
            pass

    async def apply(self, sandbox: str, wanted: ForwardSet) -> list[dict[str, Any]]:
        async with self.lock:
            ports = [spec.port for spec in wanted.forwards]
            if len(set(ports)) != len(ports):
                raise HTTPException(422, "a port may appear once per sandbox")
            for spec in wanted.forwards:
                owner = self.listeners.get(spec.port)
                if owner is not None and owner.sandbox != sandbox:
                    raise HTTPException(409, f"port {spec.port} belongs to sandbox '{owner.sandbox}'")

            previous = self.sets.get(sandbox)
            old = {spec.port: spec for spec in (previous.forwards if previous else [])}
            new = {spec.port: spec for spec in wanted.forwards}

            for port in set(old) - set(new):
                await self._drop(port)
            for port, spec in new.items():
                existing = self.listeners.get(port)
                # A listener that failed to bind gets another try on every apply,
                # so a port that was busy for a moment is not stuck failed.
                if existing and existing.server is not None and existing.spec == spec:
                    continue
                if existing:
                    await self._drop(port)
                self.listeners[port] = await self._open(sandbox, spec)

            self.sets[sandbox] = wanted
            self._persist()
        # The probe opens a socket per forward; keep it off the event loop.
        return await asyncio.to_thread(self.status, sandbox)

    async def _drop(self, port: int) -> None:
        listener = self.listeners.pop(port, None)
        if listener:
            await self._close(listener)

    async def remove(self, sandbox: str) -> None:
        async with self.lock:
            for port in [p for p, item in self.listeners.items() if item.sandbox == sandbox]:
                await self._drop(port)
            if self.sets.pop(sandbox, None) is not None:
                self._persist()

    async def restore(self) -> None:
        stored = self._load()
        for sandbox, wanted in stored.items():
            try:
                await self.apply(sandbox, wanted)
            except HTTPException as failure:
                logger.warning("could not restore forwards of %s: %s", sandbox, failure.detail)
        logger.info("restored forwards for %d sandbox(es)", len(stored))

    # -- read -------------------------------------------------------------------------------
    def status(self, sandbox: str | None = None) -> list[dict[str, Any]]:
        rows = []
        for port, item in sorted(self.listeners.items()):
            if sandbox is not None and item.sandbox != sandbox:
                continue
            rows.append(
                {
                    "sandbox": item.sandbox,
                    "port": port,
                    "target_port": item.spec.target_port,
                    "purpose": item.spec.purpose,
                    "listening": item.server is not None,
                    "target_up": target_listening(item.sandbox, item.spec.target_port),
                    "error": item.error,
                    "connections": item.connections,
                    "bytes_in": item.bytes_in,
                    "bytes_out": item.bytes_out,
                }
            )
        return rows


manager = Manager()
_client: Any = None


def docker_client() -> Any:
    global _client
    if _client is None:
        _client = docker.from_env()
    return _client


def sandbox_address(sandbox: str) -> str | None:
    """The sandbox's address on the runtime network, read fresh each time.

    Never cached: a restarted container gets a new address, and a relay that
    remembered the old one would fail until the forward was re-applied.
    """
    try:
        container = docker_client().containers.get(f"{CONTAINER_PREFIX}{sandbox}")
    except (NotFound, DockerException):
        return None
    if container.status != "running":
        return None
    networks = (container.attrs.get("NetworkSettings") or {}).get("Networks") or {}
    entry = networks.get(NETWORK) or next(iter(networks.values()), None)
    address = (entry or {}).get("IPAddress")
    return address or None


def target_listening(sandbox: str, port: int) -> bool:
    """Whether something accepts a connection at the sandbox's port right now."""
    address = sandbox_address(sandbox)
    if address is None:
        return False
    try:
        with socket.create_connection((address, port), timeout=PROBE_SECONDS):
            return True
    except OSError:
        return False


async def relay(listener: Listener, reader: asyncio.StreamReader, writer: asyncio.StreamWriter) -> None:
    """Pipe one client connection to the sandbox, and back."""
    listener.connections += 1
    address = await asyncio.to_thread(sandbox_address, listener.sandbox)
    if address is None:
        writer.close()
        return
    try:
        upstream_reader, upstream_writer = await asyncio.wait_for(
            asyncio.open_connection(address, listener.spec.target_port), timeout=CONNECT_SECONDS
        )
    except (OSError, asyncio.TimeoutError):
        writer.close()
        return

    async def pipe(source: asyncio.StreamReader, sink: asyncio.StreamWriter, direction: str) -> None:
        try:
            while chunk := await source.read(RELAY_CHUNK):
                sink.write(chunk)
                await sink.drain()
                if direction == "in":
                    listener.bytes_in += len(chunk)
                else:
                    listener.bytes_out += len(chunk)
        except (ConnectionError, asyncio.CancelledError):
            pass
        finally:
            sink.close()

    await asyncio.gather(
        pipe(reader, upstream_writer, "in"), pipe(upstream_reader, writer, "out"), return_exceptions=True
    )


# -- routes ----------------------------------------------------------------------------------
@router.put("/v1/sandboxes/{name}/forwards")
async def put_forwards(name: str, body: ForwardSet) -> dict[str, Any]:
    import re

    if not re.match(NAME_PATTERN, name):
        raise HTTPException(422, "invalid sandbox name")
    for line in body.authorized_keys:
        if "PRIVATE KEY" in line or not line.strip().startswith(("ssh-", "ecdsa-", "sk-")):
            raise HTTPException(422, "authorized_keys takes public key lines only")
    rows = await manager.apply(name, body)
    return {"sandbox": name, "forwards": rows, "range": list(published_range())}


@router.get("/v1/sandboxes/{name}/forwards")
async def get_forwards(name: str) -> dict[str, Any]:
    rows = await asyncio.to_thread(manager.status, name)
    return {"sandbox": name, "forwards": rows, "range": list(published_range())}


@router.delete("/v1/sandboxes/{name}/forwards")
async def delete_forwards(name: str) -> dict[str, str]:
    await manager.remove(name)
    return {"sandbox": name, "status": "removed"}


@router.get("/v1/forwards")
async def all_forwards() -> dict[str, Any]:
    rows = await asyncio.to_thread(manager.status)
    return {"forwards": rows, "range": list(published_range())}
