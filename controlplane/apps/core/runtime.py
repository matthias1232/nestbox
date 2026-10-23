"""Container runtime access.

The appliance controls containers through the local Docker/Podman socket. Everything is wrapped
so a missing socket degrades into a clear ``runtime.unavailable`` error instead of a stack trace
on a developer machine that has no Docker.
"""
from __future__ import annotations

import logging
from collections.abc import Iterator
from datetime import datetime
from typing import Any

from django.conf import settings
from django.utils import timezone

from apps.api.errors import NotFound, RuntimeUnavailable
from apps.containers.models import Container
from apps.core.models import Node

logger = logging.getLogger(__name__)

ROLE_LABEL = "io.nestbox.role"


class DockerRuntime:
    """Thin, defensive wrapper around the Docker SDK."""

    _client = None

    def client(self):  # noqa: ANN201
        if self._client is not None:
            return self._client
        try:
            import docker  # noqa: F401
        except ImportError as exc:  # pragma: no cover
            raise RuntimeUnavailable("Docker SDK is not installed.") from exc
        try:
            self._client = docker.DockerClient(base_url=settings.NESTBOX_DOCKER_SOCKET, timeout=5)
            self._client.ping()
        except Exception as exc:  # noqa: BLE001
            self._client = None
            raise RuntimeUnavailable(
                "Container runtime is not reachable.",
                code="runtime.unavailable",
                details={"socket": settings.NESTBOX_DOCKER_SOCKET, "reason": str(exc)[:200]},
            ) from exc
        return self._client

    def available(self) -> bool:
        try:
            self.client()
            return True
        except RuntimeUnavailable:
            return False

    # -- read ---------------------------------------------------------------------------------
    def list_containers(self) -> list[dict[str, Any]]:
        client = self.client()
        out: list[dict[str, Any]] = []
        for c in client.containers.list(all=True):
            attrs = c.attrs or {}
            state = (attrs.get("State") or {}).get("Status", "unknown")
            health = ((attrs.get("State") or {}).get("Health") or {}).get("Status", "")
            labels = attrs.get("Config", {}).get("Labels") or {}
            ports = []
            for container_port, bindings in (attrs.get("NetworkSettings", {}).get("Ports") or {}).items():
                for b in bindings or []:
                    ports.append(
                        {
                            "container": container_port,
                            "host_ip": b.get("HostIp", "0.0.0.0"),
                            "host_port": b.get("HostPort", ""),
                        }
                    )
            started = (attrs.get("State") or {}).get("StartedAt")
            out.append(
                {
                    "name": (attrs.get("Name") or c.name).lstrip("/"),
                    "container_id": (attrs.get("Id") or "")[:12],
                    "image": attrs.get("Config", {}).get("Image", ""),
                    "image_digest": (attrs.get("Image") or "")[:100],
                    "state": state if state in dict(Container._meta.get_field("state").choices) else "unknown",
                    "health": health,
                    "status_text": (attrs.get("State") or {}).get("Status", ""),
                    "ports": ports,
                    "labels": labels,
                    "role": labels.get(ROLE_LABEL, "other"),
                    "started_at": _parse_ts(started),
                }
            )
        return out

    def sync(self) -> int:
        """Reconcile the container table with the runtime. Returns the number of rows touched."""
        node, _ = Node.objects.get_or_create(name="local", defaults={"is_local": True})
        seen = set()
        touched = 0
        for info in self.list_containers():
            seen.add(info["name"])
            Container.objects.update_or_create(
                node=node,
                name=info["name"],
                defaults={
                    "container_id": info["container_id"],
                    "image": info["image"],
                    "image_digest": info["image_digest"],
                    "role": info["role"],
                    "state": info["state"],
                    "health": info["health"],
                    "status_text": info["status_text"],
                    "ports": info["ports"],
                    "labels": info["labels"],
                    "started_at": info["started_at"],
                },
            )
            touched += 1
        Container.objects.filter(node=node).exclude(name__in=seen).update(state="dead")
        Node.objects.filter(pk=node.pk).update(last_seen=timezone.now())
        return touched

    # -- write --------------------------------------------------------------------------------
    def action(self, name: str, verb: str) -> dict[str, Any]:
        client = self.client()
        try:
            container = client.containers.get(name)
        except Exception as exc:  # noqa: BLE001
            raise NotFound(f"Container {name!r} does not exist.", code="container.not_found") from exc

        if verb == "start":
            container.start()
        elif verb == "stop":
            container.stop(timeout=10)
        elif verb == "restart":
            container.restart(timeout=10)
        elif verb == "kill":
            container.kill()
        else:  # pragma: no cover - guarded by the view
            raise NotFound(f"Unknown action {verb!r}.", code="container.unknown_action")
        container.reload()
        return {"name": name, "state": container.status}

    def logs(self, name: str, tail: int = 200) -> Iterator[str]:
        client = self.client()
        try:
            container = client.containers.get(name)
        except Exception as exc:  # noqa: BLE001
            raise NotFound(f"Container {name!r} does not exist.", code="container.not_found") from exc
        for chunk in container.logs(stream=True, follow=True, tail=tail, timestamps=True):
            yield chunk.decode("utf-8", errors="replace")


def _parse_ts(value: str | None) -> datetime | None:
    if not value or value.startswith("0001-01-01"):
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


runtime = DockerRuntime()
