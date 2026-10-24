"""Gateway configuration and readiness helpers.

Readiness here is deliberately non-probing.  The portable controller must be
able to render its settings while a gateway is down or unreachable; a future
SDK adapter can add a bounded remote probe without changing this API contract.
"""
from __future__ import annotations

from typing import Any

from django.conf import settings

from apps.openshell.adapter import get_adapter
from apps.openshell.models import Gateway


def credential_state(gateway: Gateway) -> dict[str, bool]:
    """Return only presence indicators, never credential material."""
    return {
        "ca_configured": bool(gateway.ca_pem),
        "client_cert_configured": bool(gateway.client_cert_pem),
        "client_key_configured": bool(gateway.client_key_pem),
    }


def readiness(gateway: Gateway) -> dict[str, Any]:
    """Describe whether this registration can be used by this controller."""
    credentials = credential_state(gateway)
    configured = gateway.auth_mode != "mtls" or all(credentials.values())
    try:
        adapter = get_adapter()
        capabilities = adapter.capabilities
    except RuntimeError as exc:
        return {
            "status": "unavailable",
            "reason": "adapter_unavailable",
            "detail": str(exc),
            "endpoint": gateway.endpoint,
            "configured": configured,
            "credentials": credentials,
            "capabilities": {},
        }

    if not gateway.is_active:
        status = "disabled"
        reason = "gateway_inactive"
    elif not configured:
        status = "misconfigured"
        reason = "credentials_incomplete"
    elif capabilities.get("simulated"):
        status = "simulated"
        reason = "sdk_or_remote_probe_unavailable"
    else:
        status = "configured"
        reason = "remote_probe_not_implemented"

    return {
        "status": status,
        "reason": reason,
        "endpoint": gateway.endpoint,
        "configured": configured,
        "credentials": credentials,
        "capabilities": capabilities,
        "controller_endpoint": settings.NESTBOX_OPENSHELL_ENDPOINT,
    }
