"""The module catalog: what is available, what runs, and what the operator asked for."""
from __future__ import annotations

import json

import pytest
from apps.core import modules as runtime
from apps.core.models import ModuleState
from config import modules as catalog

pytestmark = pytest.mark.django_db


@pytest.fixture()
def state_dir(tmp_path, monkeypatch):
    monkeypatch.setenv("NESTBOX_STATE_DIR", str(tmp_path))
    monkeypatch.delenv("NESTBOX_MODULES", raising=False)
    return tmp_path


def test_resolve_closes_the_dependency_graph():
    assert catalog.resolve(["policies"]) == (
        "core", "api", "containers", "sandboxes", "policies",
    )
    assert catalog.resolve(["core"]) == ("core", "api")


def test_unknown_keys_are_ignored_and_always_on_stays():
    assert catalog.resolve(["does-not-exist"]) == ("core", "api")


def test_state_file_wins_over_the_environment(state_dir, monkeypatch):
    monkeypatch.setenv("NESTBOX_MODULES", "core,backups")
    catalog.write_state(["core", "identity"])
    assert "identity" in catalog.active_keys()
    assert "backups" not in catalog.active_keys()


def test_environment_is_the_seed_when_no_file_exists(state_dir, monkeypatch):
    monkeypatch.setenv("NESTBOX_MODULES", "core,backups")
    assert "backups" in catalog.active_keys()


def test_default_is_everything(state_dir):
    assert set(catalog.active_keys()) == set(catalog.CATALOG)


def test_state_file_is_json_and_atomic(state_dir):
    effective = catalog.write_state(["core", "sandboxes"])
    payload = json.loads((state_dir / "active-modules.json").read_text())
    assert payload["modules"] == list(effective)
    assert not list(state_dir.glob("*.tmp"))


def test_set_enabled_writes_the_file_and_flags_a_restart(state_dir, monkeypatch):
    monkeypatch.setattr("django.conf.settings.NESTBOX_ACTIVE_MODULES", catalog.active_keys())
    effective = runtime.set_enabled("backups", False)

    assert "backups" not in effective
    assert "backups" not in json.loads((state_dir / "active-modules.json").read_text())["modules"]
    assert ModuleState.objects.get(key="backups").enabled is False
    assert runtime.pending_restart() is True


def test_a_non_toggleable_module_cannot_be_switched(state_dir):
    with pytest.raises(ValueError):
        runtime.set_enabled("core", False)


def test_init_is_idempotent(state_dir, monkeypatch):
    monkeypatch.setenv("NESTBOX_MODULES", "core,backups")
    first = runtime.init_state_file()
    second = runtime.init_state_file()
    assert first == second
    assert "backups" in first
