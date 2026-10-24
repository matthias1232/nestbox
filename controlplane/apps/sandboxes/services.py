"""Nest lifecycle.

Thin orchestration: the controller owns desired state, the OpenShell adapter owns execution.
"""
from __future__ import annotations

from django.db import transaction

from apps.api.errors import Conflict, NotFound
from apps.agentpacks.models import AgentPack
from apps.openshell.adapter import get_adapter
from apps.sandboxes.models import Sandbox, SandboxTemplate


@transaction.atomic
def create_sandbox(*, user, name: str, template: SandboxTemplate | None = None,
                    agent_pack_key: str = "", workspace: str = "default", image: str = "",
                    port: int = 0, spec: dict | None = None, configuration: dict | None = None,
                    policy=None) -> Sandbox:
    if Sandbox.objects.filter(name=name).exists():
        raise Conflict(f"A nest named {name!r} already exists.", code="sandbox.name_taken")

    from apps.core.nest_sections import validate_spec

    spec_configuration = validate_spec(spec or {})
    pack_key = agent_pack_key or (template.agent_pack_key if template else "")
    pack = AgentPack.objects.filter(key=pack_key).first() if pack_key else None
    runtime_configuration = pack_configuration(pack, configuration)
    sandbox = Sandbox.objects.create(
        name=name,
        template=template,
        agent_pack_key=pack_key,
        workspace=workspace,
        image=image or (pack.image_ref if pack else "") or (template.agent_pack_key if template else ""),
        state="pending",
        port=port,
        spec=spec_configuration,
        configuration=runtime_configuration,
        policy=policy,
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


def pack_configuration(pack: AgentPack | None, supplied: dict | None) -> dict:
    """Merge data-only pack defaults with an operator's nest configuration.

    This is controller metadata, not an attempt to translate configuration into unsupported
    gateway features. In particular, host or filesystem mounts are intentionally unavailable.
    """
    if supplied is not None and not isinstance(supplied, dict):
        raise Conflict("Nest configuration must be an object.", code="sandbox.invalid_configuration")
    result = dict(pack.defaults) if pack and isinstance(pack.defaults, dict) else {}
    result.update(supplied or {})
    forbidden = {key for key in result if key.lower() in {"mount", "mounts", "volumes"}}
    if forbidden:
        raise Conflict("Mount configuration is not supported.", code="sandbox.mounts_unsupported")
    return result


def start_sandbox(name: str) -> Sandbox:
    """Apply the assigned policy before making a stopped nest runnable in controller state."""
    sandbox = Sandbox.objects.select_related("policy").filter(name=name).first()
    if sandbox is None:
        raise NotFound(f"Nest {name!r} does not exist.", code="sandbox.not_found")
    if sandbox.policy is None:
        raise Conflict("Assign a policy before starting this nest.", code="sandbox.policy_required")
    if not sandbox.policy.is_active:
        raise Conflict("The assigned policy is inactive.", code="sandbox.policy_inactive")
    result = get_adapter().set_policy(sandbox.openshell_id or sandbox.name, sandbox.policy.yaml_text)
    if not result.get("ok"):
        raise Conflict("The assigned policy could not be applied.", code="sandbox.policy_apply_failed")
    sandbox.state = "running"
    sandbox.last_error = ""
    sandbox.save(update_fields=["state", "last_error", "updated_at"])
    return sandbox


def stop_sandbox(name: str) -> Sandbox:
    sandbox = Sandbox.objects.filter(name=name).first()
    if sandbox is None:
        raise NotFound(f"Nest {name!r} does not exist.", code="sandbox.not_found")
    get_adapter().stop_sandbox(sandbox.openshell_id or sandbox.name)
    sandbox.state = "stopped"
    sandbox.save(update_fields=["state", "updated_at"])
    return sandbox
