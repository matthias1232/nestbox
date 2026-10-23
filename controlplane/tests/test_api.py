"""Contract-level tests for the API surface."""
from __future__ import annotations

import pytest
from apps.core.models import Container, Node
from apps.policies.models import Policy
from apps.sandboxes.models import Sandbox
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from rest_framework.authtoken.models import Token
from rest_framework.test import APIClient

pytestmark = pytest.mark.django_db


@pytest.fixture()
def admin(db):  # noqa: ANN001, ARG001
    user = get_user_model().objects.create_user(
        username="admin", email="admin@example.test", password="x"  # noqa: S106
    )
    user.is_superuser = True
    user.save()
    Group.objects.get_or_create(name="admin")
    user.groups.add(Group.objects.get(name="admin"))
    return user


@pytest.fixture()
def viewer(db):  # noqa: ANN001, ARG001
    user = get_user_model().objects.create_user(
        username="viewer", email="viewer@example.test", password="x"  # noqa: S106
    )
    Group.objects.get_or_create(name="viewer")
    user.groups.add(Group.objects.get(name="viewer"))
    return user


@pytest.fixture()
def client_for():  # noqa: ANN201
    def _make(user):  # noqa: ANN001
        client = APIClient()
        token, _ = Token.objects.get_or_create(user=user)
        client.credentials(HTTP_AUTHORIZATION=f"Token {token.key}")
        return client

    return _make


def test_system_endpoint_is_public():  # noqa: ANN201
    response = APIClient().get("/api/v1/system/")
    assert response.status_code == 200
    assert response.json()["name"] == "nestbox"


def test_unauthenticated_access_is_denied():  # noqa: ANN201
    response = APIClient().get("/api/v1/containers/")
    assert response.status_code in {401, 403}
    assert "error" in response.json() or response.status_code == 403


def test_container_inventory_roundtrip(admin, client_for):  # noqa: ANN001
    node = Node.objects.create(name="local")
    Container.objects.create(node=node, name="nestbox-controller", role="controller", state="running")
    response = client_for(admin).get("/api/v1/containers/")
    assert response.status_code == 200
    names = [row["name"] for row in response.json()["results"]]
    assert "nestbox-controller" in names


def test_viewer_cannot_create_sandbox(viewer, client_for):  # noqa: ANN001
    response = client_for(viewer).post("/api/v1/sandboxes/", {"name": "hermes-01"}, format="json")
    assert response.status_code == 403


def test_sandbox_create_and_conflict(admin, client_for):  # noqa: ANN001
    client = client_for(admin)
    first = client.post("/api/v1/sandboxes/", {"name": "hermes-01", "agent_pack_key": "hermes-agent"}, format="json")
    assert first.status_code == 201
    assert first.json()["name"] == "hermes-01"
    assert Sandbox.objects.filter(name="hermes-01").exists()

    second = client.post("/api/v1/sandboxes/", {"name": "hermes-01"}, format="json")
    assert second.status_code == 409
    assert second.json()["error"]["code"] == "sandbox.name_taken"


def test_policy_validation(admin, client_for):  # noqa: ANN001
    policy = Policy.objects.create(
        name="coding-agent",
        scope="sandbox",
        yaml_text="filesystem_policy:\n  read_only: ['/usr']\nnetwork_policies:\n  api:\n    endpoints: []\n",
    )
    response = client_for(admin).post(f"/api/v1/policies/{policy.pk}/validate/", {}, format="json")
    assert response.status_code == 200
    assert response.json()["valid"] is True

    policy.yaml_text = "network_policies: [this is not a mapping]\n"
    policy.save()
    response = client_for(admin).post(f"/api/v1/policies/{policy.pk}/validate/", {}, format="json")
    assert response.json()["valid"] is False


def test_policy_update_creates_revision(admin, client_for):  # noqa: ANN001
    policy = Policy.objects.create(name="p1", scope="sandbox", yaml_text="a: 1\n")
    response = client_for(admin).patch(
        f"/api/v1/policies/{policy.pk}/", {"yaml_text": "a: 2\n"}, format="json"
    )
    assert response.status_code == 200
    policy.refresh_from_db()
    assert policy.version == 2
    assert policy.revisions.count() == 1


def test_schema_and_docs_are_served():  # noqa: ANN201
    client = APIClient()
    assert client.get("/api/schema/").status_code == 200
    assert client.get("/api/docs/").status_code == 200
    assert client.get("/api/redoc/").status_code == 200


def test_language_catalog_starts_with_english():  # noqa: ANN201
    response = APIClient().get("/api/v1/identity/languages/")
    assert response.status_code == 200
    codes = [row["code"] for row in response.json()]
    assert codes[0] == "en"
    assert response.json()[0]["completion"] == 100
