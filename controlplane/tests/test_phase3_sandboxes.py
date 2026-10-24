"""Phase 3 contracts: pack metadata, persisted configuration and policy-first starts."""
from __future__ import annotations

import pytest
from apps.agentpacks.models import AgentPack
from apps.policies.models import Policy
from apps.sandboxes.models import Sandbox
from apps.sandboxes.services import create_sandbox, pack_configuration, start_sandbox
from django.contrib.auth import get_user_model
from rest_framework.test import APIClient

pytestmark = pytest.mark.django_db


def policy(name="nest-policy"):
    return Policy.objects.create(name=name, yaml_text="network_policies: {}\n")


def test_pack_configuration_merges_defaults_and_rejects_mounts():
    pack = AgentPack.objects.create(key="browser-pack", name="Browser pack", defaults={"theme": "dark"})
    assert pack_configuration(pack, {"theme": "light", "port": 3000}) == {"theme": "light", "port": 3000}
    with pytest.raises(Exception) as excinfo:
        pack_configuration(pack, {"mounts": ["/host"]})
    assert excinfo.value.code == "sandbox.mounts_unsupported"


def test_create_persists_assigned_policy(monkeypatch):
    user = get_user_model().objects.create_user(username="operator")
    adapter_calls = []

    class Adapter:
        def create_sandbox(self, **kwargs):
            adapter_calls.append(kwargs)
            return {"ok": True, "id": "nest-1", "state": "running"}

        def set_policy(self, *args):
            return {"ok": True}

        def stop_sandbox(self, *args):
            return {"ok": True}

    monkeypatch.setattr("apps.sandboxes.services.get_adapter", lambda: Adapter())
    sandbox = create_sandbox(user=user, name="with-policy", policy=policy())
    assert sandbox.state == "running"
    assert sandbox.policy_id is not None
    assert adapter_calls


def test_start_requires_and_applies_policy(monkeypatch):
    sandbox = Sandbox.objects.create(name="stopped", state="stopped")
    with pytest.raises(Exception) as excinfo:
        start_sandbox(sandbox.name)
    assert excinfo.value.code == "sandbox.policy_required"

    applied = []

    class Adapter:
        def set_policy(self, name, text):
            applied.append((name, text))
            return {"ok": True}

    sandbox.policy = policy("start-policy")
    sandbox.save()
    monkeypatch.setattr("apps.sandboxes.services.get_adapter", lambda: Adapter())
    started = start_sandbox(sandbox.name)
    assert started.state == "running"
    assert applied == [("stopped", "network_policies: {}\n")]


def test_guided_endpoint_exposes_browser_metadata():
    AgentPack.objects.create(
        key="web-agent",
        name="Web agent",
        browser={"available": True, "port": 3000},
        defaults={"workspace": "default"},
    )
    client = APIClient()
    user = get_user_model().objects.create_user(username="guided-operator", is_superuser=True)
    client.force_authenticate(user)
    response = client.get("/api/v1/sandboxes/guided/")
    assert response.status_code == 200
    assert response.data["configuration"] == {"mounts_supported": False}
    assert response.data["packs"][0]["browser"] == {"available": True, "port": 3000}
