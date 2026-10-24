"""The Image Creator: one recipe, an ISO seed and honest warnings."""
from __future__ import annotations

import pytest
from apps.builds.models import BuildRecipe
from apps.builds.services import recipe_warnings, render_autoinstall, render_seed
from apps.core.crypto import decrypt, encrypt
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
        tls_mode="letsencrypt",
        tls_domain="nestbox-lab.example.com",
        ui_port=443,
        preseed_nests=[{"name": "hermes-01", "pack": "hermes-agent", "port": 8080}],
    )


def test_password_is_encrypted_at_rest(recipe: BuildRecipe):  # noqa: ANN001
    recipe.admin_password_encrypted = encrypt("s3cret-value")
    recipe.save()
    assert recipe.admin_password_encrypted.startswith("enc:v1:")
    assert "s3cret-value" not in recipe.admin_password_encrypted
    assert decrypt(recipe.admin_password_encrypted) == "s3cret-value"


def test_autoinstall_never_contains_the_plaintext_password(recipe: BuildRecipe):  # noqa: ANN001
    recipe.admin_password_encrypted = encrypt("s3cret-value")
    recipe.save()
    seed = render_seed(recipe)
    assert "s3cret-value" not in seed["autoinstall.yaml"]
    assert "s3cret-value" not in seed["user-data"]


def test_autoinstall_reflects_network_and_ntp(recipe: BuildRecipe):  # noqa: ANN001
    recipe.network_mode = "static"
    recipe.address = "10.0.0.5/24"
    recipe.gateway = "10.0.0.1"
    recipe.dns = ["10.0.0.2"]
    recipe.ntp_enabled = False
    recipe.save()
    doc = render_autoinstall(recipe)
    ethernet = doc["network"]["ethernets"]["all-ethernet"]
    assert ethernet["dhcp4"] is False
    assert ethernet["match"] == {"name": "en*"}
    assert ethernet["addresses"] == ["10.0.0.5/24"]
    assert ethernet["nameservers"]["addresses"] == ["10.0.0.2"]


def test_port_conflict_is_reported(recipe: BuildRecipe):  # noqa: ANN001
    recipe.ui_port = 8080
    recipe.save()
    warnings = recipe_warnings(recipe)
    assert any("8080" in w for w in warnings)


def test_upload_mode_without_certificate_warns(recipe: BuildRecipe):  # noqa: ANN001
    recipe.tls_mode = "upload"
    recipe.save()
    assert any("certificate" in w.lower() for w in recipe_warnings(recipe))


def test_preview_endpoint_returns_the_iso_artefacts(recipe: BuildRecipe):  # noqa: ANN001
    user = get_user_model().objects.create_superuser("admin", "a@example.test", "x")
    client = APIClient()
    token, _ = Token.objects.get_or_create(user=user)
    client.credentials(HTTP_AUTHORIZATION=f"Token {token.key}")
    response = client.get(f"/api/v1/builds/recipes/{recipe.pk}/preview/")
    assert response.status_code == 200
    body = response.json()
    assert set(body) == {"seed", "manifest.json", "warnings"}
    assert body["seed"]["autoinstall.yaml"]
    # Deployment artefacts belong to the provisioning module, not to an ISO build.
    assert "install.sh" not in body
    assert "proxmox-plan.sh" not in body
