"""Small helpers shared by the admin modules."""
from __future__ import annotations

import logging

logger = logging.getLogger(__name__)


def audit_admin_action(request, action: str, target_type: str = "", target_id="", summary: str = "") -> None:  # noqa: ANN001
    """Record an operator action in the audit trail without breaking the UI.

    The audit trail is part of the identity module, which can be disabled in a stripped-down
    image. Admin actions must still work, so a missing or failing trail is logged instead of
    turning a successful runtime action into a server error.
    """
    try:
        from apps.identity.services import audit
    except Exception:  # noqa: BLE001 - identity is replaceable in minimal images
        logger.debug("Audit trail is not available", exc_info=True)
        return
    try:
        audit(
            getattr(request, "user", None),
            action,
            target_type,
            str(target_id),
            summary,
        )
    except Exception:  # noqa: BLE001 - never fail the action because its log failed
        logger.warning("Could not write audit event %s for %s", action, target_type, exc_info=True)
