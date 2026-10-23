"""Containers and services as the runtime reports them.

The models used to live in ``apps/core``. They moved here when containers became a module of their
own; the physical tables keep their old names (``core_container``, ``core_service``) and the move is
recorded with state-only migrations on both sides, so no data was rewritten.
"""
from __future__ import annotations

from django.db import models

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


class Container(models.Model):
    """A container as reported by the runtime. Desired state lives in the controller."""

    node = models.ForeignKey(
        "core.Node", on_delete=models.CASCADE, related_name="containers"
    )
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
        db_table = "core_container"
        constraints = [
            models.UniqueConstraint(fields=["node", "name"], name="uniq_container_per_node"),
        ]
        indexes = [models.Index(fields=["state"], name="core_contai_state_0164e8_idx")]

    def __str__(self) -> str:
        return self.name

    @property
    def is_running(self) -> bool:
        return self.state == "running"


class Service(models.Model):
    """A port of a container that is reachable at a URL."""

    container = models.ForeignKey(
        Container, on_delete=models.CASCADE, related_name="services"
    )
    name = models.CharField(max_length=100, default="ui")
    port = models.PositiveIntegerField()
    url = models.URLField(max_length=500, blank=True)
    tls_domain = models.CharField(max_length=255, blank=True)
    is_public = models.BooleanField(default=False)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["container", "port"]
        db_table = "core_service"
        constraints = [
            models.UniqueConstraint(
                fields=["container", "name"], name="uniq_service_per_container"
            ),
        ]

    def __str__(self) -> str:
        return f"{self.container.name}:{self.port}"
