"""Nests: OpenShell sandboxes and their templates."""
from __future__ import annotations

from django.db import models

SANDBOX_STATES = [
    ("pending", "pending"),
    ("starting", "starting"),
    ("running", "running"),
    ("stopped", "stopped"),
    ("failed", "failed"),
    ("deleted", "deleted"),
]

ZERO = 0


class SandboxTemplate(models.Model):
    """A reusable blueprint: agent pack plus resources and a policy profile."""

    name = models.CharField(max_length=100, unique=True)
    slug = models.SlugField(max_length=100, unique=True)
    agent_pack_key = models.CharField(max_length=60)
    description = models.TextField(blank=True)
    cpu = models.DecimalField(max_digits=5, decimal_places=2, default=2)
    memory_mb = models.PositiveIntegerField(default=4096)
    gpu = models.PositiveSmallIntegerField(default=ZERO)
    policy_profile = models.CharField(max_length=60, default="coding-agent")
    is_builtin = models.BooleanField(default=False)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name


class Sandbox(models.Model):
    """A running nest.

    ``openshell_id`` is the gateway's identifier; ``name`` is the human/URL-safe name used in
    ``https://<name>.apps.<domain>``.
    """

    name = models.SlugField(max_length=63, unique=True)
    openshell_id = models.CharField(max_length=120, blank=True, db_index=True)
    workspace = models.CharField(max_length=120, default="default")
    template = models.ForeignKey(
        SandboxTemplate, null=True, blank=True, on_delete=models.SET_NULL, related_name="sandboxes"
    )
    agent_pack_key = models.CharField(max_length=60, blank=True)
    state = models.CharField(max_length=20, choices=SANDBOX_STATES, default="pending")
    port = models.PositiveIntegerField(
        default=0, help_text="Published port for this nest; 0 means the gateway assigns one."
    )
    spec = models.JSONField(
        default=dict,
        blank=True,
        help_text="Werte der aktiven Module, z. B. Netzwerk-Defaults und Policy-Zuordnung.",
    )
    configuration = models.JSONField(
        default=dict,
        blank=True,
        help_text="Pack configuration selected for this nest. Mounts are not supported.",
    )
    policy = models.ForeignKey(
        "policies.Policy",
        null=True,
        blank=True,
        on_delete=models.SET_NULL,
        related_name="assigned_sandboxes",
        help_text="Policy that must be applied before this nest can be started.",
    )
    overlay_ip = models.GenericIPAddressField(protocol="IPv4", null=True, blank=True)
    image = models.CharField(max_length=300, blank=True)
    last_error = models.CharField(max_length=500, blank=True)
    created_by = models.ForeignKey(
        "auth.User", null=True, blank=True, on_delete=models.SET_NULL, related_name="sandboxes"
    )
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]
        indexes = [models.Index(fields=["state"])]

    def __str__(self) -> str:
        return self.name

    @property
    def url_host(self) -> str:
        host = f"{self.name}.apps.localhost"
        return f"{host}:{self.port}" if self.port else host
