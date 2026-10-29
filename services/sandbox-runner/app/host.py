"""The host's load, and changing a running sandbox's resources.

Two things an operator asks for while sandboxes work:

* *How loaded is this machine?* CPU, memory, disk and load average of the **host**
  (the runner is given the host's `/proc`), plus what the sandboxes have been
  promised, so "free" and "promised" can be told apart.
* *Give this sandbox more (or less).* `docker update` changes memory, CPU and the
  process limit of a running container without restarting it. Mounts cannot be
  changed that way; that is a different operation (see the persistent roots).
"""
from __future__ import annotations

import os
import re
import shutil
import time
from typing import Any

import docker
from docker.errors import APIError, NotFound
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

router = APIRouter()
HOST_PROC = os.getenv("NESTBOX_HOST_PROC", "/proc")
PREFIX = "nestbox-sandbox-"

_last_cpu: tuple[float, int, int] | None = None


def _read(path: str) -> str:
    try:
        with open(path, encoding="utf-8") as handle:
            return handle.read()
    except OSError:
        return ""


def parse_cpu_times(stat_text: str) -> tuple[int, int] | None:
    """(busy, total) jiffies from the first line of /proc/stat."""
    for line in stat_text.splitlines():
        if line.startswith("cpu "):
            fields = [int(value) for value in line.split()[1:] if value.isdigit()]
            if len(fields) < 4:
                return None
            idle = fields[3] + (fields[4] if len(fields) > 4 else 0)  # idle + iowait
            total = sum(fields[:8]) if len(fields) >= 8 else sum(fields)
            return total - idle, total
    return None


def cpu_percent(previous: tuple[int, int], current: tuple[int, int]) -> float:
    busy = current[0] - previous[0]
    total = current[1] - previous[1]
    if total <= 0:
        return 0.0
    return round(max(0.0, min(100.0, busy / total * 100.0)), 1)


def parse_meminfo(text: str) -> dict[str, int]:
    out: dict[str, int] = {}
    for line in text.splitlines():
        match = re.match(r"^(\w+):\s+(\d+)\s*kB", line)
        if match:
            out[match.group(1)] = int(match.group(2)) * 1024
    return out


def sample_cpu() -> float | None:
    """CPU use since the last call; a first call measures over a short interval."""
    global _last_cpu
    now = parse_cpu_times(_read(os.path.join(HOST_PROC, "stat")))
    if now is None:
        return None
    previous = _last_cpu
    if previous is None or time.time() - previous[0] > 120:
        time.sleep(0.25)
        later = parse_cpu_times(_read(os.path.join(HOST_PROC, "stat")))
        if later is None:
            return None
        _last_cpu = (time.time(), later[0], later[1])
        return cpu_percent(now, later)
    _last_cpu = (time.time(), now[0], now[1])
    return cpu_percent((previous[1], previous[2]), now)


@router.get("/v1/host/load")
def host_load() -> dict[str, Any]:
    memory = parse_meminfo(_read(os.path.join(HOST_PROC, "meminfo")))
    total = memory.get("MemTotal", 0)
    available = memory.get("MemAvailable", memory.get("MemFree", 0))
    swap_total = memory.get("SwapTotal", 0)
    cpus = os.cpu_count() or 1
    load = _read(os.path.join(HOST_PROC, "loadavg")).split()
    uptime = _read(os.path.join(HOST_PROC, "uptime")).split()
    try:
        disk = shutil.disk_usage("/var/lib/nestbox/persistent" if os.path.isdir("/var/lib/nestbox/persistent") else "/")
        disk_info = {"total": disk.total, "used": disk.used, "free": disk.free}
    except OSError:
        disk_info = None

    promised_memory = 0
    promised_cpu = 0.0
    running = 0
    total_sandboxes = 0
    try:
        client = docker.from_env()
        for container in client.containers.list(all=True):
            if not container.name.startswith(PREFIX):
                continue
            total_sandboxes += 1
            if container.status != "running":
                continue
            running += 1
            host_config = container.attrs.get("HostConfig") or {}
            promised_memory += int(host_config.get("Memory") or 0)
            promised_cpu += (host_config.get("NanoCpus") or 0) / 1_000_000_000
    except Exception:  # noqa: BLE001 - load is still worth showing without the Docker part
        pass

    return {
        "cpu_percent": sample_cpu(),
        "cpus": cpus,
        "load_average": [float(value) for value in load[:3]] if len(load) >= 3 else None,
        "memory": {
            "total": total,
            "available": available,
            "used": max(total - available, 0),
            "swap_total": swap_total,
        },
        "disk": disk_info,
        "uptime_seconds": float(uptime[0]) if uptime else None,
        "sandboxes": {"total": total_sandboxes, "running": running},
        "promised": {"memory_bytes": promised_memory, "cpus": round(promised_cpu, 2)},
    }


class ResourceChange(BaseModel):
    # Bytes. The control plane has already normalised and checked the figure.
    memory_bytes: int | None = Field(default=None, ge=16 * 1024 * 1024)
    cpus: float | None = Field(default=None, gt=0, le=64)
    pids_limit: int | None = Field(default=None, ge=32, le=65536)


@router.patch("/v1/sandboxes/{name}/resources")
def change_resources(name: str, body: ResourceChange) -> dict[str, Any]:
    """Change a running sandbox's limits without restarting it."""
    client = docker.from_env()
    try:
        container = client.containers.get(f"{PREFIX}{name}")
    except NotFound as exc:
        raise HTTPException(404, "sandbox not found") from exc
    changes: dict[str, Any] = {}
    if body.memory_bytes is not None:
        # Memory and memory+swap are set together: Docker refuses a memory limit
        # above the existing swap limit, and a sandbox has no swap.
        changes["mem_limit"] = body.memory_bytes
        changes["memswap_limit"] = body.memory_bytes
    if body.cpus is not None:
        changes["nano_cpus"] = int(body.cpus * 1_000_000_000)
    if body.pids_limit is not None:
        changes["pids_limit"] = body.pids_limit
    if not changes:
        raise HTTPException(422, "nothing to change")
    try:
        container.update(**changes)
    except APIError as exc:
        raise HTTPException(409, f"Docker refused the change: {exc.explanation or exc}") from exc
    container.reload()
    host_config = container.attrs.get("HostConfig") or {}
    return {
        "name": name,
        "memory_bytes": host_config.get("Memory"),
        "cpus": (host_config.get("NanoCpus") or 0) / 1_000_000_000,
        "pids_limit": host_config.get("PidsLimit"),
    }
