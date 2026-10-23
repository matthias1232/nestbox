"""Core inventory: nodes, containers, services, overlay peers."""
from __future__ import annotations

from django.db import models
from django.utils import timezone

CONTAINER_STATES = [
    ("created", "created"),
    ("running", "running"),
    ("paused", "paused"),
    ("restarting", "restarting"),
    ("exited", "exited"),
    ("dead", "dead"),
    ("unknown", "unknown"),
]

CONTAINER_ROLES = [
    ("controller", "controller"),
    ("gateway", "gateway"),
    ("traffic", "traffic"),
    ("dns", "dns"),
    ("nest", "nest"),
    ("other", "other"),
]


class Node(models.Model):
    """A host that runs containers. Single-box today, multi-node ready."""

    name = models.CharField(max_length=100, unique=True)
    hostname = models.CharField(max_length=255, blank=True)
    os_release = models.CharField(max_length=120, blank=True)
    kernel = models.CharField(max_length=120, blank=True)
    gpu_model = models.CharField(max_length=120, blank=True)
    gpu_memory_mb = models.PositiveIntegerField(null=True, blank=True)
    is_local = models.BooleanField(default=True)
    last_seen = models.DateTimeField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name


class Container(models.Model):
    """A container as reported by the runtime. Desired state lives in the controller."""

    node = models.ForeignKey(Node, on_delete=models.CASCADE, related_name="containers")
    name = models.CharField(max_length=200)
    container_id = models.CharField(max_length=64, blank=True, db_index=True)
    image = models.CharField(max_length=400, blank=True)
    image_digest = models.CharField(max_length=100, blank=True)
    role = models.CharField(max_length=20, choices=CONTAINER_ROLES, default="other")
    state = models.CharField(max_length=20, choices=CONTAINER_STATES, default="unknown")
    health = models.CharField(max_length=30, blank=True)
    status_text = models.CharField(max_length=200, blank=True)
    ports = models.JSONField(default=list, blank=True)
    labels = models.JSONField(default=dict, blank=True)
    started_at = models.DateTimeField(null=True, blank=True)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["role", "name"]
        constraints = [
            models.UniqueConstraint(fields=["node", "name"], name="uniq_container_per_node"),
        ]
        indexes = [models.Index(fields=["state"])]

    def __str__(self) -> str:
        return self.name

    @property
    def is_running(self) -> bool:
        return self.state == "running"


class Service(models.Model):
    """A port of a container that is reachable at a URL."""

    container = models.ForeignKey(Container, on_delete=models.CASCADE, related_name="services")
    name = models.CharField(max_length=100, default="ui")
    port = models.PositiveIntegerField()
    url = models.URLField(max_length=500, blank=True)
    tls_domain = models.CharField(max_length=255, blank=True)
    is_public = models.BooleanField(default=False)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["container", "port"]
        constraints = [
            models.UniqueConstraint(fields=["container", "name"], name="uniq_service_per_container"),
        ]

    def __str__(self) -> str:
        return f"{self.container.name}:{self.port}"


class OverlayPeer(models.Model):
    """A WireGuard peer in the appliance overlay."""

    name = models.CharField(max_length=100, unique=True)
    public_key = models.CharField(max_length=64, blank=True)
    overlay_ip = models.GenericIPAddressField(protocol="IPv4")
    endpoint = models.CharField(max_length=255, blank=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["overlay_ip"]

    def __str__(self) -> str:
        return f"{self.name} ({self.overlay_ip})"


def touch_node(node: Node) -> None:
    Node.objects.filter(pk=node.pk).update(last_seen=timezone.now())
