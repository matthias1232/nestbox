"""The SSH door into a sandbox.

Sandbox images carry no `sshd`, and giving every image one would mean a daemon,
host keys, capabilities and a user database inside each sandbox. Instead the
runner *is* the SSH server for the port a sandbox was given: a key-authenticated
`ssh -p <port> nestbox@<node>` lands in `docker exec` of the same sandbox, with a
real PTY when the client asks for one. That covers an interactive shell,
`ssh host <command>` (so `ssh host claude` works), and agent CLIs that only need a
terminal.

What it is **not**: an SFTP or port-forwarding server. There is no SFTP
subsystem, and `-L`, `-R` and `-D` are refused, so this door cannot be used to
tunnel traffic around the egress policy. VS Code Remote-SSH needs both and does
not work here; the console says so rather than hiding it. Legacy `scp -O` still
works when the image has `scp`, because it is only a command run inside the
sandbox (`scp -t`), the same as typing it in a shell.

Authentication is public-key only. The authorised keys are the sandbox's own
public key (the pair the control plane generated), delivered with the forward
set; the private half never reaches the runner.
"""
from __future__ import annotations
from app.workload import get_container, list_workloads, sandbox_name

import asyncio
import hashlib
import hmac
import logging
import os
import struct
import time
from pathlib import Path
from typing import Any

logger = logging.getLogger("nestbox.runner.ssh")

CONTAINER_PREFIX = "nestbox-sandbox-"
READ_CHUNK = 65_536
SSH_USER = "nestbox"
TERM_ENV = {"LANG": "C.UTF-8"}


def demux(buffer: bytes) -> tuple[list[tuple[int, bytes]], bytes]:
    """Split Docker's multiplexed exec stream into `(stream, payload)` frames.

    Without a TTY, Docker prefixes every chunk with an 8-byte header: one byte
    for the stream (1 stdout, 2 stderr), three zero bytes, and a big-endian
    length. A chunk can arrive split anywhere, so what is left over is returned
    and joined to the next read.
    """
    frames: list[tuple[int, bytes]] = []
    offset = 0
    while len(buffer) - offset >= 8:
        stream = buffer[offset]
        (length,) = struct.unpack(">I", buffer[offset + 4 : offset + 8])
        end = offset + 8 + length
        if len(buffer) < end:
            break
        frames.append((stream, buffer[offset + 8 : end]))
        offset = end
    return frames, buffer[offset:]


def password_matches(credentials: list[dict[str, Any]], username: str, password: str, now: float | None = None) -> bool:
    """Whether `username`/`password` is a temporary login that is still good.

    The control plane stores `sha256(salt || password)` and sends only that; the
    password is never here. The expiry is checked on every attempt against this
    machine's clock, so a login ends on time even if the control plane is gone.
    Every entry is compared, so the time taken does not say which user names exist.
    """
    moment = time.time() if now is None else now
    ok = False
    for entry in credentials:
        digest = hashlib.sha256(f"{entry.get('salt', '')}{password}".encode()).hexdigest()
        same = hmac.compare_digest(digest, str(entry.get("hash", "")))
        if same and entry.get("username") == username and float(entry.get("expires_at", 0)) > moment:
            ok = True
    return ok


def shell_command(command: str | None) -> list[str]:
    """The argument vector for a login shell or one requested command.

    The command string is the client's own, like any `ssh host <command>`; it is
    handed to `bash -lc` as a single argument and never spliced into another
    string here.
    """
    if command:
        return ["bash", "-lc", command]
    return ["bash", "-l"]


def host_key_path(state_dir: str) -> Path:
    return Path(state_dir) / "ssh_host_ed25519_key"


def ensure_host_key(state_dir: str) -> Path:
    """Create the runner's SSH host key once and keep it across restarts.

    A new key at every start would make every client warn about a changed host
    key, which teaches people to ignore that warning.
    """
    import asyncssh

    path = host_key_path(state_dir)
    if path.exists():
        return path
    path.parent.mkdir(parents=True, exist_ok=True)
    key = asyncssh.generate_private_key("ssh-ed25519", comment="nestbox-runner")
    key.write_private_key(str(path))
    os.chmod(path, 0o600)
    return path


class _Exec:
    """One `docker exec` of a sandbox, as a byte stream."""

    def __init__(self, client: Any, sandbox: str, command: str | None, tty: bool, size: tuple[int, int]):
        from app.terminal import PtySocket

        container = get_container(client, sandbox)
        if container.status != "running":
            raise RuntimeError(f"sandbox '{sandbox}' is {container.status}, not running")
        self._client = client
        created = client.api.exec_create(
            container.id,
            shell_command(command),
            tty=tty,
            stdin=True,
            stdout=True,
            stderr=True,
            environment={**TERM_ENV, "TERM": "xterm-256color"} if tty else TERM_ENV,
        )
        self.exec_id: str = created["Id"]
        self.bytes_in = 0
        self.bytes_out = 0
        self.tty = tty
        self.sock = PtySocket(client.api.exec_start(self.exec_id, tty=tty, socket=True))
        if tty:
            self.resize(*size)

    def resize(self, cols: int, rows: int) -> None:
        self._client.api.exec_resize(self.exec_id, height=max(rows, 1), width=max(cols, 1))

    def exit_code(self) -> int:
        info = self._client.api.exec_inspect(self.exec_id)
        code = info.get("ExitCode")
        return int(code) if code is not None else 0

    def close(self) -> None:
        self.sock.close()


async def _pump(process: Any, run: _Exec) -> None:
    """Move bytes both ways until the command or the client ends."""
    import asyncssh

    async def to_client() -> None:
        pending = b""
        while True:
            chunk = await asyncio.to_thread(run.sock.read, READ_CHUNK)
            if not chunk:
                return
            run.bytes_out += len(chunk)
            if run.tty:
                process.stdout.write(chunk)
                continue
            pending += chunk
            frames, pending = demux(pending)
            for stream, payload in frames:
                (process.stderr if stream == 2 else process.stdout).write(payload)

    async def to_sandbox() -> None:
        while True:
            try:
                data = await process.stdin.read(READ_CHUNK)
            except asyncssh.TerminalSizeChanged as change:
                await asyncio.to_thread(run.resize, change.width, change.height)
                continue
            except (asyncssh.BreakReceived, asyncssh.SignalReceived):
                continue
            if not data:
                # The client closed its input. For a TTY the shell ends with the
                # session; for a plain command this is end-of-file on stdin.
                if not run.tty:
                    try:
                        run.sock._socket.shutdown(1)  # SHUT_WR
                    except OSError:
                        pass
                return
            run.bytes_in += len(data)
            await asyncio.to_thread(run.sock.write, data)

    reader = asyncio.create_task(to_client())
    writer = asyncio.create_task(to_sandbox())
    try:
        await reader
    finally:
        writer.cancel()


def make_process_handler(client: Any, sandbox: str):
    async def handle(process: Any) -> None:
        peer = process.get_extra_info("peername")
        tty = process.get_terminal_type() is not None
        command = process.command
        logger.info("ssh session for %s from %s (tty=%s, command=%s)", sandbox, peer, tty, bool(command))
        cols, rows, _, _ = process.get_terminal_size() if tty else (80, 24, 0, 0)
        try:
            run = await asyncio.to_thread(_Exec, client, sandbox, command, tty, (cols or 80, rows or 24))
        except Exception as failure:  # noqa: BLE001 - every cause ends the session with a sentence
            process.stderr.write(f"nestbox: {failure}\r\n".encode())
            process.exit(1)
            return
        # Who, when, how long and how many bytes go to the control plane's audit
        # table; what was typed or printed never does.
        from app.telemetry_shipper import post_json

        opened = await asyncio.to_thread(post_json, "/api/v1/internal/ssh-sessions", {"event": "open", "sandbox": sandbox})
        session_id = (opened or {}).get("session_id")
        reason = "user"
        try:
            await _pump(process, run)
            process.exit(await asyncio.to_thread(run.exit_code))
        except Exception:  # noqa: BLE001
            logger.exception("ssh session for %s ended with an error", sandbox)
            reason = "error"
            process.exit(1)
        finally:
            run.close()
            logger.info("ssh session for %s closed", sandbox)
            if session_id:
                await asyncio.to_thread(
                    post_json,
                    "/api/v1/internal/ssh-sessions",
                    {"event": "close", "sandbox": sandbox, "session_id": session_id, "reason": reason,
                     "bytes_in": run.bytes_in, "bytes_out": run.bytes_out},
                )

    return handle


async def serve(
    *,
    client: Any,
    sandbox: str,
    bind: str,
    port: int,
    keys_for: Any,
    host_key: Path,
    credentials_for: Any = lambda _sandbox: [],
) -> Any:
    """Start an SSH listener on `port` that opens `sandbox`.

    `keys_for(sandbox)` returns the current authorised-keys text each time a
    client connects, so a rotated key applies to the next login without
    restarting the listener.
    """
    import asyncssh

    class Door(asyncssh.SSHServer):
        def connection_made(self, conn: Any) -> None:
            self._conn = conn

        def begin_auth(self, username: str) -> bool:
            # `True` means "authentication is required". Returning `False` would
            # accept the connection without it, so every path returns `True`
            # and a wrong user simply gets an empty key list.
            text = keys_for(sandbox) if username == SSH_USER else ""
            self._conn.set_authorized_keys(asyncssh.import_authorized_keys(text))
            return True

        def public_key_auth_supported(self) -> bool:
            return True

        def password_auth_supported(self) -> bool:
            # Only a still-valid temporary login passes `validate_password`; with
            # none issued every attempt fails, so offering it costs nothing.
            return True

        async def validate_password(self, username: str, password: str) -> bool:
            good = password_matches(credentials_for(sandbox), username, password)
            if not good:
                # Slows a guesser without making a typo painful.
                await asyncio.sleep(1.0)
            logger.info("password login for %s as %s: %s", sandbox, username, "accepted" if good else "refused")
            return good

        def kbdint_auth_supported(self) -> bool:
            return False

    return await asyncssh.listen(
        bind,
        port,
        server_factory=Door,
        server_host_keys=[str(host_key)],
        process_factory=make_process_handler(client, sandbox),
        encoding=None,
        agent_forwarding=False,
        x11_forwarding=False,
        allow_pty=True,
        line_editor=False,
        # No sftp_factory: SFTP is not offered. `-L`/`-R`/`-D` are
        # refused because `Door` does not override `connection_requested` or
        # `server_requested`, whose default is to say no. This door opens a
        # shell, not a tunnel around the egress policy.
    )
