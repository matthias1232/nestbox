"""Built-in policy presets.

Presets are ordinary OpenShell documents, not admin-specific shortcuts. The same definitions seed
``PolicyTemplate`` rows, fill the API's template endpoint and create policies in the admin. They are
kept in code so a fresh installation has useful starting points even before anyone edits one.
"""
from __future__ import annotations

from copy import deepcopy
from typing import Any

CODING_AGENT_DOCUMENT: dict[str, Any] = {
    "filesystem_policy": {
        "include_workdir": True,
        "read_only": ["/usr", "/lib", "/lib64", "/etc/ssl/certs"],
        "read_write": ["/sandbox", "/tmp"],
    },
    "landlock": {"compatibility": "best_effort"},
    "process": {"run_as_user": "sandbox", "run_as_group": "sandbox"},
    "network_policies": {
        "model-api": {
            "endpoints": [
                {
                    "host": "api.anthropic.com",
                    "port": 443,
                    "protocol": "rest",
                    "enforcement": "enforced",
                    "access": "allow",
                    "methods": ["GET", "POST"],
                }
            ],
            "binaries": [{"path": "/usr/local/bin/claude"}],
        }
    },
}

OFFLINE_DOCUMENT: dict[str, Any] = {
    "filesystem_policy": {
        "include_workdir": True,
        "read_only": ["/usr", "/lib", "/lib64", "/etc/ssl/certs"],
        "read_write": ["/sandbox", "/tmp"],
    },
    "landlock": {"compatibility": "strict"},
    "process": {"run_as_user": "sandbox", "run_as_group": "sandbox"},
    "network_policies": {},
}

AUDIT_DOCUMENT: dict[str, Any] = {
    "filesystem_policy": {
        "include_workdir": True,
        "read_only": ["/usr", "/lib", "/lib64", "/etc/ssl/certs"],
        "read_write": ["/sandbox", "/tmp"],
    },
    "landlock": {"compatibility": "best_effort"},
    "process": {"run_as_user": "sandbox", "run_as_group": "sandbox"},
    "network_policies": {
        "audit-all-web": {
            "endpoints": [
                {
                    "host": "api.openai.com",
                    "port": 443,
                    "protocol": "rest",
                    "enforcement": "audit",
                    "access": "allow",
                    "methods": ["GET", "POST"],
                }
            ],
            "binaries": [{"path": "/usr/local/bin/agent"}],
        }
    },
}

#: Keys are stable identifiers; names are what the operator sees.
BUILTIN_PRESETS: tuple[dict[str, Any], ...] = (
    {
        "key": "coding-agent",
        "name": "Coding agent (Anthropic API)",
        "description": (
            "A working starting point: read-only system paths, a writable sandbox, and HTTPS "
            "access to api.anthropic.com for the Claude binary."
        ),
        "document": CODING_AGENT_DOCUMENT,
    },
    {
        "key": "offline",
        "name": "Offline nest",
        "description": "Strict Landlock fencing and no outbound network blocks. Useful for local models.",
        "document": OFFLINE_DOCUMENT,
    },
    {
        "key": "audit-web",
        "name": "Audit API traffic",
        "description": (
            "Record requests to api.openai.com without blocking them. Switch enforcement to "
            "enforced once the rule list is complete."
        ),
        "document": AUDIT_DOCUMENT,
    },
)


def preset_document(key: str) -> dict[str, Any] | None:
    """Return a copy of a built-in document, or ``None`` for an unknown key."""
    for preset in BUILTIN_PRESETS:
        if preset["key"] == key or preset["name"] == key:
            return deepcopy(preset["document"])
    return None


def presets() -> list[dict[str, Any]]:
    """Return copies so callers cannot mutate the module-level definitions."""
    return [
        {
            "key": preset["key"],
            "name": preset["name"],
            "description": preset["description"],
            "document": deepcopy(preset["document"]),
        }
        for preset in BUILTIN_PRESETS
    ]
