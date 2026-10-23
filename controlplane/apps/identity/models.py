"""Profiles, roles and the audit trail."""
from __future__ import annotations

from django.conf import settings
from django.db import models

AUDIT_ACTIONS = [
    ("container.start", "container.start"),
    ("container.stop", "container.stop"),
    ("container.restart", "container.restart"),
    ("container.kill", "container.kill"),
    ("sandbox.create", "sandbox.create"),
    ("sandbox.start", "sandbox.start"),
    ("sandbox.stop", "sandbox.stop"),
    ("sandbox.delete", "sandbox.delete"),
    ("policy.create", "policy.create"),
    ("policy.update", "policy.update"),
    ("policy.rollback", "policy.rollback"),
    ("build.create", "build.create"),
    ("backup.create", "backup.create"),
    ("backup.restore", "backup.restore"),
    ("login", "login"),
    ("secret.update", "secret.update"),
    ("other", "other"),
]

ROLES = ("admin", "operator", "viewer")


class Profile(models.Model):
    user = models.OneToOneField(
        settings.AUTH_USER_MODEL, on_delete=models.CASCADE, related_name="profile"
    )
    language = models.CharField(max_length=10, blank=True)
    timezone = models.CharField(max_length=64, default="UTC")
    created_at = models.DateTimeField(auto_now_add=True)

    def __str__(self) -> str:
        return f"profile({self.user})"

    @property
    def role(self) -> str:
        names = set(self.user.groups.values_list("name", flat=True))
        for role in ROLES:
            if role in names:
                return role
        return "admin" if self.user.is_superuser else "viewer"


class AuditEvent(models.Model):
    """Append-only record of every mutating action."""

    actor = models.ForeignKey(
        settings.AUTH_USER_MODEL,
        null=True,
        blank=True,
        on_delete=models.SET_NULL,
        related_name="audit_events",
    )
    action = models.CharField(max_length=40, choices=AUDIT_ACTIONS)
    target_type = models.CharField(max_length=60, blank=True)
    target_id = models.CharField(max_length=120, blank=True)
    summary = models.CharField(max_length=300, blank=True)
    diff = models.JSONField(default=dict, blank=True)
    request_id = models.UUIDField(null=True, blank=True)
    created_at = models.DateTimeField(auto_now_add=True, db_index=True)

    class Meta:
        ordering = ["-created_at"]
        indexes = [models.Index(fields=["action", "-created_at"])]

    def __str__(self) -> str:
        return f"{self.action} {self.target_type}/{self.target_id}"
