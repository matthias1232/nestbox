"""The appliance's own containers: their state and their logs.

Sandbox logs have always been readable; the components around them were not. A
stack diagram that shows a gateway, an egress proxy and a control API is only
useful if clicking one of them answers "is it up, and what did it say", so this
module reports exactly that for the Compose services of the project the runner
itself belongs to.

Two rules:

1. **A fixed list.** Only the services named below can be read. The runner holds
   the Docker socket; a route that took a container name from the request would
   be a way to read any container on the host (a database, another tenant's
   model server).
2. **Only this project.** A host can run two Compose projects with the same
   service names (a test stack next to the real one). The runner finds its own
   project from its own container labels and never reads another's.
"""
from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import docker
from docker.errors import DockerException
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

router = APIRouter()

PROJECT_LABEL = "com.docker.compose.project"
SERVICE_LABEL = "com.docker.compose.service"

#: service -> what the operator should know it is for.
COMPONENTS: dict[str, str] = {
    "control-api": "The control plane: API, policy decisions, audit, the only writer of the database.",
    "web-console": "The browser console you are looking at, served by nginx.",
    "gateway": "Front door for the control plane: terminates requests and writes the access log.",
    "sandbox-runner": "The only component with the Docker socket. Starts sandboxes, publishes their ports, opens SSH.",
    "egress-proxy": "The only way out of the sandbox network. Asks the control plane per request; default deny.",
    "mcp-hub": "The MCP tool hub sandboxes reach as host.openshell.internal; keeps the tool servers agents call.",
    "postgres": "The single control store.",
}

MAX_TAIL = 1000


def _client() -> Any:
    return docker.from_env()


def own_project(client: Any) -> str | None:
    """The Compose project this runner belongs to, from its own container."""
    hostname = os.getenv("HOSTNAME", "")
    if not hostname:
        return None
    try:
        return client.containers.get(hostname).labels.get(PROJECT_LABEL)
    except DockerException:
        return None


def _find(client: Any, project: str | None, service: str) -> Any | None:
    filters = {"label": [f"{SERVICE_LABEL}={service}"]}
    if project:
        filters["label"].append(f"{PROJECT_LABEL}={project}")
    found = client.containers.list(all=True, filters=filters)
    return found[0] if found else None


def _stats(container: Any) -> dict[str, Any]:
    """One sample of the container's own use, plus the limits it runs under.

    `docker stats` with `stream=False` blocks for about a second, which is why
    this is only asked for on the components page (a handful of containers), not
    on every list. A container that cannot be sampled reports no numbers rather
    than zeroes: zero would read as "idle" and that is a different statement.
    """
    try:
        raw = container.stats(stream=False)
    except Exception:  # noqa: BLE001 - sampling must never break the page
        return {}
    cpu = raw.get("cpu_stats") or {}
    pre = raw.get("precpu_stats") or {}
    cpu_delta = (cpu.get("cpu_usage") or {}).get("total_usage", 0) - (pre.get("cpu_usage") or {}).get("total_usage", 0)
    system_delta = cpu.get("system_cpu_usage", 0) - pre.get("system_cpu_usage", 0)
    cores = cpu.get("online_cpus") or len((cpu.get("cpu_usage") or {}).get("percpu_usage") or []) or 1
    memory = raw.get("memory_stats") or {}
    return {
        "cpu_pct": round((cpu_delta / system_delta) * cores * 100, 2) if system_delta > 0 else None,
        "mem_bytes": memory.get("usage"),
        "mem_limit_bytes": memory.get("limit"),
        "pids": (raw.get("pids_stats") or {}).get("current"),
    }


def describe(container: Any | None, service: str) -> dict[str, Any]:
    base: dict[str, Any] = {"service": service, "description": COMPONENTS[service]}
    if container is None:
        return {**base, "state": "missing", "health": None, "container": None}
    # A just-updated container still reports the limits it was started with in
    # the attributes this process cached; reload so the page shows what is true
    # now rather than what was true before the last change.
    try:
        container.reload()
    except Exception:  # noqa: BLE001 - a vanished container is described as before
        pass
    state = (container.attrs.get("State") or {})
    ports = []
    for target, bindings in ((container.attrs.get("NetworkSettings") or {}).get("Ports") or {}).items():
        for binding in bindings or []:
            ports.append({"container": target, "host": binding.get("HostIp"), "host_port": binding.get("HostPort")})
    return {
        **base,
        "container": container.name,
        "image": (container.attrs.get("Config") or {}).get("Image"),
        "state": state.get("Status", "unknown"),
        "health": (state.get("Health") or {}).get("Status"),
        "started_at": state.get("StartedAt"),
        "restarts": container.attrs.get("RestartCount", 0),
        "exit_code": state.get("ExitCode"),
        "ports": ports,
        # What it is using now, and what it is allowed to use. Both matter: a
        # gateway at 900 MB of a 1 GB limit is a different problem from one at
        # 900 MB of 8 GB.
        **_stats(container),
        "limits": {
            "memory_limit_bytes": (container.attrs.get("HostConfig") or {}).get("Memory") or None,
            "nano_cpus": (container.attrs.get("HostConfig") or {}).get("NanoCpus") or None,
            "cpu_shares": (container.attrs.get("HostConfig") or {}).get("CpuShares") or None,
            "pids_limit": (container.attrs.get("HostConfig") or {}).get("PidsLimit") or None,
        },
    }


@router.get("/v1/system/components")
def components() -> dict[str, Any]:
    """Every component, with one live sample each.

    The samples are taken **concurrently**: `docker stats` blocks for about a
    second per container, and six of them one after another exceeded the control
    plane's four-second budget, which turned a healthy page into a 500. In
    parallel the whole answer stays inside a second or two.
    """
    try:
        client = _client()
        project = own_project(client)
        found = {service: _find(client, project, service) for service in COMPONENTS}

        def sample(service: str) -> dict[str, Any]:
            return describe(found[service], service)

        with ThreadPoolExecutor(max_workers=len(COMPONENTS)) as pool:
            rows = list(pool.map(sample, COMPONENTS))
    except DockerException as failure:
        raise HTTPException(503, f"docker unavailable: {failure}") from failure
    return {"project": project, "components": rows}


@router.get("/v1/system/components/{service}/logs")
def component_logs(service: str, tail: int = 200) -> dict[str, Any]:
    if service not in COMPONENTS:
        raise HTTPException(404, "unknown component")
    if not 1 <= tail <= MAX_TAIL:
        raise HTTPException(422, f"tail must be between 1 and {MAX_TAIL}")
    try:
        client = _client()
        container = _find(client, own_project(client), service)
        if container is None:
            raise HTTPException(404, "this component has no container in this project")
        text = container.logs(tail=tail, timestamps=True).decode(errors="replace")
    except DockerException as failure:
        raise HTTPException(503, f"docker unavailable: {failure}") from failure
    return {"service": service, "tail": tail, "logs": text}


#: Components whose restart would cut the request that asks for it. The console
#: refuses these too; the runner refuses them again because it is a separate
#: security boundary, and a self-restart would answer nothing.
NO_SELF_RESTART = {"control-api", "sandbox-runner", "postgres"}

#: Below this, a component cannot do its job; the platform would look "edited"
#: while being broken.
MIN_MEMORY_BYTES = 64 * 1024 * 1024


class ComponentLimits(BaseModel):
    """Limits a person may change on a platform container.

    Deliberately narrow: no device requests, no privileged mode, no mounts. This
    is resource tuning of components that already exist, not a way to build a
    new container through the console.
    """

    memory_limit_bytes: int | None = Field(default=None, ge=MIN_MEMORY_BYTES, le=1 << 46)
    nano_cpus: int | None = Field(default=None, ge=100_000_000, le=1 << 40)
    cpu_shares: int | None = Field(default=None, ge=2, le=262_144)
    pids_limit: int | None = Field(default=None, ge=16, le=1_000_000)


def _component(service: str) -> Any:
    if service not in COMPONENTS:
        raise HTTPException(404, "unknown component")
    try:
        client = _client()
        container = _find(client, own_project(client), service)
    except DockerException as failure:
        raise HTTPException(503, f"docker unavailable: {failure}") from failure
    if container is None:
        raise HTTPException(404, "this component has no container in this project")
    return container


@router.put("/v1/system/components/{service}/limits")
def set_limits(service: str, body: ComponentLimits) -> dict[str, Any]:
    """Apply CPU/memory/process limits to one platform container, live.

    Docker applies memory and CPU limits to a running container without a
    restart; the answer says which happened so the console does not claim a
    change it did not make.
    """
    container = _component(service)
    update: dict[str, Any] = {}
    if body.memory_limit_bytes is not None:
        update["mem_limit"] = body.memory_limit_bytes
    if body.nano_cpus is not None:
        update["nano_cpus"] = body.nano_cpus
    if body.cpu_shares is not None:
        update["cpu_shares"] = body.cpu_shares
    if body.pids_limit is not None:
        update["pids_limit"] = body.pids_limit
    if not update:
        raise HTTPException(422, "nothing to change: name a memory, CPU or process limit")
    # Docker refuses a memory limit above the container's swap limit, so a
    # lowered memory limit has to move the swap limit with it. Both are set to the
    # same number, which is "no swap": the honest limit for a component whose job
    # is to answer quickly, not to swap.
    if "mem_limit" in update:
        update["memswap_limit"] = update["mem_limit"]
    try:
        # Straight to the engine, with the engine's own field names. docker-py
        # 7.2's `update_container` helper has neither `nano_cpus` nor
        # `pids_limit`, so half of this form used to fail with a TypeError that
        # reached the operator as "Internal Server Error". The engine accepts all
        # of them, and this is the documented endpoint behind the helper.
        api = container.client.api
        api._post_json(
            api._url("/containers/{0}/update", container.id),
            # Only the fields that were asked for: an explicit 0 would remove a
            # limit the caller never mentioned.
            data={
                engine: update[key]
                for key, engine in (
                    ("mem_limit", "Memory"),
                    ("memswap_limit", "MemorySwap"),
                    ("nano_cpus", "NanoCpus"),
                    ("cpu_shares", "CpuShares"),
                    ("pids_limit", "PidsLimit"),
                )
                if key in update
            },
        )
    except DockerException as failure:
        raise HTTPException(409, f"docker refused the change: {failure}") from failure
    except Exception as failure:  # noqa: BLE001 - a bad field must be a clear 409, not a 500
        raise HTTPException(409, f"docker refused the change: {failure}") from failure
    return {"service": service, "applied": "live", "changed": sorted(update)}


@router.post("/v1/system/components/{service}/restart")
def restart(service: str) -> dict[str, Any]:
    """Restart one platform container.

    Says beforehand what it will interrupt, because the honest answer to
    "restart the gateway" is "every sandbox request for a moment", not "done".
    """
    if service in NO_SELF_RESTART:
        raise HTTPException(
            409,
            f"{service} cannot be restarted from the console: the request that asks for it "
            "would be cut off before it could answer. Restart it from a shell on the node.",
        )
    container = _component(service)
    try:
        container.restart(timeout=20)
    except DockerException as failure:
        raise HTTPException(409, f"docker refused the restart: {failure}") from failure
    return {"service": service, "state": "restarting"}
