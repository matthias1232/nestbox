"""OpenShell gateway registration contracts."""
from __future__ import annotations

import pytest
from apps.openshell.models import Gateway
from django.contrib.auth import get_user_model
from django.contrib.auth.models import Group
from django.core.exceptions import ValidationError
from rest_framework.authtoken.models import Token
from rest_framework.test import APIClient

pytestmark = pytest.mark.django_db


@pytest.fixture()
def client():  # noqa: ANN201
    user = get_user_model().objects.create_user(
        username="admin", email="admin@example.test", password="x"  # noqa: S106
    )
    user.is_superuser = True
    user.save()
    group, _ = Group.objects.get_or_create(name="admin")
    user.groups.add(group)
    token, _ = Token.objects.get_or_create(user=user)
    api_client = APIClient()
    api_client.credentials(HTTP_AUTHORIZATION=f"Token {token.key}")
    return api_client


def test_gateway_credentials_are_write_only_and_redacted(client):  # noqa: ANN001
    response = client.post(
        "/api/v1/gateways/",
        {
            "name": "local",
            "endpoint": "127.0.0.1:50051",
            "auth_mode": "mtls",
            "ca_pem": "ca-secret",
            "client_cert_pem": "cert-secret",
            "client_key_pem": "key-secret",
        },
        format="json",
    )
    assert response.status_code == 201
    body = response.json()
    assert not {"ca_pem", "client_cert_pem", "client_key_pem"} & body.keys()
    assert body["ca_configured"] is True
    assert body["client_cert_configured"] is True
    assert body["client_key_configured"] is True
    assert Gateway.objects.get(pk=body["id"]).client_key_pem == "key-secret"


def test_gateway_readiness_never_includes_credentials(client):  # noqa: ANN001
    gateway = Gateway.objects.create(
        name="local", ca_pem="ca-secret", client_cert_pem="cert-secret", client_key_pem="key-secret"
    )
    response = client.get(f"/api/v1/gateways/{gateway.pk}/readiness/")
    assert response.status_code == 200
    assert response.json()["status"] in {"configured", "simulated"}
    assert "secret" not in str(response.json())


def test_active_mtls_gateway_requires_all_credentials(client):
    gateway = Gateway(name="missing-creds")
    with pytest.raises(ValidationError):
        gateway.full_clean()
