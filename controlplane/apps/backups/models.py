"""Backup targets and restic snapshots."""
from __future__ import annotations

from django.db import models

BACKUP_KINDS = [
    ("controller_db", "controller_db"),
    ("config", "config"),
    ("openshell", "openshell"),
    ("sandbox_workspace", "sandbox_workspace"),
    ("certificates", "certificates"),
    ("packs", "packs"),
]


class BackupTarget(models.Model):
    name = models.CharField(max_length=120, unique=True)
    kind = models.CharField(max_length=30, choices=BACKUP_KINDS)
    path = models.CharField(max_length=500)
    include = models.JSONField(default=list, blank=True)
    exclude = models.JSONField(default=list, blank=True)
    schedule = models.CharField(max_length=60, blank=True, help_text="Cron expression or empty for manual")
    retention = models.CharField(max_length=60, default="7d")
    is_active = models.BooleanField(default=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["name"]

    def __str__(self) -> str:
        return self.name


class BackupSnapshot(models.Model):
    target = models.ForeignKey(BackupTarget, on_delete=models.CASCADE, related_name="snapshots")
    engine = models.CharField(max_length=20, default="restic")
    repo_id = models.CharField(max_length=120, blank=True)
    size = models.BigIntegerField(default=0)
    metadata = models.JSONField(default=dict, blank=True)
    created_at = models.DateTimeField(auto_now_add=True, db_index=True)

    class Meta:
        ordering = ["-created_at"]

    def __str__(self) -> str:
        return f"{self.target.name} {self.repo_id or self.pk}"


class RestoreJob(models.Model):
    snapshot = models.ForeignKey(BackupSnapshot, on_delete=models.CASCADE, related_name="restores")
    path = models.CharField(max_length=500, blank=True, help_text="Empty restores everything")
    dry_run = models.BooleanField(default=True)
    state = models.CharField(max_length=20, default="queued")
    log = models.TextField(blank=True)
    created_at = models.DateTimeField(auto_now_add=True)

    class Meta:
        ordering = ["-created_at"]
