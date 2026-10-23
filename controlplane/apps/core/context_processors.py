"""Template context shared by every page."""
from __future__ import annotations

from django.conf import settings
from django.urls import reverse


def navigation(request):  # noqa: ANN001
    current = getattr(request, "resolver_match", None)
    url_name = current.url_name if current else ""
    items = [
        {"label": "Dashboard", "url": reverse("dashboard"), "name": "dashboard"},
        {"label": "Containers", "url": reverse("containers"), "name": "containers"},
        {"label": "Sandboxes", "url": reverse("sandboxes"), "name": "sandboxes"},
        {"label": "Policies", "url": reverse("policies"), "name": "policies"},
        {"label": "Builds", "url": reverse("builds"), "name": "builds"},
        {"label": "API", "url": reverse("swagger-ui"), "name": "swagger-ui"},
    ]
    return {
        "nav_items": items,
        "active_nav": url_name,
        "nestbox_version": settings.NESTBOX_VERSION,
    }
