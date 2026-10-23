"""TLS issuance and the network safety net."""
from __future__ import annotations

import pytest
from apps.netconfig.services import apply, plan_change, render_netplan
from apps.network.models import NetworkProfile
from apps.tls.services import days_left, issue_self_signed, parse_expiry, store_certificate

pytestmark = pytest.mark.django_db


def test_self_signed_pair_is_usable():  # noqa: ANN201
    pair = issue_self_signed("nestbox.lab.example")
    assert pair.cert_pem.startswith("-----BEGIN CERTIFICATE-----")
    assert pair.key_pem.startswith("-----BEGIN RSA PRIVATE KEY-----")
    assert parse_expiry(pair.cert_pem) == pair.expires_at


def test_certificate_is_stored_with_an_encrypted_key():  # noqa: ANN201
    pair = issue_self_signed("nestbox.lab.example")
    cert = store_certificate(
        "nestbox.lab.example", source="internal_ca", cert_pem=pair.cert_pem, key_pem=pair.key_pem
    )
    assert cert.status == "valid"
    assert cert.key_pem_encrypted.startswith("enc:v1:")
    assert "BEGIN RSA PRIVATE KEY" not in cert.key_pem_encrypted
    assert days_left(cert) is not None


def test_dhcp_profile_renders_optional_ethernet():  # noqa: ANN201
    profile = NetworkProfile.objects.create(name="dhcp", mode="dhcp", dns_servers=["1.1.1.1"])
    doc = render_netplan(profile)
    eth = doc["network"]["ethernets"]["all-en"]
    assert eth["dhcp4"] is True
    assert eth["nameservers"]["addresses"] == ["1.1.1.1"]


def test_static_change_is_marked_critical_and_needs_confirmation():  # noqa: ANN201
    profile = NetworkProfile.objects.create(
        name="static", mode="static", address="10.1.2.3/24", gateway="10.1.2.1"
    )
    plan = plan_change(profile)
    assert plan.critical is True
    assert any("reachable" in risk.lower() for risk in plan.risks)

    refused = apply(profile, confirm_hostname="wrong-host")
    assert refused["applied"] is False
    assert refused["code"] == "network.confirmation_required"


def test_dry_run_never_touches_the_system():  # noqa: ANN201
    profile = NetworkProfile.objects.create(name="static2", mode="static", address="10.9.9.9/24", gateway="10.9.9.1")
    result = apply(profile, confirm_hostname="whatever", dry_run=True)
    assert result["applied"] is False
    assert result["code"] in {"network.dry_run", "network.confirmation_required"}
