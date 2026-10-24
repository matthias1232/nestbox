"""Render an OpenShell policy document from the structured rule rows.

The YAML column is the contract the gateway receives. In ``structured`` mode this module builds
that document from the rows, in a stable order, and keeps fields the editor does not model so a
hand-written option is not silently dropped when a policy is saved again.

In ``raw`` mode nothing here rewrites the document. The parser at the bottom only derives rows so
the admin can show what the hand-written file contains.
"""
from __future__ import annotations

from collections.abc import Iterable, Mapping
from typing import Any

import yaml

from apps.policies.models import Policy

KNOWN_SECTIONS = ("filesystem_policy", "landlock", "process", "network_policies")

_ENDPOINT_KEYS = ("host", "port", "protocol", "enforcement", "access", "methods")
_BINARY_KEYS = ("path",)


def _mapping(value: Any) -> dict[str, Any]:
    return dict(value) if isinstance(value, Mapping) else {}


def _document_from_text(text: str) -> dict[str, Any]:
    try:
        value = yaml.safe_load(text) if text.strip() else {}
    except yaml.YAMLError:
        return {}
    return value if isinstance(value, dict) else {}


def _merge_known(existing: Any, generated: dict[str, Any]) -> dict[str, Any]:
    """Keep unknown keys from the previous document, with the modelled keys first."""
    previous = _mapping(existing)
    merged = dict(generated)
    for key, value in previous.items():
        if key not in merged:
            merged[key] = value
    return merged


def _endpoint_identity(endpoint: Mapping[str, Any]) -> tuple[Any, Any]:
    return (endpoint.get("host"), endpoint.get("port"))


def build_document(policy: Policy) -> dict[str, Any]:
    """Return the document described by ``policy`` and its current rule rows.

    Unknown top-level sections, unknown keys in known sections and unknown endpoint/binary options
    are carried over from ``policy.yaml_text``. They are not represented in the form, but they stay
    in the document until an operator removes them in raw mode.
    """
    previous = _document_from_text(policy.yaml_text) if policy else {}
    files = list(policy.filesystem_rules.all()) if policy and policy.pk else []
    landlock = getattr(policy, "landlock", None) if policy and policy.pk else None
    process = policy.process_rules.first() if policy and policy.pk else None

    filesystem = _merge_known(
        previous.get("filesystem_policy"),
        {
            "include_workdir": landlock.include_workdir if landlock else True,
            "read_only": [rule.path for rule in files if rule.mode == "read_only"],
            "read_write": [rule.path for rule in files if rule.mode == "read_write"],
        },
    )
    landlock_section = _merge_known(
        previous.get("landlock"),
        {"compatibility": landlock.compatibility if landlock else "best_effort"},
    )
    process_section = _merge_known(
        previous.get("process"),
        {
            "run_as_user": process.run_as_user if process and process.run_as_user else "sandbox",
            "run_as_group": process.run_as_group if process and process.run_as_group else "sandbox",
        },
    )

    previous_blocks = _mapping(previous.get("network_policies"))
    blocks: dict[str, dict[str, Any]] = {}
    for rule in policy.network_rules.all() if policy and policy.pk else []:
        name = rule.block or "default"
        previous_block = _mapping(previous_blocks.get(name))
        block = blocks.get(name)
        if block is None:
            block = {
                "endpoints": [],
                "binaries": [],
            }
            # A block can carry gateway options the editor does not model.
            for key, value in previous_block.items():
                if key not in block:
                    block[key] = value
            blocks[name] = block

        previous_endpoints = {
            _endpoint_identity(item): _mapping(item)
            for item in previous_block.get("endpoints", [])
            if isinstance(item, Mapping)
        }
        endpoint: dict[str, Any] = {
            "host": rule.endpoint_host,
            "port": rule.endpoint_port,
            "protocol": rule.protocol,
            "enforcement": rule.enforcement,
            "access": rule.access,
        }
        if rule.methods:
            endpoint["methods"] = list(rule.methods)
        previous_endpoint = previous_endpoints.get(_endpoint_identity(endpoint), {})
        for key, value in previous_endpoint.items():
            if key not in endpoint and key not in _ENDPOINT_KEYS:
                endpoint[key] = value
        block["endpoints"].append(endpoint)

        known_binaries = {item["path"] for item in block["binaries"]}
        previous_binaries = {
            item.get("path"): _mapping(item)
            for item in previous_block.get("binaries", [])
            if isinstance(item, Mapping)
        }
        for path in rule.binaries or []:
            if not isinstance(path, str) or path in known_binaries:
                continue
            binary: dict[str, Any] = {"path": path}
            for key, value in previous_binaries.get(path, {}).items():
                if key not in binary and key not in _BINARY_KEYS:
                    binary[key] = value
            block["binaries"].append(binary)
            known_binaries.add(path)

    document: dict[str, Any] = {
        "filesystem_policy": filesystem,
        "landlock": landlock_section,
        "process": process_section,
        "network_policies": blocks,
    }
    for key, value in previous.items():
        if key not in document:
            document[key] = value
    return document


def dump_document(document: Mapping[str, Any]) -> str:
    """Serialise a document in the stable form the gateway receives."""
    return yaml.safe_dump(dict(document), sort_keys=False, allow_unicode=True)


def render_yaml(policy: Policy) -> str:
    return dump_document(build_document(policy))


def parse_document(text: str) -> dict[str, Any]:
    """Parse a raw document without validating it. An invalid document yields ``{}``."""
    return _document_from_text(text)


def rows_from_document(document: Mapping[str, Any]) -> dict[str, Any]:
    """Flatten a document into rule-row dictionaries.

    Used when a raw document is saved so the read-only admin tables can show its contents, and by
    template seeding. It deliberately ignores anything the rows cannot represent; the YAML remains
    the source of truth.
    """
    filesystem = _mapping(document.get("filesystem_policy"))
    landlock = _mapping(document.get("landlock"))
    process = _mapping(document.get("process"))
    network = _mapping(document.get("network_policies"))

    filesystem_rules = []
    for mode in ("read_only", "read_write"):
        value = filesystem.get(mode)
        if isinstance(value, Iterable) and not isinstance(value, (str, bytes)):
            for path in value:
                if isinstance(path, str):
                    filesystem_rules.append({"path": path, "mode": mode})

    network_rules = []
    for block_name, block in network.items():
        body = _mapping(block)
        endpoints = body.get("endpoints") if isinstance(body.get("endpoints"), list) else []
        binaries = body.get("binaries") if isinstance(body.get("binaries"), list) else []
        paths = []
        for binary in binaries:
            if isinstance(binary, Mapping) and isinstance(binary.get("path"), str):
                paths.append(binary["path"])
            elif isinstance(binary, str):
                paths.append(binary)
        for position, endpoint in enumerate(endpoints):
            if not isinstance(endpoint, Mapping):
                continue
            network_rules.append(
                {
                    "block": block_name,
                    "endpoint_host": endpoint.get("host", ""),
                    "endpoint_port": endpoint.get("port", 443),
                    "protocol": endpoint.get("protocol", "rest"),
                    "access": endpoint.get("access", "allow"),
                    "enforcement": endpoint.get("enforcement", "enforced"),
                    "methods": list(endpoint.get("methods") or []),
                    "binaries": list(paths),
                    "position": position,
                }
            )

    return {
        "filesystem_rules": filesystem_rules,
        "landlock": {
            "compatibility": landlock.get("compatibility", "best_effort"),
            "include_workdir": filesystem.get("include_workdir", True),
        },
        "process": {
            "run_as_user": process.get("run_as_user", "sandbox"),
            "run_as_group": process.get("run_as_group", "sandbox"),
        },
        "network_rules": network_rules,
    }
