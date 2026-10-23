"""Nest lifecycle.

Thin orchestration: the controller owns desired state, the OpenShell adapter owns execution.
"""
from __future__ import annotations

from django.db import transaction

from apps.api.errors import Conflict, NotFound
from apps.openshell.adapter import get_adapter
from apps.sandboxes.models import Sandbox, SandboxTemplate


@transaction.atomic
def create_sandbox(*, user, name: str, template: SandboxTemplate | None = None,
                   agent_pack_key: str = "", workspace: str = "default", image: str = "") -> Sandbox:
    if Sandbox.objects.filter(name=name).exists():
        raise Conflict(f"A nest named {name!r} already exists.", code="sandbox.name_taken")

    pack_key = agent_pack_key or (template.agent_pack_key if template else "")
    sandbox = Sandbox.objects.create(
        name=name,
        template=template,
        agent_pack_key=pack_key,
        workspace=workspace,
        image=image or (template.agent_pack_key if template else ""),
        state="pending",
        created_by=user,
    )
    result = get_adapter().create_sandbox(
        name=name, image=sandbox.image, workspace=workspace, pack_key=pack_key
    )
    if result.get("ok"):
        sandbox.openshell_id = result.get("id", "")
        sandbox.overlay_ip = result.get("overlay_ip") or sandbox.overlay_ip
        sandbox.state = result.get("state", "running")
        sandbox.last_error = ""
    else:
        sandbox.state = "failed" if result.get("reason") == "error" else "pending"
        sandbox.last_error = result.get("reason", "")[:500]
    sandbox.save()
    return sandbox


def stop_sandbox(name: str) -> Sandbox:
    sandbox = Sandbox.objects.filter(name=name).first()
    if sandbox is None:
        raise NotFound(f"Nest {name!r} does not exist.", code="sandbox.not_found")
    get_adapter().stop_sandbox(sandbox.openshell_id or sandbox.name)
    sandbox.state = "stopped"
    sandbox.save(update_fields=["state", "updated_at"])
    return sandbox
