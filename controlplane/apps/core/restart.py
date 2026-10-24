"""Restart the controller container from inside it.

Module switches only take effect when the process starts again, so the admin offers a button.
The controller talks to the host daemon over the mounted socket; if that socket is absent (a
development checkout), the caller falls back to asking the operator to restart by hand.
"""
from __future__ import annotations

import http.client
import os
import socket
import threading
import time
from pathlib import Path

DOCKER_SOCKET = os.environ.get("NESTBOX_DOCKER_SOCKET", "/var/run/docker.sock")


def available() -> bool:
    return Path(DOCKER_SOCKET).exists()


def _request(method: str, path: str) -> tuple[int, bytes]:
    connection = http.client.HTTPConnection("localhost", timeout=10)
    connection.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.sock.settimeout(10)
    connection.sock.connect(DOCKER_SOCKET)
    connection.request(method, path)
    response = connection.getresponse()
    body = response.read()
    connection.close()
    return response.status, body


def container_id() -> str:
    """Inside a container the hostname is the short container id."""
    return os.environ.get("HOSTNAME", socket.gethostname())


def restart_self(delay: float = 2.0) -> bool:
    """Ask the daemon to restart this container after the response has gone out."""
    if not available():
        return False

    def _restart() -> None:
        time.sleep(delay)
        try:
            _request("POST", f"/containers/{container_id()}/restart?t=5")
        except OSError:
            pass

    threading.Thread(target=_restart, daemon=True).start()
    return True
