"""Gateway registration and workspace mapping."""
from __future__ import annotations

from django.contrib.auth.models import Group
from django.core.exceptions import ValidationError
from django.db import models

AUTH_MODES = [
    ("mtls", "mtls"),
    ("oidc", "oidc"),
    ("none", "none"),
]


class Gateway(models.Model):
    """A registered OpenShell gateway.

    The controller connects with an operator certificate; users never talk to the gateway.
    ``capabilities`` records what the connected gateway version supports, so the adapter can
    degrade instead of crashing across OpenShell 0.1.x breaking changes.
    """

    name = models.CharField(max_length=100, unique=True)
    endpoint = models.CharField(max_length=255, default="127.0.0.1:50051")
    auth_mode = models.CharField(max_length=10, choices=AUTH_MODES, default="mtls")
    version = models.CharField(max_length=40, blank=True)
    capabilities = models.JSONField(default=dict, blank=True)
    ca_pem = models.TextField(blank=True)
    client_cert_pem = models.TextField(blank=True)
    client_key_pem = models.TextField(blank=True)
    is_active = models.BooleanField(default=True)
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name

    def clean(self) -> None:
        """Keep the stored gateway configuration internally consistent.

        Credential material is intentionally optional while a gateway is being
        registered, but an mTLS gateway must not be marked active without a
        complete operator credential set.
        """
        if self.auth_mode == "mtls" and self.is_active:
            missing = [
                field
                for field in ("ca_pem", "client_cert_pem", "client_key_pem")
                if not getattr(self, field)
            ]
            if missing:
                raise ValidationError({field: "Required for an active mTLS gateway." for field in missing})


class WorkspaceMapping(models.Model):
    """Maps an OpenShell workspace to a Django group, so permissions have one source."""

    gateway = models.ForeignKey(Gateway, on_delete=models.CASCADE, related_name="workspaces")
    workspace = models.CharField(max_length=120)
    group = models.ForeignKey(Group, on_delete=models.CASCADE, related_name="workspaces")
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["workspace"]
        constraints = [
            models.UniqueConstraint(fields=["gateway", "workspace"], name="uniq_workspace_per_gateway"),
        ]

    def __str__(self) -> str:
        return f"{self.workspace} -> {self.group.name}"
