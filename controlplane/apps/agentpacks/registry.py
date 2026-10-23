"""Load agent packs from ``packs/*.yaml``.

A pack is data, not code: adding an agent is a YAML file, no controller release required.
"""
from __future__ import annotations

import logging
from pathlib import Path

import yaml
from django.conf import settings

from apps.agentpacks.models import AgentPack

logger = logging.getLogger(__name__)

PACKS_DIR = Path(settings.REPO_DIR) / "packs"

REQUIRED_KEYS = {"key", "name", "install", "entrypoint"}


def load_packs(directory: Path | None = None, *, mark_builtin: bool = True) -> tuple[int, int, list[str]]:
    directory = directory or PACKS_DIR
    created = updated = 0
    problems: list[str] = []
    if not directory.exists():
        return 0, 0, [f"packs directory missing: {directory}"]

    for path in sorted(directory.glob("*.yaml")):
        try:
            data = yaml.safe_load(path.read_text(encoding="utf-8"))
        except yaml.YAMLError as exc:
            problems.append(f"{path.name}: YAML error: {exc}")
            continue
        if not isinstance(data, dict) or not REQUIRED_KEYS.issubset(data):
            missing = REQUIRED_KEYS - set(data or {})
            problems.append(f"{path.name}: missing keys {sorted(missing)}")
            continue

        defaults = {
            "name": data.get("name", data["key"]),
            "version": str(data.get("version", "")),
            "description": data.get("description", ""),
            "vendor": data.get("vendor", ""),
            "install": data.get("install", {}),
            "entrypoint": data.get("entrypoint", []),
            "ports": data.get("ports", []),
            "env_slots": data.get("env", []),
            "healthcheck": data.get("healthcheck", {}),
            "policy_profile": data.get("policy_profile", "coding-agent"),
            "requires_gpu": bool(data.get("requires_gpu", False)),
            "image_ref": data.get("image", ""),
            "docs_url": data.get("docs_url", ""),
            "is_builtin": mark_builtin,
        }
        _, was_created = AgentPack.objects.update_or_create(key=data["key"], defaults=defaults)
        created += int(was_created)
        updated += int(not was_created)

    return created, updated, problems
