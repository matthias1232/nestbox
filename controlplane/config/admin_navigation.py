"""Curated operator navigation for the Unfold admin.

This deliberately names workflows rather than Django models.  A module can be
left out of an appliance image, so entries are assembled from the active
module keys instead of exposing an ever-growing list of internal tables.
"""
from __future__ import annotations

from collections.abc import Iterable

_MODULE_ITEMS = {
    "containers": ("Operate", "Containers", "deployed_code", "/admin/containers/container/"),
    "sandboxes": ("Operate", "Nests", "inventory_2", "/admin/sandboxes/sandbox/"),
    "policies": ("Govern", "Policies", "policy", "/admin/policies/policy/"),
    "network": ("Govern", "Network & certificates", "lan", "/admin/network/certificate/"),
    "builds": ("Build", "Build jobs", "pending_actions", "/admin/builds/buildjob/"),
    "agentpacks": ("Build", "Agent packs", "extension", "/admin/agentpacks/agentpack/"),
    "openshell": ("Platform", "OpenShell & gateway", "hub", "/admin/openshell/gateway/"),
    "backups": ("Platform", "Backups", "backup", "/admin/backups/backuptarget/"),
}


def navigation(active_modules: Iterable[str]) -> list[dict]:
    """Return the small, stable navigation surface for an appliance image.

    New module settings models intentionally do not need an entry here. Their
    integration can add an operational destination once its workflow is known,
    without turning configuration tables into top-level navigation by default.
    """
    groups = {
        "Operate": [{"title": "Dashboard", "icon": "dashboard", "link": "/admin/"}],
        "Govern": [],
        "Build": [],
        "Platform": [
            {"title": "Users & access", "icon": "group", "link": "/admin/auth/user/"},
            {"title": "Audit log", "icon": "history", "link": "/admin/identity/auditevent/"},
        ],
        "Developer": [
            {"title": "API explorer", "icon": "api", "link": "/api/docs/"},
            {"title": "API schema", "icon": "code", "link": "/api/schema/"},
            {"title": "API root", "icon": "account_tree", "link": "/api/v1/"},
        ],
    }
    for key in active_modules:
        item = _MODULE_ITEMS.get(key)
        if item:
            group, title, icon, link = item
            groups[group].append({"title": title, "icon": icon, "link": link})

    return [
        {"title": title, "separator": True, "items": items}
        for title, items in groups.items()
        if items
    ]


def quickstart(active_modules: Iterable[str]) -> list[dict]:
    """Return first-use actions for the dashboard without assuming all modules."""
    active = set(active_modules)
    steps = []
    if "containers" in active:
        steps.append({"title": "Review containers", "description": "Check runtime health and services.", "url": "/admin/containers/container/", "action": "Open containers"})
    if "sandboxes" in active:
        steps.append({"title": "Create a nest", "description": "Choose a template and define its runtime configuration.", "url": "/admin/sandboxes/sandbox/add/", "action": "Create nest"})
    if "policies" in active:
        steps.append({"title": "Set policy", "description": "Define the controls applied to nests.", "url": "/admin/policies/policy/", "action": "Manage policies"})
    steps.append({"title": "Connect automation", "description": "Explore the versioned Developer API and its schema.", "url": "/api/docs/", "action": "Open API explorer"})
    return steps
