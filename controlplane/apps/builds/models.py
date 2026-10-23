"""ISO build recipes and jobs."""
from __future__ import annotations

from django.db import models

BUILD_PROFILES = [
    ("slim", "slim"),
    ("offline", "offline"),
]

NETWORK_MODES = [
    ("dhcp", "dhcp"),
    ("static", "static"),
]

BUILD_STATES = [
    ("queued", "queued"),
    ("discovering", "discovering"),
    ("rendering", "rendering"),
    ("baking", "baking"),
    ("verifying", "verifying"),
    ("succeeded", "succeeded"),
    ("failed", "failed"),
    ("cancelled", "cancelled"),
]


class BuildRecipe(models.Model):
    """Everything the user configured before a build. Rendered into cloud-init at build time."""

    name = models.CharField(max_length=120, unique=True)
    ubuntu_series = models.CharField(max_length=20, default="24.04")
    profile = models.CharField(max_length=10, choices=BUILD_PROFILES, default="slim")
    locale = models.CharField(max_length=20, default="en_US.UTF-8")
    keyboard = models.CharField(max_length=40, default="us")
    hostname = models.CharField(max_length=120, default="nestbox")
    username = models.CharField(max_length=60, default="nestbox")
    ssh_authorized_keys = models.JSONField(default=list, blank=True)
    packages = models.JSONField(default=list, blank=True)
    network_mode = models.CharField(max_length=10, choices=NETWORK_MODES, default="dhcp")
    address = models.CharField(max_length=64, blank=True)
    gateway = models.CharField(max_length=64, blank=True)
    dns = models.JSONField(default=list, blank=True)
    tls = models.JSONField(default=dict, blank=True)
    proxy = models.JSONField(default=dict, blank=True)
    preseed_sandbox = models.ForeignKey(
        "sandboxes.SandboxTemplate",
        null=True,
        blank=True,
        on_delete=models.SET_NULL,
        related_name="build_recipes",
    )
    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name


class BuildJob(models.Model):
    recipe = models.ForeignKey(BuildRecipe, on_delete=models.CASCADE, related_name="jobs")
    state = models.CharField(max_length=20, choices=BUILD_STATES, default="queued")
    base_iso = models.CharField(max_length=500, blank=True)
    base_iso_sha256 = models.CharField(max_length=64, blank=True)
    artifact_path = models.CharField(max_length=500, blank=True)
    artifact_sha256 = models.CharField(max_length=64, blank=True)
    artifact_size = models.BigIntegerField(default=0)
    log = models.TextField(blank=True)
    error = models.CharField(max_length=500, blank=True)
    started_at = models.DateTimeField(null=True, blank=True)
    finished_at = models.DateTimeField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["-created_at"]

    def __str__(self) -> str:
        return f"{self.recipe.name} #{self.pk} ({self.state})"
