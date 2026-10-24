"""Golden tests for the structured policy editor.

The editor has one authority per mode: structured rows or a raw document. These tests pin the exact
document produced from rows, the revision/audit trail, what survives a round-trip, and the API/admin
surface that operators actually touch.
"""
from __future__ import annotations

import copy
import importlib
import textwrap

import pytest
import yaml
from apps.core.nest_sections import validate_spec
from apps.identity.models import AuditEvent
from apps.policies import presets, services
from apps.policies.models import (
    FilesystemRule,
    LandlockRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    PolicyTemplate,
    ProcessRule,
)
from django import forms
from django.apps import apps as django_apps
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from django.core.exceptions import ValidationError
from django.test import Client
from rest_framework.authtoken.models import Token
from rest_framework.test import APIClient
from unfold.admin import TabularInline

pytestmark = pytest.mark.django_db


@pytest.fixture()
def admin_user():  # noqa: ANN201
    user = get_user_model().objects.create_user(
        username="policy-admin", email="policy@example.test", password="x"  # noqa: S106
    )
    user.is_superuser = True
    user.is_staff = True
    user.save()
    group, _ = Group.objects.get_or_create(name="admin")
    user.groups.add(group)
    return user


@pytest.fixture()
def api_client(admin_user):  # noqa: ANN001
    client = APIClient()
    token, _ = Token.objects.get_or_create(user=admin_user)
    client.credentials(HTTP_AUTHORIZATION=f"Token {token.key}")
    return client


@pytest.fixture()
def http_client(admin_user):  # noqa: ANN001
    client = Client()
    client.force_login(admin_user)
    return client


def _rule_rows(policy: Policy, *, path: str = "/usr", host: str = "api.anthropic.com") -> None:
    FilesystemRule.objects.create(policy=policy, path="/usr", mode="read_only")
    FilesystemRule.objects.create(policy=policy, path="/sandbox", mode="read_write")
    LandlockRule.objects.create(policy=policy, compatibility="best_effort", include_workdir=True)
    ProcessRule.objects.create(policy=policy, run_as_user="sandbox", run_as_group="sandbox")
    NetworkRule.objects.create(
        policy=policy,
        block="model-api",
        endpoint_host=host,
        endpoint_port=443,
        protocol="rest",
        enforcement="enforced",
        access="allow",
        methods=["GET"],
        binaries=["/usr/local/bin/claude"],
        position=0,
    )


def make_structured_policy(name: str = "coding-agent", **kwargs) -> Policy:  # noqa: ANN003
    policy = Policy.objects.create(name=name, scope="sandbox", yaml_source="structured")
    _rule_rows(policy, **kwargs)
    return services.save_policy(
        policy, source="structured", action="policy.create", force_audit=True
    )


# --- golden document --------------------------------------------------------------------------

def test_structured_rows_render_a_valid_document_at_version_one():
    policy = make_structured_policy()

    expected = textwrap.dedent(
        """\
        filesystem_policy:
          include_workdir: true
          read_only:
          - /usr
          read_write:
          - /sandbox
        landlock:
          compatibility: best_effort
        process:
          run_as_user: sandbox
          run_as_group: sandbox
        network_policies:
          model-api:
            endpoints:
            - host: api.anthropic.com
              port: 443
              protocol: rest
              enforcement: enforced
              access: allow
              methods:
              - GET
            binaries:
            - path: /usr/local/bin/claude
        """
    )
    assert policy.yaml_text == expected
    assert policy.version == 1
    assert policy.yaml_source == "structured"
    assert policy.structured == yaml.safe_load(expected)
    assert services.validate_policy_yaml(policy.yaml_text) == {
        "valid": True,
        "errors": [],
        "warnings": [],
    }


def test_documented_example_and_network_errors_are_checked():
    valid = textwrap.dedent(
        """\
        filesystem_policy:
          include_workdir: true
          read_only: ["/usr"]
          read_write: ["/sandbox"]
        landlock:
          compatibility: best_effort
        process:
          run_as_user: sandbox
          run_as_group: sandbox
        network_policies:
          model-api:
            endpoints:
              - host: api.anthropic.com
                port: 443
                protocol: rest
                enforcement: enforced
                access: allow
                methods: [GET, POST]
            binaries:
              - path: /usr/local/bin/claude
        """
    )
    assert services.validate_policy_yaml(valid)["valid"] is True

    for document, message in (
        ({"network_policies": []}, "network_policies must be a mapping"),
        (
            {"network_policies": {"api": {"endpoints": [{"host": "x", "port": 70000}]}}},
            "port must be an integer",
        ),
        ({"landlock": {"compatibility": "sometimes"}}, "landlock.compatibility"),
        ({"filesystem_policy": {"read_only": "/usr"}}, "read_only must be a list"),
    ):
        result = services.validate_document(document)
        assert result["valid"] is False
        assert any(message in error for error in result["errors"])


def test_static_section_change_needs_a_new_nest_network_change_hot_reloads(monkeypatch):
    policy = make_structured_policy()
    effective = copy.deepcopy(policy.structured)

    network_only = copy.deepcopy(effective)
    network_only["network_policies"] = {}
    monkeypatch.setattr(
        "apps.openshell.adapter.get_adapter",
        lambda: type("Gateway", (), {"get_policy": lambda self, name: yaml.safe_dump(network_only)})(),
    )
    diff = services.diff_against_effective(policy)
    assert diff["changed_sections"] == ["network_policies"]
    assert diff["hot_reload"] is True
    assert diff["restart_required"] is False

    static_only = copy.deepcopy(effective)
    static_only["process"]["run_as_user"] = "other"
    monkeypatch.setattr(
        "apps.openshell.adapter.get_adapter",
        lambda: type("Gateway", (), {"get_policy": lambda self, name: yaml.safe_dump(static_only)})(),
    )
    diff = services.diff_against_effective(policy)
    assert diff["changed_sections"] == ["process"]
    assert diff["hot_reload"] is False
    assert diff["restart_required"] is True

    monkeypatch.setattr(
        "apps.openshell.adapter.get_adapter",
        lambda: type("Gateway", (), {"get_policy": lambda self, name: ""})(),
    )
    diff = services.diff_against_effective(policy)
    assert diff["initial_apply"] is True
    assert diff["restart_required"] is False


def test_switching_raw_to_structured_keeps_fields_the_form_does_not_model():
    raw = textwrap.dedent(
        """\
        filesystem_policy:
          include_workdir: true
          read_only: ["/usr"]
          keep_fs: yes
        landlock:
          compatibility: best_effort
          keep_landlock: 1
        process:
          run_as_user: sandbox
          run_as_group: sandbox
        seccomp:
          profile: strict
        network_policies:
          api:
            endpoints:
            - host: api.example.test
              port: 443
              protocol: rest
              enforcement: enforced
              access: allow
              tls_verify: true
            binaries:
            - path: /bin/agent
              architecture: arm64
        """
    )
    policy = Policy.objects.create(name="raw-unknowns", yaml_source="raw", yaml_text=raw)
    services.save_policy(policy, source="raw", action="policy.create", force_audit=True)
    policy.refresh_from_db()

    assert policy.yaml_text == raw
    assert policy.network_rules.count() == 1
    assert policy.filesystem_rules.count() == 1

    structured = services.save_policy(policy, source="structured", action="policy.update")
    document = yaml.safe_load(structured.yaml_text)

    assert document["seccomp"] == {"profile": "strict"}
    assert document["filesystem_policy"]["keep_fs"] is True or document["filesystem_policy"]["keep_fs"] == "yes"
    assert document["landlock"]["keep_landlock"] == 1
    endpoint = document["network_policies"]["api"]["endpoints"][0]
    assert endpoint["tls_verify"] is True
    assert document["network_policies"]["api"]["binaries"][0]["architecture"] == "arm64"
    # The render is still a valid OpenShell document.
    assert services.validate_policy_yaml(structured.yaml_text)["valid"] is True


# --- revisions, audit and diff ----------------------------------------------------------------

def test_each_change_records_the_previous_document_and_an_audit_event(admin_user):
    policy = make_structured_policy()
    old_yaml = policy.yaml_text
    old_version = policy.version

    ProcessRule.objects.filter(policy=policy).update(run_as_user="builder")
    saved = services.save_policy(
        policy, user=admin_user, note="run as builder", source="structured"
    )

    assert saved.version == old_version + 1
    revision = saved.revisions.get(version=old_version)
    assert revision.yaml_text == old_yaml
    assert revision.author == admin_user
    assert revision.note == "run as builder"

    event = AuditEvent.objects.filter(action="policy.update", target_id=str(saved.pk)).latest("created_at")
    assert event.actor == admin_user
    assert "diff" in event.diff

    diff = services.diff_texts(revision.yaml_text, saved.yaml_text)
    assert any("run_as_user" in line for line in diff)


def test_saving_without_changes_does_not_create_noise():
    policy = make_structured_policy()
    revisions = PolicyRevision.objects.filter(policy=policy).count()

    same = services.save_policy(policy, source="structured")

    assert same.version == policy.version
    assert PolicyRevision.objects.filter(policy=policy).count() == revisions


def test_rollback_restores_the_document_exactly(api_client, admin_user):
    policy = make_structured_policy()
    original = policy.yaml_text

    ProcessRule.objects.filter(policy=policy).update(run_as_user="builder")
    services.save_policy(policy, user=admin_user, source="structured")
    policy.refresh_from_db()
    assert policy.yaml_text != original

    response = api_client.post(
        f"/api/v1/policies/{policy.pk}/rollback/", {"version": 1}, format="json"
    )
    assert response.status_code == 200, response.content
    policy.refresh_from_db()
    assert policy.yaml_text == original


# --- API --------------------------------------------------------------------------------------

def test_api_exposes_yaml_source_landlock_and_templates(api_client):
    policy = make_structured_policy()
    response = api_client.get(f"/api/v1/policies/{policy.pk}/")
    assert response.status_code == 200
    body = response.json()
    assert body["yaml_source"] == "structured"
    assert body["landlock"] == {
        "id": policy.landlock.pk,
        "compatibility": "best_effort",
        "include_workdir": True,
    }
    assert body["network_rules"][0]["endpoint_host"] == "api.anthropic.com"

    templates = api_client.get("/api/v1/policies/templates/")
    assert templates.status_code == 200
    keys = {item["key"] for item in templates.json()}
    assert {"coding-agent", "offline", "audit-web"} <= keys

    saved = api_client.get("/api/v1/policy-templates/")
    assert saved.status_code == 200
    assert len(saved.json()) >= 3


def test_api_rule_mutations_regenerate_the_document(api_client):
    policy = make_structured_policy()
    policy.version = 1
    policy.save()

    response = api_client.patch(
        f"/api/v1/policies/{policy.pk}/",
        {
            "network_rules": [
                {
                    "block": "api",
                    "endpoint_host": "api.example.test",
                    "endpoint_port": 8443,
                    "protocol": "rest",
                    "access": "allow",
                    "enforcement": "audit",
                    "methods": ["POST"],
                    "binaries": ["/bin/agent"],
                }
            ]
        },
        format="json",
    )
    assert response.status_code == 200, response.content
    policy.refresh_from_db()
    document = yaml.safe_load(policy.yaml_text)
    assert document["network_policies"]["api"]["endpoints"][0]["host"] == "api.example.test"
    assert document["network_policies"]["api"]["endpoints"][0]["enforcement"] == "audit"
    assert document["network_policies"]["api"]["binaries"] == [{"path": "/bin/agent"}]
    assert policy.version == 2
    assert policy.revisions.count() == 1


def test_api_raw_rule_mutation_is_refused(api_client):
    policy = Policy.objects.create(
        name="raw-only", yaml_source="raw", yaml_text="network_policies: {}\n"
    )
    response = api_client.post(
        "/api/v1/network-rules/",
        {"policy": policy.pk, "block": "api", "endpoint_host": "x.test"},
        format="json",
    )
    assert response.status_code == 400
    assert "raw mode" in str(response.content)


def test_api_yaml_edit_switches_a_structured_policy_to_raw(api_client):
    policy = make_structured_policy()
    new_text = "a: 2\n"
    response = api_client.patch(
        f"/api/v1/policies/{policy.pk}/", {"yaml_text": new_text}, format="json"
    )
    assert response.status_code == 200, response.content
    policy.refresh_from_db()
    assert policy.yaml_source == "raw"
    assert policy.yaml_text == new_text
    assert policy.version == 2
    assert policy.revisions.count() == 1


# --- shared presets ---------------------------------------------------------------------------

def test_presets_are_shared_and_create_structured_policies():
    assert services.ensure_builtin_templates() == len(presets.BUILTIN_PRESETS)
    assert services.ensure_builtin_templates() == len(presets.BUILTIN_PRESETS)
    assert PolicyTemplate.objects.filter(is_builtin=True).count() == len(presets.BUILTIN_PRESETS)

    policy = services.create_policy_from_template("offline", name="offline-policy")
    assert policy.yaml_source == "structured"
    assert policy.version == 1
    assert policy.structured["network_policies"] == {}
    assert policy.filesystem_rules.count() == 6


# --- admin ------------------------------------------------------------------------------------

def test_admin_change_page_renders_unfold_inlines_with_beginner_help(http_client):
    policy = make_structured_policy()
    response = http_client.get(f"/admin/policies/policy/{policy.pk}/change/")
    assert response.status_code == 200, response.content
    assert b"Structured: edit the rule tables" in response.content
    assert b"Network rules (hot-reloadable)" in response.content
    assert b"Filesystem rules (fixed at nest start)" in response.content


def test_admin_inlines_use_the_unfold_base_class():
    from apps.policies.admin import NetworkRuleInline

    assert issubclass(NetworkRuleInline, TabularInline)


def test_legacy_imports_are_marked_raw_by_the_data_migration():
    # Simulate a row from the migration that added yaml_source.
    legacy = Policy.objects.create(
        name="legacy", yaml_source="structured", yaml_text="network_policies: {}\n"
    )
    migration = importlib.import_module(
        "apps.policies.migrations.0002_policytemplate_policy_yaml_source_landlockrule"
    )
    migration.preserve_legacy_documents(django_apps, None)
    legacy.refresh_from_db()
    assert legacy.yaml_source == "raw"
    assert legacy.yaml_text == "network_policies: {}\n"


# --- nest configuration -----------------------------------------------------------------------

def test_policy_nest_section_validates_an_existing_active_policy():
    from apps.policies.nest_config import SECTIONS

    section = SECTIONS[0]
    assert section.key == "policy"
    assert "policy" in section.fields

    policy = Policy.objects.create(name="chosen", is_active=True)
    form_type = type("PolicyConfigForm", (forms.Form,), section.fields.copy())
    form = form_type(data={"policy": str(policy.pk)})
    assert form.is_valid(), form.errors
    assert form.cleaned_data["policy"] == policy.pk

    missing = form_type(data={"policy": "999999"})
    assert not missing.is_valid()
    assert "existing policy" in missing.errors["policy"][0].lower()


def test_nest_spec_accepts_the_policy_section():
    policy = Policy.objects.create(name="spec-policy", is_active=True)
    cleaned = validate_spec({"policy": {"policy": policy.pk}})
    assert cleaned == {"policy": {"policy": policy.pk}}

    with pytest.raises(ValidationError):
        validate_spec({"policy": {"policy": 999999}})
