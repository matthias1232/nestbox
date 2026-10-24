"""Provisioning: deployment artefacts are rendered apart from the ISO build."""
from __future__ import annotations

import io
import zipfile

import pytest
from apps.builds.models import BuildJob, BuildRecipe
from apps.core.crypto import encrypt
from apps.provisioning.services import (
    render_brev_setup,
    render_install_script,
    render_provisioning,
    render_proxmox_plan,
)
from django.contrib.auth import get_user_model
from rest_framework.authtoken.models import Token
from rest_framework.test import APIClient

pytestmark = pytest.mark.django_db


@pytest.fixture()
def recipe() -> BuildRecipe:
    return BuildRecipe.objects.create(
        name="hermes-lab",
        ubuntu_series="24.04",
        hostname="nestbox-lab",
        admin_email="ops@example.com",
        admin_password_encrypted=encrypt("s3cret-value"),
        tls_mode="letsencrypt",
        tls_domain="nestbox-lab.example.com",
        ui_port=443,
        preseed_nests=[{"name": "hermes-01", "pack": "hermes-agent", "port": 8080}],
    )


@pytest.fixture()
def admin():  # noqa: ANN001, ANN201
    return get_user_model().objects.create_superuser("admin", "admin@example.test", "x")


def _client_for(user) -> APIClient:  # noqa: ANN001
    client = APIClient()
    token, _ = Token.objects.get_or_create(user=user)
    client.credentials(HTTP_AUTHORIZATION=f"Token {token.key}")
    return client


def test_brev_script_is_user_facing_and_uses_sudo(recipe: BuildRecipe):  # noqa: ANN001
    script = render_brev_setup(recipe)
    assert script.startswith("#!/usr/bin/env bash")
    assert "sudo apt-get" in script
    assert "hermes-01" in script
    assert "8080" in script
    assert "docker compose" in script


def test_install_script_embeds_netplan(recipe: BuildRecipe):  # noqa: ANN001
    recipe.network_mode = "dhcp"
    recipe.save()
    script = render_install_script(recipe)
    assert "/etc/netplan/10-nestbox.yaml" in script
    assert "dhcp4: true" in script


def test_proxmox_plan_creates_a_vm_with_the_iso(recipe: BuildRecipe):  # noqa: ANN001
    plan = render_proxmox_plan(recipe, vmid=201, node="pve02")
    assert "qm create" in plan
    assert "--memory 4096" in plan
    assert "nestbox-24.04-slim.iso" in plan
    assert "pve02" in plan


def test_render_combines_the_seed_with_every_deployment_artefact(recipe: BuildRecipe):  # noqa: ANN001
    artefacts = render_provisioning(recipe)
    assert set(artefacts) == {
        "seed",
        "install.sh",
        "brev-setup.sh",
        "brev-command.txt",
        "proxmox-plan.sh",
        "manifest.json",
        "warnings",
    }
    assert artefacts["seed"]["autoinstall.yaml"]
    assert "hermes-lab" in artefacts["install.sh"]
    assert "brev start" in artefacts["brev-command.txt"]


def test_provisioning_is_a_baked_default_module():  # noqa: ANN201
    from config.modules import CATALOG

    spec = CATALOG["provisioning"]
    assert spec.app == "apps.provisioning"
    assert spec.api_prefix == "provisioning"
    assert spec.requires == ("builds",)


def test_render_endpoint_returns_every_deployment_artefact(recipe: BuildRecipe, admin):  # noqa: ANN001
    response = _client_for(admin).get(f"/api/v1/provisioning/recipes/{recipe.pk}/render/")

    assert response.status_code == 200
    body = response.json()
    assert set(body) == {
        "seed",
        "install.sh",
        "brev-setup.sh",
        "brev-command.txt",
        "proxmox-plan.sh",
        "manifest.json",
        "warnings",
    }
    assert body["seed"]["user-data"].startswith("#cloud-config")


def test_render_endpoint_is_admin_only(recipe: BuildRecipe):  # noqa: ANN001
    anonymous = APIClient().get(f"/api/v1/provisioning/recipes/{recipe.pk}/render/")
    assert anonymous.status_code in {401, 403}

    viewer = get_user_model().objects.create_user("viewer", "viewer@example.test", "x")
    denied = _client_for(viewer).get(f"/api/v1/provisioning/recipes/{recipe.pk}/render/")
    assert denied.status_code == 403


def test_download_endpoint_returns_a_zip_with_the_deployment_plan(recipe: BuildRecipe, admin):  # noqa: ANN001
    response = _client_for(admin).get(f"/api/v1/provisioning/recipes/{recipe.pk}/download/")

    assert response.status_code == 200
    assert response["Content-Type"] == "application/zip"
    names = set(zipfile.ZipFile(io.BytesIO(response.content)).namelist())
    assert {
        "seed/autoinstall.yaml",
        "seed/user-data",
        "seed/meta-data",
        "install.sh",
        "brev-setup.sh",
        "brev-command.txt",
        "proxmox-plan.sh",
        "manifest.json",
        "warnings.txt",
    } <= names


def test_rendering_never_queues_a_build(recipe: BuildRecipe, admin):  # noqa: ANN001
    client = _client_for(admin)
    assert BuildJob.objects.count() == 0

    assert client.get(f"/api/v1/provisioning/recipes/{recipe.pk}/render/").status_code == 200
    assert client.get(f"/api/v1/provisioning/recipes/{recipe.pk}/download/").status_code == 200

    assert BuildJob.objects.count() == 0


def test_recipe_list_is_available_to_admins(recipe: BuildRecipe, admin):  # noqa: ANN001
    response = _client_for(admin).get("/api/v1/provisioning/recipes/")

    assert response.status_code == 200
    assert [item["name"] for item in response.json()["results"]] == [recipe.name]
