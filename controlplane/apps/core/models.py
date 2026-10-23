"""Core inventory: nodes and overlay peers.

Container and service models moved to ``apps/containers`` when containers became a module.
"""
from __future__ import annotations

from django.db import models
from django.utils import timezone


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


class ModuleState(models.Model):
    """What the operator wants a module to do on the next start.

    The catalog itself is code (baked into the image); this table only remembers choices and
    whether a restart is still outstanding.
    """

    key = models.CharField(max_length=40, unique=True)
    enabled = models.BooleanField(default=True)
    pending_restart = models.BooleanField(default=False)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["key"]

    def __str__(self) -> str:
        return f"{self.key}: {'on' if self.enabled else 'off'}"
