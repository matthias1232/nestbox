"""Resource samples for one sandbox.

One sample is one `docker stats` one-shot: no stream, no held socket, no state
on the runner. That is deliberate — a Docker stats stream per sandbox is a
connection per sandbox that has to survive a runner restart, and the console only
needs a point every five seconds.

The arithmetic is Docker's own: CPU is a share of all host cores (a container
pinned to one core of four reads 25 at full load), memory subtracts the page
cache, network and block I/O are cumulative counters, so two samples are needed
to draw a rate.

This module owns its own Docker client rather than reaching into `app.main`, so
the Admin Center packages can change one file without touching the request path
that already exists.
"""
from __future__ import annotations

import json
import os
import threading
import time
import urllib.request
from typing import Any

import docker
from docker.errors import NotFound
from fastapi import APIRouter, HTTPException

router = APIRouter()
client = docker.from_env()

# How often the sampler wakes up. The control plane keeps raw points for a day,
# so this is also the shortest resolution the console can ever draw.
SAMPLE_INTERVAL_SECONDS = float(os.getenv("NESTBOX_METRICS_INTERVAL", "5"))
# A single sample can be slow on a loaded host; a stuck one must not pile up.
SAMPLE_TIMEOUT_SECONDS = 10.0


def container(name: str) -> Any:
    """Look up a managed container or answer 404, never a traceback."""
    try:
        return client.containers.get(f"nestbox-sandbox-{name}")
    except NotFound as exc:
        raise HTTPException(404, f"no such sandbox: {name}") from exc


def _cpu_percent(stats: dict[str, Any]) -> float:
    cpu = stats.get("cpu_stats") or {}
    pre = stats.get("precpu_stats") or {}
    total = (cpu.get("cpu_usage") or {}).get("total_usage") or 0
    previous = (pre.get("cpu_usage") or {}).get("total_usage") or 0
    system = cpu.get("system_cpu_usage") or 0
    system_previous = pre.get("system_cpu_usage") or 0
    delta = total - previous
    system_delta = system - system_previous
    if delta <= 0 or system_delta <= 0:
        return 0.0
    online = cpu.get("online_cpus") or len((cpu.get("cpu_usage") or {}).get("percpu_usage") or []) or 1
    return round(delta / system_delta * online * 100.0, 3)


def _memory(stats: dict[str, Any]) -> tuple[int, int]:
    memory = stats.get("memory_stats") or {}
    usage = int(memory.get("usage") or 0)
    # cgroup v1 reports the page cache in `cache`, v2 in `inactive_file`.
    cache = int((memory.get("stats") or {}).get("cache") or (memory.get("stats") or {}).get("inactive_file") or 0)
    limit = int(memory.get("limit") or 0)
    return max(0, usage - cache), limit


def _network(stats: dict[str, Any]) -> tuple[int, int]:
    rx = tx = 0
    for interface in (stats.get("networks") or {}).values():
        rx += int(interface.get("rx_bytes") or 0)
        tx += int(interface.get("tx_bytes") or 0)
    return rx, tx


def _block_io(stats: dict[str, Any]) -> tuple[int, int]:
    read = write = 0
    for entry in (stats.get("blkio_stats") or {}).get("io_service_bytes_recursive") or []:
        op = (entry.get("op") or "").lower()
        value = int(entry.get("value") or 0)
        if op == "read":
            read += value
        elif op == "write":
            write += value
    return read, write


def sample(name: str) -> dict[str, Any]:
    """One point for one sandbox, in the shape the control plane stores."""
    item = container(name)
    raw = item.stats(stream=False)
    mem_bytes, mem_limit = _memory(raw)
    net_rx, net_tx = _network(raw)
    blk_read, blk_write = _block_io(raw)
    return {
        "sandbox": name,
        "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "cpu_pct": _cpu_percent(raw),
        "mem_bytes": mem_bytes,
        "mem_limit": mem_limit,
        "net_rx": net_rx,
        "net_tx": net_tx,
        "blk_read": blk_read,
        "blk_write": blk_write,
        "pids": int((raw.get("pids_stats") or {}).get("current") or 0),
    }


def managed_sandboxes() -> list[str]:
    """Every sandbox container this runner knows, by short name."""
    return [
        item.name.removeprefix("nestbox-sandbox-")
        for item in client.containers.list(filters={"label": "nestbox.managed=true"})
    ]


@router.get("/v1/sandboxes/{name}/stats")
def stats(name: str) -> dict[str, Any]:
    """One sample, for the console's "now" number and for tests."""
    return sample(name)


def poster_loop(url: str, token: str, interval: float = SAMPLE_INTERVAL_SECONDS) -> None:
    """Sample every managed sandbox and post one batch to the control plane.

    Started only when `NESTBOX_METRICS_POST_URL` is set, so a runner without a
    control plane (a test, a bare install) does not log a failed request every
    five seconds. A batch that fails is dropped rather than retried: the next
    batch is five seconds away and a queue of stale points is worse than a gap.
    """
    while True:
        started = time.monotonic()
        try:
            samples = [sample(name) for name in managed_sandboxes()]
            if samples:
                body = json.dumps({"samples": samples}).encode()
                request = urllib.request.Request(
                    url,
                    data=body,
                    method="POST",
                    headers={"Content-Type": "application/json", "Authorization": f"Bearer {token}"},
                )
                with urllib.request.urlopen(request, timeout=SAMPLE_TIMEOUT_SECONDS) as answer:
                    answer.read()
        except HTTPException:
            # A sandbox disappeared between the list and the sample. Normal.
            pass
        except Exception:  # noqa: BLE001 - the sampler must never die
            pass
        time.sleep(max(0.0, interval - (time.monotonic() - started)))


def _start_if_configured() -> None:
    url = os.getenv("NESTBOX_METRICS_POST_URL")
    if not url:
        return
    token = os.getenv("NESTBOX_NODE_TOKEN", "")
    threading.Thread(target=poster_loop, args=(url, token), daemon=True, name="metrics-poster").start()


_start_if_configured()
