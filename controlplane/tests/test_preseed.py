"""Preseeded nests: the recipe's list becomes rows the dashboard shows on first sign-in."""
from __future__ import annotations

import json
from io import StringIO

import pytest
from apps.sandboxes.models import Sandbox
from django.core.management import call_command

pytestmark = pytest.mark.django_db


def _recipe(tmp_path, nests):
    path = tmp_path / "recipe.json"
    path.write_text(json.dumps({"version": 1, "preseed_nests": nests}), encoding="utf-8")
    return path


def test_seed_nests_creates_a_row_with_its_port(tmp_path):
    path = _recipe(tmp_path, [{"name": "Hermes 01", "pack": "hermes-agent", "port": 8080}])

    call_command("seed_nests", recipe=str(path), stdout=StringIO())

    sandbox = Sandbox.objects.get(name="hermes-01")
    assert sandbox.port == 8080
    assert sandbox.agent_pack_key == "hermes-agent"
    assert sandbox.state == "pending"


def test_seed_nests_is_idempotent_and_follows_a_changed_port(tmp_path):
    call_command(
        "seed_nests",
        recipe=str(_recipe(tmp_path, [{"name": "coder", "pack": "coding-agent", "port": 8080}])),
        stdout=StringIO(),
    )
    call_command(
        "seed_nests",
        recipe=str(_recipe(tmp_path, [{"name": "coder", "pack": "coding-agent", "port": 9090}])),
        stdout=StringIO(),
    )

    assert Sandbox.objects.filter(name="coder").count() == 1
    assert Sandbox.objects.get(name="coder").port == 9090


def test_seed_nests_skips_a_nameless_entry(tmp_path):
    path = _recipe(tmp_path, [{"name": "", "port": 8080}, {"name": "real", "port": 8081}])

    call_command("seed_nests", recipe=str(path), stdout=StringIO())

    assert list(Sandbox.objects.values_list("name", flat=True)) == ["real"]


def test_seed_nests_without_a_recipe_writes_nothing(tmp_path):
    call_command("seed_nests", recipe=str(tmp_path / "missing.json"), stdout=StringIO())

    assert not Sandbox.objects.exists()


def test_sandbox_url_carries_the_port():
    sandbox = Sandbox(name="coder", port=8080)

    assert sandbox.url_host == "coder.apps.localhost:8080"
    assert Sandbox(name="coder").url_host == "coder.apps.localhost"
