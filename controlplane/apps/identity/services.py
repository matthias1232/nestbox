"""Audit trail and language discovery."""
from __future__ import annotations

from apps.identity.models import AuditEvent


def audit(user, action: str, target_type: str = "", target_id: str | int = "", summary: str = "", diff=None):  # noqa: ANN001
    return AuditEvent.objects.create(
        actor=user if getattr(user, "is_authenticated", False) else None,
        action=action,
        target_type=target_type,
        target_id=str(target_id),
        summary=summary[:300],
        diff=diff or {},
    )

from apps.core.language import language_catalog  # noqa: E402,F401
