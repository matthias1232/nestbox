"""ISO build recipes and jobs.

A recipe is the single source of truth for every deployable artefact: the ISO, the cloud-init
seed, the Brev setup script and the Proxmox provisioning plan. Nothing is duplicated per target;
the renderers read the same fields.
"""
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

TLS_MODES = [
    ("self_signed", "self_signed"),
    ("letsencrypt", "letsencrypt"),
    ("upload", "upload"),
]

ACME_CHALLENGES = [
    ("http01", "http01"),
    ("dns01", "dns01"),
]

TARGETS = [
    ("iso", "iso"),
    ("cloud_init", "cloud_init"),
    ("brev", "brev"),
    ("proxmox", "proxmox"),
]

UBUNTU_SERIES = [
    ("22.04", "22.04"),
    ("24.04", "24.04"),
    ("26.04", "26.04"),
]


class BuildRecipe(models.Model):
    name = models.CharField(max_length=120, unique=True)
    description = models.TextField(blank=True)

    # base
    ubuntu_series = models.CharField(max_length=20, choices=UBUNTU_SERIES, default="24.04")
    profile = models.CharField(max_length=10, choices=BUILD_PROFILES, default="slim")
    target = models.CharField(max_length=20, choices=TARGETS, default="iso")
    locale = models.CharField(max_length=20, default="en_US.UTF-8")
    keyboard = models.CharField(max_length=40, default="us")

    # identity
    hostname = models.CharField(max_length=120, default="nestbox")
    username = models.CharField(max_length=60, default="nestbox")
    ssh_authorized_keys = models.JSONField(default=list, blank=True)
    admin_email = models.CharField(max_length=200, blank=True)
    admin_password_encrypted = models.TextField(blank=True)
    force_password_change = models.BooleanField(default=True)

    # packages and services
    packages = models.JSONField(default=list, blank=True)
    install_docker = models.BooleanField(default=True)
    install_openshell = models.BooleanField(default=True)

    # network
    network_mode = models.CharField(max_length=10, choices=NETWORK_MODES, default="dhcp")
    address = models.CharField(max_length=64, blank=True)
    gateway = models.CharField(max_length=64, blank=True)
    dns = models.JSONField(default=list, blank=True)
    dns_domain = models.CharField(max_length=255, blank=True)
    dns_search = models.JSONField(default=list, blank=True)
    ntp_enabled = models.BooleanField(default=True)
    ntp_servers = models.JSONField(default=list, blank=True)

    # ports
    ui_port = models.PositiveIntegerField(default=443)
    admin_port = models.PositiveIntegerField(default=443)

    # tls
    tls_mode = models.CharField(max_length=20, choices=TLS_MODES, default="self_signed")
    tls_domain = models.CharField(max_length=255, blank=True)
    tls_email = models.CharField(max_length=200, blank=True)
    acme_challenge = models.CharField(max_length=10, choices=ACME_CHALLENGES, default="http01")
    certificate = models.ForeignKey(
        "network.Certificate", null=True, blank=True, on_delete=models.SET_NULL,
        related_name="recipes",
    )

    # workloads started on first boot
    preseed_nests = models.JSONField(
        default=list, blank=True,
        help_text='[{"name": "hermes-01", "pack": "hermes-agent", "port": 8080}]',
    )

    preseed_sandbox = models.ForeignKey(
        "sandboxes.SandboxTemplate", null=True, blank=True, on_delete=models.SET_NULL,
        related_name="build_recipes",
    )

    created_at = models.DateTimeField(auto_now_add=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name

    @property
    def public_host(self) -> str:
        return self.tls_domain or self.hostname

    @property
    def ui_url(self) -> str:
        scheme = "https" if self.tls_mode != "none" else "http"
        port = "" if self.ui_port in {80, 443} else f":{self.ui_port}"
        return f"{scheme}://{self.public_host}{port}/"


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
