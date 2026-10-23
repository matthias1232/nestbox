"""Policy validation, application and diffing.

The YAML document is authoritative. These helpers never rebuild a policy from structured rows;
they parse, validate and submit the document the operator actually edited.
"""
from __future__ import annotations

import difflib
from typing import Any

import yaml

from apps.policies.models import Policy

REQUIRED_SECTIONS = ("filesystem_policy", "landlock", "process", "network_policies")


def validate_policy_yaml(text: str) -> dict[str, Any]:
    errors: list[str] = []
    warnings: list[str] = []
    if not text.strip():
        return {"valid": False, "errors": ["Policy document is empty."], "warnings": []}

    try:
        document = yaml.safe_load(text)
    except yaml.YAMLError as exc:
        return {"valid": False, "errors": [f"YAML parse error: {exc}"], "warnings": []}

    if not isinstance(document, dict):
        return {"valid": False, "errors": ["Policy must be a mapping at the top level."], "warnings": []}

    for section in REQUIRED_SECTIONS:
        if section not in document:
            warnings.append(f"Missing section {section!r}; the gateway will apply its default.")

    filesystem = document.get("filesystem_policy")
    if filesystem is not None:
        if not isinstance(filesystem, dict):
            errors.append("filesystem_policy must be a mapping.")
        else:
            for key in ("read_only", "read_write"):
                value = filesystem.get(key)
                if value is not None and not isinstance(value, list):
                    errors.append(f"filesystem_policy.{key} must be a list of paths.")

    network = document.get("network_policies")
    if network is not None:
        if not isinstance(network, dict):
            errors.append("network_policies must be a mapping of block name to rules.")
        else:
            for block, rules in network.items():
                if not isinstance(rules, dict):
                    errors.append(f"network_policies.{block} must be a mapping.")
                    continue
                endpoints = rules.get("endpoints")
                if endpoints is not None and not isinstance(endpoints, list):
                    errors.append(f"network_policies.{block}.endpoints must be a list.")

    return {"valid": not errors, "errors": errors, "warnings": warnings}


def _effective_yaml(policy: Policy) -> str:
    from apps.openshell.adapter import get_adapter

    try:
        return get_adapter().get_policy(policy.name)
    except Exception:  # noqa: BLE001
        return ""


def diff_against_effective(policy: Policy) -> dict[str, Any]:
    effective = _effective_yaml(policy)
    diff = difflib.unified_diff(
        effective.splitlines(),
        policy.yaml_text.splitlines(),
        fromfile="effective",
        tofile=f"desired (v{policy.version})",
        lineterm="",
    )
    return {"policy": policy.name, "changed": effective.strip() != policy.yaml_text.strip(), "diff": list(diff)}


def apply_policy_to_gateway(policy: Policy) -> dict[str, Any]:
    from apps.openshell.adapter import get_adapter

    result = validate_policy_yaml(policy.yaml_text)
    if not result["valid"]:
        return {"applied": False, "reason": "invalid_policy", "validation": result}
    return get_adapter().set_policy(policy.name, policy.yaml_text)
