"""OpenShell adapter.

Everything the controller knows about OpenShell goes through this module. No view, serializer or
task imports the SDK directly, so an OpenShell 0.1.x breaking change is a one-file problem.

The interface is intentionally narrow: create, stop, delete, exec, list, get_policy, set_policy.
``capabilities`` records what the connected gateway supports.
"""
from __future__ import annotations

import logging
import uuid
from typing import Any, Protocol

from django.conf import settings

logger = logging.getLogger(__name__)


class OpenShellAdapter(Protocol):
    capabilities: dict[str, bool]

    def create_sandbox(self, *, name: str, image: str, workspace: str, pack_key: str) -> dict[str, Any]: ...
    def stop_sandbox(self, identifier: str) -> dict[str, Any]: ...
    def delete_sandbox(self, identifier: str) -> dict[str, Any]: ...
    def list_sandboxes(self) -> list[dict[str, Any]]: ...
    def get_policy(self, name: str) -> str: ...
    def set_policy(self, name: str, yaml_text: str) -> dict[str, Any]: ...


class InMemoryAdapter:
    """Development adapter: records desired state without a gateway.

    Used when the SDK is absent or no gateway is registered. It is honest about it: results carry
    ``simulated: True`` so the UI can say so instead of pretending a nest is running.
    """

    capabilities = {
        "sandboxes": True,
        "policies": True,
        "exec": False,
        "services": False,
        "simulated": True,
    }

    def __init__(self) -> None:
        self._sandboxes: dict[str, dict[str, Any]] = {}
        self._policies: dict[str, str] = {}

    def create_sandbox(self, *, name: str, image: str, workspace: str, pack_key: str) -> dict[str, Any]:
        identifier = f"sim-{uuid.uuid4().hex[:12]}"
        self._sandboxes[identifier] = {"name": name, "image": image, "workspace": workspace, "state": "running"}
        return {
            "ok": True,
            "id": identifier,
            "name": name,
            "state": "running",
            "simulated": True,
            "reason": "no_gateway",
        }

    def stop_sandbox(self, identifier: str) -> dict[str, Any]:
        self._sandboxes.pop(identifier, None)
        return {"ok": True, "simulated": True}

    def delete_sandbox(self, identifier: str) -> dict[str, Any]:
        self._sandboxes.pop(identifier, None)
        return {"ok": True, "simulated": True}

    def list_sandboxes(self) -> list[dict[str, Any]]:
        return list(self._sandboxes.values())

    def get_policy(self, name: str) -> str:
        return self._policies.get(name, "")

    def set_policy(self, name: str, yaml_text: str) -> dict[str, Any]:
        self._policies[name] = yaml_text
        return {"applied": True, "simulated": True, "policy": name}


class SdkAdapter:
    """Adapter over the official Python SDK.

    The SDK is imported lazily: the appliance must start (and the UI must render) even when the
    gateway or the SDK is absent. Phase 2 fills in the calls against the pinned SDK version; the
    capability flags below describe what this build can honestly do.
    """

    capabilities = {
        "sandboxes": False,
        "policies": False,
        "exec": False,
        "services": False,
        "simulated": False,
    }

    def __init__(self) -> None:
        try:
            import openshell  # noqa: F401
        except ImportError as exc:  # pragma: no cover
            raise RuntimeError("OpenShell SDK is not installed.") from exc
        self._endpoint = settings.NESTBOX_OPENSHELL_ENDPOINT
        logger.info("OpenShell adapter initialised for %s", self._endpoint)

    def _not_wired(self) -> dict[str, Any]:
        return {"ok": False, "reason": "adapter_not_wired", "simulated": False}

    def create_sandbox(self, *, name: str, image: str, workspace: str, pack_key: str) -> dict[str, Any]:
        return self._not_wired()

    def stop_sandbox(self, identifier: str) -> dict[str, Any]:
        return self._not_wired()

    def delete_sandbox(self, identifier: str) -> dict[str, Any]:
        return self._not_wired()

    def list_sandboxes(self) -> list[dict[str, Any]]:
        return []

    def get_policy(self, name: str) -> str:
        return ""

    def set_policy(self, name: str, yaml_text: str) -> dict[str, Any]:
        return {"applied": False, "reason": "adapter_not_wired"}


_adapter: OpenShellAdapter | None = None


def get_adapter() -> OpenShellAdapter:
    global _adapter
    if _adapter is not None:
        return _adapter
    mode = settings.NESTBOX_OPENSHELL_ADAPTER
    if mode in {"auto", "sdk"}:
        try:
            _adapter = SdkAdapter()
            return _adapter
        except RuntimeError:
            if mode == "sdk":
                raise
    _adapter = InMemoryAdapter()
    return _adapter
