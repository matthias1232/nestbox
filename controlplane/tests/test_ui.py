"""The appliance has one UI: the Unfold admin. These tests pin that down."""
from __future__ import annotations

import io
import zipfile

import pytest
from apps.builds.models import BuildRecipe
from apps.policies.models import Policy
from django.contrib.auth import get_user_model
from django.test import Client
from rest_framework.test import APIClient

pytestmark = pytest.mark.django_db


@pytest.fixture()
def admin_user():  # noqa: ANN201
    return get_user_model().objects.create_superuser(
        username="op", email="op@example.test", password="pw-for-test"  # noqa: S106
    )


@pytest.fixture()
def client(admin_user):  # noqa: ANN001, ANN201
    http = Client()
    http.force_login(admin_user)
    return http


def test_root_redirects_to_the_admin(client):  # noqa: ANN001
    response = client.get("/")
    assert response.status_code == 302
    assert response["Location"].startswith("/admin/")


def test_admin_requires_a_login():  # noqa: ANN001
    response = Client().get("/admin/")
    assert response.status_code == 302
    assert "/admin/login/" in response["Location"] or "/accounts/" in response["Location"]


@pytest.mark.parametrize(
    "path",
    ["/admin/", "/admin/core/", "/admin/policies/policy/", "/admin/sandboxes/sandbox/",
     "/admin/builds/buildrecipe/", "/admin/core/modulestate/"],
)
def test_admin_pages_render(client, path):  # noqa: ANN001
    response = client.get(path)
    assert response.status_code == 200, path
    assert b"<html" in response.content


def test_admin_lists_a_policy(client):  # noqa: ANN001
    Policy.objects.create(name="ui-policy", scope="sandbox", yaml_text="filesystem_policy: {}\n")
    response = client.get("/admin/policies/policy/")
    assert b"ui-policy" in response.content


def test_api_docs_render():  # noqa: ANN001
    http = Client()
    assert http.get("/api/docs/").status_code == 200
    assert http.get("/api/schema/").status_code == 200


def test_recipe_bundle_is_an_api_action(admin_user):  # noqa: ANN001
    recipe = BuildRecipe.objects.create(
        name="ui-recipe", hostname="nestbox-ui", admin_email="ops@example.test",
        ui_port=8443, tls_domain="nestbox-ui.local",
        preseed_nests=[{"name": "hermes-01", "pack": "hermes-agent", "port": 8080}],
    )
    api = APIClient()
    api.force_authenticate(admin_user)

    response = api.get(f"/api/v1/builds/recipes/{recipe.pk}/bundle/")

    assert response.status_code == 200
    assert response["Content-Type"] == "application/zip"
    names = zipfile.ZipFile(io.BytesIO(response.content)).namelist()
    assert {"seed/autoinstall.yaml", "seed/user-data", "seed/meta-data", "install.sh"} <= set(names)
