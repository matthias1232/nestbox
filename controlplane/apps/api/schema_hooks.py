"""drf-spectacular hooks: stable operation ids plus authorization/effect metadata."""
from __future__ import annotations

import re

_READ = {"get", "head", "options"}
_DESTRUCTIVE = {"delete"}
_ADMIN_ONLY_TAGS = {"builds", "backups", "audit"}


def _camel(stem: str) -> str:
    parts = re.split(r"[^a-zA-Z0-9]+", stem)
    head, *rest = [p for p in parts if p]
    return head + "".join(p.capitalize() for p in rest)


def fix_operation_ids(result, *args, **kwargs):  # noqa: ANN001, ARG001
    """Deterministic, unique, human-readable operation ids."""
    seen: dict[str, int] = {}
    for path, methods in result.get("paths", {}).items():
        for method, operation in methods.items():
            if method not in {"get", "post", "put", "patch", "delete", "head", "options"}:
                continue
            parts = [p for p in path.strip("/").split("/") if p and not p.startswith("{")]
            stem = "_".join(parts[-2:]) if len(parts) >= 2 else "_".join(parts)
            stem = stem.replace("{", "").replace("}", "")
            op_id = _camel(stem or "root") + method.capitalize()
            if op_id in seen:
                seen[op_id] += 1
                op_id = f"{op_id}{seen[op_id]}"
            else:
                seen[op_id] = 1
            operation["operationId"] = op_id
    return result


def annotate_operations(result, *args, **kwargs):  # noqa: ANN001, ARG001
    """Attach stable operation ids, the role matrix and the blast radius to every operation."""
    result = fix_operation_ids(result)
    for path, methods in result.get("paths", {}).items():
        for method, operation in methods.items():
            if method not in {"get", "post", "put", "patch", "delete"}:
                continue
            tags = operation.get("tags", [])
            if method in _READ:
                roles = ["admin", "operator", "viewer"]
                effect = "read"
            elif method in _DESTRUCTIVE:
                roles = ["admin"]
                effect = "destructive"
            else:
                roles = ["admin", "operator"]
                effect = "write"
            if tags and tags[0] in _ADMIN_ONLY_TAGS:
                roles = ["admin"]
            operation["x-roles"] = roles
            operation["x-effect"] = effect
            if "stream" in path:
                operation["x-stream"] = "sse"
    return result
