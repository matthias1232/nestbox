"""The admin navigation presents workflows rather than implementation tables."""
from __future__ import annotations

from config.admin_navigation import navigation, quickstart


def _titles(items):
    return [item["title"] for group in items for item in group["items"]]


def test_navigation_only_includes_active_operational_modules():
    items = navigation(["core", "identity", "containers", "sandboxes"])
    titles = _titles(items)

    assert "Containers" in titles
    assert "Nests" in titles
    assert "OpenShell gateway" not in titles
    assert "API explorer" in titles
    assert "/admin/containers/container/" in [item["link"] for group in items for item in group["items"]]


def test_quickstart_is_usable_on_a_minimal_image():
    steps = quickstart(["core", "identity"])

    assert [step["title"] for step in steps] == ["Connect automation"]
