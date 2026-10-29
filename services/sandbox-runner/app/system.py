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
from typing import Any

import docker
from docker.errors import DockerException
from fastapi import APIRouter, HTTPException

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


def describe(container: Any | None, service: str) -> dict[str, Any]:
    base: dict[str, Any] = {"service": service, "description": COMPONENTS[service]}
    if container is None:
        return {**base, "state": "missing", "health": None, "container": None}
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
    }


@router.get("/v1/system/components")
def components() -> dict[str, Any]:
    try:
        client = _client()
        project = own_project(client)
        rows = [describe(_find(client, project, service), service) for service in COMPONENTS]
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
