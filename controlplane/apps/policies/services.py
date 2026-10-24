"""Policy validation, rendering, revisions and application.

There is one authority per policy, recorded in ``Policy.yaml_source``:

* ``structured`` — the rule rows are edited; this module renders the YAML document from them.
* ``raw`` — the operator edits the document; it is stored byte-for-byte and only parsed for display.

Both paths validate the OpenShell document, create an immutable revision of the *previous* version
and write one audit event, inside the same transaction. Nothing here claims that a gateway has
applied a document; :func:`apply_policy_to_gateway` reports exactly what the adapter returned and
whether the change is limited to the hot-reloadable network section.
"""
from __future__ import annotations

import difflib
from collections.abc import Iterable, Mapping
from typing import Any

import yaml
from django.db import transaction
from django.db.models import Model

from apps.identity.services import audit
from apps.policies import presets, render
from apps.policies.models import (
    FilesystemRule,
    LandlockRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    PolicyTemplate,
    ProcessRule,
)

REQUIRED_SECTIONS = render.KNOWN_SECTIONS
STATIC_SECTIONS = ("filesystem_policy", "landlock", "process")
NETWORK_SECTION = "network_policies"
LANDLOCK_COMPATIBILITY = {"strict", "best_effort"}
NETWORK_ACCESS = {"allow", "deny"}
NETWORK_ENFORCEMENT = {"enforced", "audit"}


class PolicyValidationError(ValueError):
    """Raised when a document or structured payload cannot be saved."""

    def __init__(self, result: dict[str, Any]) -> None:
        self.result = result
        super().__init__("; ".join(result.get("errors") or ["invalid policy"]))


# --- validation -------------------------------------------------------------------------------

def _is_list(value: Any) -> bool:
    return isinstance(value, list)


def _is_mapping(value: Any) -> bool:
    return isinstance(value, Mapping)


def _unknown_keys(value: Mapping[str, Any], allowed: Iterable[str], where: str, warnings: list[str]) -> None:
    unknown = sorted(set(value) - set(allowed))
    if unknown:
        warnings.append(
            f"{where} contains fields this editor does not model ({', '.join(unknown)}); "
            "they are kept but can only be changed in raw mode."
        )


def _check_string_list(value: Any, where: str, errors: list[str]) -> None:
    if value is None:
        return
    if not _is_list(value):
        errors.append(f"{where} must be a list.")
        return
    for index, item in enumerate(value):
        if not isinstance(item, str) or not item:
            errors.append(f"{where}[{index}] must be a non-empty string.")


def validate_document(document: Any) -> dict[str, Any]:
    """Validate a parsed OpenShell policy document.

    The checks follow ``docs/policies/network-rules.md`` and the adapter contract. Unknown fields are
    warnings, never errors: OpenShell may support options this editor does not understand, and a raw
    document must survive validation unchanged.
    """
    errors: list[str] = []
    warnings: list[str] = []

    if not _is_mapping(document):
        return {"valid": False, "errors": ["Policy must be a mapping at the top level."], "warnings": []}

    for section in REQUIRED_SECTIONS:
        if section not in document:
            warnings.append(f"Missing section {section!r}; the gateway will apply its default.")
    _unknown_keys(document, REQUIRED_SECTIONS, "The document", warnings)

    filesystem = document.get("filesystem_policy")
    if filesystem is not None and not _is_mapping(filesystem):
        errors.append("filesystem_policy must be a mapping.")
    elif filesystem is not None:
        _unknown_keys(filesystem, ("include_workdir", "read_only", "read_write"), "filesystem_policy", warnings)
        include_workdir = filesystem.get("include_workdir")
        if include_workdir is not None and not isinstance(include_workdir, bool):
            errors.append("filesystem_policy.include_workdir must be true or false.")
        _check_string_list(filesystem.get("read_only"), "filesystem_policy.read_only", errors)
        _check_string_list(filesystem.get("read_write"), "filesystem_policy.read_write", errors)

    landlock = document.get("landlock")
    if landlock is not None and not _is_mapping(landlock):
        errors.append("landlock must be a mapping.")
    elif landlock is not None:
        _unknown_keys(landlock, ("compatibility",), "landlock", warnings)
        compatibility = landlock.get("compatibility")
        if compatibility is not None and compatibility not in LANDLOCK_COMPATIBILITY:
            errors.append("landlock.compatibility must be 'strict' or 'best_effort'.")
        if "include_workdir" in landlock:
            warnings.append(
                "landlock.include_workdir is not an OpenShell field; use "
                "filesystem_policy.include_workdir."
            )

    process = document.get("process")
    if process is not None and not _is_mapping(process):
        errors.append("process must be a mapping.")
    elif process is not None:
        _unknown_keys(process, ("run_as_user", "run_as_group"), "process", warnings)
        for key in ("run_as_user", "run_as_group"):
            value = process.get(key)
            if value is not None and (not isinstance(value, str) or not value.strip()):
                errors.append(f"process.{key} must be a non-empty string.")

    network = document.get(NETWORK_SECTION)
    if network is not None and not _is_mapping(network):
        errors.append("network_policies must be a mapping of block name to rules.")
    elif network is not None:
        for block_name, block in network.items():
            where = f"network_policies.{block_name}"
            if not _is_mapping(block):
                errors.append(f"{where} must be a mapping.")
                continue
            _unknown_keys(block, ("endpoints", "binaries"), where, warnings)
            endpoints = block.get("endpoints")
            if endpoints is not None and not _is_list(endpoints):
                errors.append(f"{where}.endpoints must be a list.")
                endpoints = []
            for index, endpoint in enumerate(endpoints or []):
                ep_where = f"{where}.endpoints[{index}]"
                if not _is_mapping(endpoint):
                    errors.append(f"{ep_where} must be a mapping.")
                    continue
                _unknown_keys(
                    endpoint,
                    ("host", "port", "protocol", "enforcement", "access", "methods"),
                    ep_where,
                    warnings,
                )
                host = endpoint.get("host")
                if not isinstance(host, str) or not host.strip():
                    errors.append(f"{ep_where}.host must be a non-empty string.")
                port = endpoint.get("port")
                if isinstance(port, bool) or not isinstance(port, int) or not 1 <= port <= 65535:
                    errors.append(f"{ep_where}.port must be an integer between 1 and 65535.")
                protocol = endpoint.get("protocol")
                if not isinstance(protocol, str) or not protocol.strip():
                    errors.append(f"{ep_where}.protocol must be a non-empty string.")
                access = endpoint.get("access")
                if access is not None and access not in NETWORK_ACCESS:
                    errors.append(f"{ep_where}.access must be 'allow' or 'deny'.")
                enforcement = endpoint.get("enforcement")
                if enforcement is not None and enforcement not in NETWORK_ENFORCEMENT:
                    errors.append(f"{ep_where}.enforcement must be 'enforced' or 'audit'.")
                _check_string_list(endpoint.get("methods"), f"{ep_where}.methods", errors)

            binaries = block.get("binaries")
            if binaries is not None and not _is_list(binaries):
                errors.append(f"{where}.binaries must be a list.")
                binaries = []
            for index, binary in enumerate(binaries or []):
                b_where = f"{where}.binaries[{index}]"
                if isinstance(binary, str) and binary:
                    warnings.append(f"{b_where} is a bare path; OpenShell expects a mapping with 'path'.")
                    continue
                if not _is_mapping(binary):
                    errors.append(f"{b_where} must be a mapping with a 'path'.")
                    continue
                _unknown_keys(binary, ("path",), b_where, warnings)
                path = binary.get("path")
                if not isinstance(path, str) or not path.strip():
                    errors.append(f"{b_where}.path must be a non-empty string.")

    return {"valid": not errors, "errors": errors, "warnings": warnings}


def validate_policy_yaml(text: str) -> dict[str, Any]:
    """Validate policy YAML and return ``valid``, ``errors`` and ``warnings``."""
    if not text or not text.strip():
        return {"valid": False, "errors": ["Policy document is empty."], "warnings": []}
    try:
        document = yaml.safe_load(text)
    except yaml.YAMLError as exc:
        return {"valid": False, "errors": [f"YAML parse error: {exc}"], "warnings": []}
    return validate_document(document)


def require_valid_yaml(text: str) -> dict[str, Any]:
    result = validate_policy_yaml(text)
    if not result["valid"]:
        raise PolicyValidationError(result)
    return result


def require_valid_document(document: Any) -> dict[str, Any]:
    result = validate_document(document)
    if not result["valid"]:
        raise PolicyValidationError(result)
    return result


# --- diffs ------------------------------------------------------------------------------------

def diff_texts(
    previous: str,
    current: str,
    *,
    fromfile: str = "previous",
    tofile: str = "current",
) -> list[str]:
    return list(
        difflib.unified_diff(
            (previous or "").splitlines(),
            (current or "").splitlines(),
            fromfile=fromfile,
            tofile=tofile,
            lineterm="",
        )
    )


def changed_sections(previous: Mapping[str, Any] | None, current: Mapping[str, Any] | None) -> list[str]:
    previous = previous or {}
    current = current or {}
    return [section for section in REQUIRED_SECTIONS if previous.get(section) != current.get(section)]


def _effective_yaml(policy: Policy) -> tuple[str, str]:
    from apps.openshell.adapter import get_adapter

    try:
        return get_adapter().get_policy(policy.name) or "", ""
    except Exception as exc:  # noqa: BLE001 - the gateway being down is a normal state
        return "", f"{type(exc).__name__}: {exc}"


def diff_against_effective(policy: Policy) -> dict[str, Any]:
    effective, error = _effective_yaml(policy)
    previous = render.parse_document(effective)
    current = render.parse_document(policy.yaml_text)
    sections = changed_sections(previous, current)
    static = [section for section in sections if section in STATIC_SECTIONS]
    initial = not effective.strip()
    return {
        "policy": policy.name,
        "changed": effective.strip() != policy.yaml_text.strip(),
        "diff": diff_texts(effective, policy.yaml_text, fromfile="effective", tofile=f"desired (v{policy.version})"),
        "changed_sections": sections,
        "hot_reload": bool(sections) and not static and not initial,
        "restart_required": bool(static) and not initial,
        "initial_apply": initial,
        "gateway_error": error,
    }


# --- application ------------------------------------------------------------------------------

def apply_policy_to_gateway(policy: Policy, *, user: Model | None = None) -> dict[str, Any]:
    """Submit the stored document and report what happened.

    ``applied`` is the adapter's answer. The flags distinguish a network-only reload from a change
    to filesystem, Landlock or process settings, which OpenShell fixes when a nest is created and
    cannot hot-reload.
    """
    from apps.openshell.adapter import get_adapter

    validation = validate_policy_yaml(policy.yaml_text)
    if not validation["valid"]:
        return {
            "applied": False,
            "reason": "invalid_policy",
            "validation": validation,
            "hot_reload": False,
            "restart_required": False,
        }

    diff = diff_against_effective(policy)
    result = dict(get_adapter().set_policy(policy.name, policy.yaml_text))
    result["validation"] = validation
    result["hot_reload"] = diff["hot_reload"]
    result["restart_required"] = diff["restart_required"]
    result["initial_apply"] = diff["initial_apply"]
    result["changed_sections"] = diff["changed_sections"]
    if user is not None:
        audit(
            user,
            "policy.update",
            "policy",
            policy.pk,
            f"applied {policy.name} v{policy.version}",
            {"diff": diff["diff"], "adapter": result},
        )
    return result


# --- row synchronisation ----------------------------------------------------------------------

def sync_rows_from_document(policy: Policy, document: Mapping[str, Any]) -> None:
    """Make the display rows match a raw document. Unknown fields stay only in the YAML text."""
    rows = render.rows_from_document(document)
    policy.network_rules.all().delete()
    policy.filesystem_rules.all().delete()
    policy.process_rules.all().delete()
    NetworkRule.objects.bulk_create(
        [NetworkRule(policy=policy, **rule) for rule in rows["network_rules"]]
    )
    FilesystemRule.objects.bulk_create(
        [FilesystemRule(policy=policy, **rule) for rule in rows["filesystem_rules"]]
    )
    ProcessRule.objects.create(policy=policy, **rows["process"])
    LandlockRule.objects.update_or_create(policy=policy, defaults=rows["landlock"])


def apply_structured_payload(policy: Policy, payload: Mapping[str, Any]) -> None:
    """Replace the rule rows named in an API/admin payload. Other sections stay untouched."""
    if "network_rules" in payload:
        policy.network_rules.all().delete()
        for position, rule in enumerate(payload.get("network_rules") or []):
            data = dict(rule)
            data.pop("id", None)
            data.pop("policy", None)
            data.setdefault("position", position)
            NetworkRule.objects.create(policy=policy, **data)
    if "filesystem_rules" in payload:
        policy.filesystem_rules.all().delete()
        for rule in payload.get("filesystem_rules") or []:
            data = dict(rule)
            data.pop("id", None)
            data.pop("policy", None)
            FilesystemRule.objects.create(policy=policy, **data)
    if "process_rules" in payload:
        policy.process_rules.all().delete()
        for rule in payload.get("process_rules") or []:
            data = dict(rule)
            data.pop("id", None)
            data.pop("policy", None)
            ProcessRule.objects.create(policy=policy, **data)
    if "landlock" in payload:
        value = payload.get("landlock")
        if value is None:
            LandlockRule.objects.filter(policy=policy).delete()
        else:
            data = dict(value)
            data.pop("id", None)
            data.pop("policy", None)
            LandlockRule.objects.update_or_create(policy=policy, defaults=data)


# --- saving -----------------------------------------------------------------------------------

def _record_revision(policy: Policy, *, version: int, yaml_text: str, user: Model | None, note: str) -> None:
    if not yaml_text.strip():
        return
    PolicyRevision.objects.create(
        policy=policy,
        version=version,
        yaml_text=yaml_text,
        author=user if getattr(user, "pk", None) else None,
        note=note,
    )


@transaction.atomic
def save_policy(
    policy: Policy,
    *,
    user: Model | None = None,
    note: str = "",
    yaml_text: str | None = None,
    source: str | None = None,
    previous_yaml: str | None = None,
    previous_version: int | None = None,
    action: str = "policy.update",
    force_audit: bool = False,
) -> Policy:
    """Save a structured or raw policy and record the revision/audit atomically.

    ``yaml_text`` means the caller is editing the document directly. If no source is supplied that
    is a raw edit, which is also what an API client or admin does when it writes YAML. A structured
    save ignores ``yaml_text`` and renders from the rows instead.
    """
    locked = Policy.objects.select_for_update().get(pk=policy.pk)
    old_yaml = locked.yaml_text if previous_yaml is None else previous_yaml
    old_version = locked.version if previous_version is None else previous_version

    requested_source = source
    if yaml_text is not None and requested_source is None:
        requested_source = "raw"
    requested_source = requested_source or locked.yaml_source

    if requested_source not in {"structured", "raw"}:
        raise PolicyValidationError({"valid": False, "errors": ["Unknown YAML source."], "warnings": []})

    document: dict[str, Any]
    if requested_source == "raw":
        new_yaml = locked.yaml_text if yaml_text is None else yaml_text
        validation = require_valid_yaml(new_yaml)
        document = render.parse_document(new_yaml)
        new_source = "raw"
    else:
        if yaml_text is not None:
            raise PolicyValidationError(
                {
                    "valid": False,
                    "errors": [
                        "This policy is structured. Switch yaml_source to 'raw' before editing the document, "
                        "or send network_rules/filesystem_rules/process_rules/landlock instead."
                    ],
                    "warnings": [],
                }
            )
        document = render.build_document(locked)
        new_yaml = render.dump_document(document)
        validation = require_valid_document(document)
        new_source = "structured"

    changed = new_yaml != old_yaml or new_source != locked.yaml_source
    # A brand-new policy has version 1; rendering its first document is not a second edit. There is
    # no prior version to archive, so an initial save never creates a revision of itself.
    is_initial = action == "policy.create" and old_version <= 1
    if changed and is_initial:
        version = old_version
    elif changed:
        _record_revision(locked, version=old_version, yaml_text=old_yaml, user=user, note=note)
        version = old_version + 1
    else:
        version = old_version

    locked.yaml_text = new_yaml
    locked.yaml_source = new_source
    locked.structured = document
    locked.version = version
    locked.save(update_fields=["yaml_text", "yaml_source", "structured", "version", "content_hash", "updated_at"])

    if new_source == "raw":
        sync_rows_from_document(locked, document)

    if changed or force_audit:
        audit(
            user,
            action,
            "policy",
            locked.pk,
            f"{'saved' if changed else 'saved without changes'} {locked.name} v{locked.version}",
            {
                "source": new_source,
                "validation_warnings": validation["warnings"],
                "diff": diff_texts(old_yaml, new_yaml, fromfile=f"v{old_version}", tofile=f"v{version}"),
            },
        )
    return locked


# --- templates --------------------------------------------------------------------------------

def builtin_presets() -> list[dict[str, Any]]:
    """Return the shared built-in preset definitions (copies)."""
    return presets.presets()


def ensure_builtin_templates() -> int:
    """Create or refresh the built-in ``PolicyTemplate`` rows. Returns the number of presets."""
    for preset in presets.presets():
        PolicyTemplate.objects.update_or_create(
            name=preset["name"],
            defaults={
                "description": preset["description"],
                "document": preset["document"],
                "is_builtin": True,
            },
        )
    return len(presets.BUILTIN_PRESETS)


def template_document(template: Any) -> dict[str, Any]:
    if isinstance(template, PolicyTemplate):
        document = template.document
    elif isinstance(template, Mapping):
        document = template.get("document", template)
    elif isinstance(template, str):
        document = presets.preset_document(template)
        if document is None:
            saved = PolicyTemplate.objects.filter(name=template).first()
            document = saved.document if saved else None
    else:
        document = None
    if not isinstance(document, Mapping):
        raise PolicyValidationError({"valid": False, "errors": ["Unknown policy template."], "warnings": []})
    return dict(document)


@transaction.atomic
def create_policy_from_template(
    template: Any,
    *,
    name: str,
    scope: str = "sandbox",
    sandbox: Model | None = None,
    user: Model | None = None,
    note: str = "",
    source: str = "structured",
    is_active: bool = True,
) -> Policy:
    """Create a policy from a shared preset or saved template, as rows plus a generated document."""
    document = template_document(template)
    yaml_text = render.dump_document(document)
    policy = Policy.objects.create(
        name=name,
        scope=scope,
        sandbox=sandbox,
        yaml_source="structured" if source == "structured" else "raw",
        yaml_text=yaml_text if source == "raw" else "",
        is_active=is_active,
    )
    if source == "structured":
        sync_rows_from_document(policy, document)
        # Seed the base document so unknown template fields survive the first render, then render
        # the rows. The rendered document should equal the template when it only uses modelled keys.
        policy.yaml_text = yaml_text
        policy.save(update_fields=["yaml_text", "content_hash", "updated_at"])
    return save_policy(
        policy,
        user=user,
        note=note or f"created from {getattr(template, 'name', template)}",
        source=source,
        action="policy.create",
        force_audit=True,
    )


@transaction.atomic
def create_policy(
    *,
    name: str,
    scope: str = "sandbox",
    sandbox: Model | None = None,
    user: Model | None = None,
    note: str = "",
    yaml_text: str | None = None,
    source: str | None = None,
    payload: Mapping[str, Any] | None = None,
    is_active: bool = True,
) -> Policy:
    """Create a policy and route it through the same save path as an edit."""
    source = source or ("raw" if yaml_text is not None else "structured")
    policy = Policy.objects.create(
        name=name,
        scope=scope,
        sandbox=sandbox,
        yaml_source=source,
        yaml_text=yaml_text or "",
        is_active=is_active,
    )
    if payload:
        apply_structured_payload(policy, payload)
    return save_policy(
        policy,
        user=user,
        note=note,
        yaml_text=yaml_text if source == "raw" else None,
        source=source,
        action="policy.create",
        force_audit=True,
    )
