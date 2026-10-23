"""The UI must render for an authenticated operator without a container runtime present."""
from __future__ import annotations

import pytest
from apps.policies.models import Policy
from apps.sandboxes.models import Sandbox
from django.contrib.auth import get_user_model
from django.test import Client

pytestmark = pytest.mark.django_db


@pytest.fixture()
def logged_in():  # noqa: ANN201
    user = get_user_model().objects.create_user(
        username="op", email="op@example.test", password="pw-for-test"  # noqa: S106
    )
    client = Client()
    client.force_login(user)
    return client


@pytest.mark.parametrize("path", ["/", "/containers/", "/sandboxes/", "/sandboxes/new/", "/policies/", "/builds/"])
def test_pages_render(logged_in, path):  # noqa: ANN001
    response = logged_in.get(path)
    assert response.status_code == 200
    assert b"<html" in response.content


def test_dashboard_survives_a_missing_runtime(logged_in):  # noqa: ANN001
    response = logged_in.get("/")
    assert response.status_code == 200
    # The banner is expected when no container socket is reachable.
    assert b"container runtime" in response.content.lower() or b"runtime" in response.content.lower()


def test_policy_editor_roundtrip(logged_in):  # noqa: ANN001
    policy = Policy.objects.create(name="p-ui", scope="sandbox", yaml_text="filesystem_policy: {}\n")
    page = logged_in.get(f"/policies/{policy.pk}/")
    assert page.status_code == 200
    assert b"Policy document" in page.content

    response = logged_in.post(
        f"/policies/{policy.pk}/",
        {"action": "validate", "yaml_text": "filesystem_policy: {}\nnetwork_policies: {}\n"},
    )
    assert response.status_code == 200


def test_sandbox_detail_lists_pack(logged_in):  # noqa: ANN001
    Sandbox.objects.create(name="hermes-01", agent_pack_key="hermes-agent", state="running")
    response = logged_in.get("/sandboxes/hermes-01/")
    assert response.status_code == 200
    assert b"hermes-01" in response.content
