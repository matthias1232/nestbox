"""Agent packs: installable agent definitions, shipped as YAML in ``packs/``."""
from __future__ import annotations

from django.db import models


class AgentPack(models.Model):
    key = models.SlugField(max_length=60, unique=True)
    name = models.CharField(max_length=120)
    version = models.CharField(max_length=40, blank=True)
    description = models.TextField(blank=True)
    vendor = models.CharField(max_length=80, blank=True)
    install = models.JSONField(default=dict, blank=True)
    entrypoint = models.JSONField(default=list, blank=True)
    ports = models.JSONField(default=list, blank=True)
    env_slots = models.JSONField(default=list, blank=True)
    healthcheck = models.JSONField(default=dict, blank=True)
    browser = models.JSONField(default=dict, blank=True)
    defaults = models.JSONField(default=dict, blank=True)
    policy_profile = models.CharField(max_length=60, default="coding-agent")
    requires_gpu = models.BooleanField(default=False)
    image_ref = models.CharField(max_length=300, blank=True)
    docs_url = models.URLField(max_length=500, blank=True)
    is_builtin = models.BooleanField(default=False)
    updated_at = models.DateTimeField(auto_now=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name
