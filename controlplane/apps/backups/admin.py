"""Backup targets, snapshots and restores.

The controller does not run a backup engine from the admin. The admin shows the inventory and
keeps destructive restore operations visible and read-only until a worker exists.
"""
from __future__ import annotations

from django.contrib import admin
from unfold.admin import ModelAdmin

from apps.backups.models import BackupSnapshot, BackupTarget, RestoreJob


@admin.register(BackupTarget)
class BackupTargetAdmin(ModelAdmin):
    list_display = ["name", "kind", "path", "schedule", "retention", "is_active", "created_at"]
    list_filter = ["kind", "is_active"]
    search_fields = ["name", "path"]
    readonly_fields = ["created_at"]
    list_filter_submit = True
    fieldsets = [
        ("Target", {"fields": ["name", "kind", "path", "is_active"]}),
        ("Scope", {"fields": ["include", "exclude"]}),
        ("Schedule", {"fields": ["schedule", "retention"]}),
        ("Metadata", {"fields": ["created_at"]}),
    ]


@admin.register(BackupSnapshot)
class BackupSnapshotAdmin(ModelAdmin):
    list_display = ["__str__", "target", "engine", "repo_id", "size", "created_at"]
    list_filter = ["engine", "target"]
    search_fields = ["target__name", "repo_id"]
    readonly_fields = ["target", "engine", "repo_id", "size", "metadata", "created_at"]
    list_filter_submit = True

    def has_add_permission(self, request) -> bool:  # noqa: ANN001, ARG002
        return False


@admin.register(RestoreJob)
class RestoreJobAdmin(ModelAdmin):
    list_display = ["__str__", "snapshot", "path", "dry_run", "state", "created_at"]
    list_filter = ["state", "dry_run"]
    search_fields = ["snapshot__target__name", "path", "state"]
    readonly_fields = ["state", "log", "created_at"]
    list_filter_submit = True
