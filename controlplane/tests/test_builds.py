"""The recipe renderer is deterministic and is the contract the builder consumes."""
from __future__ import annotations

import pytest
from apps.builds.models import BuildRecipe
from apps.builds.services import render_autoinstall, render_seed

pytestmark = pytest.mark.django_db


def test_dhcp_recipe_renders_autoinstall():  # noqa: ANN201
    recipe = BuildRecipe.objects.create(name="slim-dhcp", network_mode="dhcp", hostname="nestbox")
    doc = render_autoinstall(recipe)
    assert doc["version"] == 1
    assert doc["identity"]["hostname"] == "nestbox"
    assert doc["network"]["ethernets"]["primary"]["dhcp4"] is True
    # Only packages present on the ISO: a failed mirror download must not abort the install.
    assert "openssh-server" in doc["packages"]
    assert not any("docker" in pkg for pkg in doc["packages"])
    assert doc["storage"]["layout"]["name"] == "direct"


def test_static_recipe_renders_netplan():  # noqa: ANN201
    recipe = BuildRecipe.objects.create(
        name="slim-static",
        network_mode="static",
        address="10.0.0.5/24",
        gateway="10.0.0.1",
        dns=["10.0.0.2", "1.1.1.1"],
    )
    doc = render_autoinstall(recipe)
    primary = doc["network"]["ethernets"]["primary"]
    assert primary["dhcp4"] is False
    assert primary["addresses"] == ["10.0.0.5/24"]
    assert primary["nameservers"]["addresses"] == ["10.0.0.2", "1.1.1.1"]


def test_seed_is_stable():  # noqa: ANN201
    recipe = BuildRecipe.objects.create(name="stable", network_mode="dhcp")
    first = render_seed(recipe)
    second = render_seed(recipe)
    assert first["sha256"] == second["sha256"]
    assert set(first) == {"autoinstall.yaml", "user-data", "meta-data", "sha256"}
    assert first["user-data"].startswith("#cloud-config")
    assert "instance-id" in first["meta-data"]


def test_seed_carries_autoinstall_inside_user_data():  # noqa: ANN201
    """subiquity only reads the autoinstall document from user-data, not from a sibling file."""
    recipe = BuildRecipe.objects.create(name="seed-shape", network_mode="dhcp")
    seed = render_seed(recipe)
    assert seed["user-data"].startswith("#cloud-config")
    assert "\nautoinstall:" in seed["user-data"]
    assert "user-data:" in seed["user-data"]  # the target's cloud-config travels nested
