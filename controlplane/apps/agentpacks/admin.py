"""Agent pack catalog: what the image can install, not what is running."""
from __future__ import annotations

from django.contrib import admin
from unfold.admin import ModelAdmin

from apps.agentpacks.models import AgentPack


@admin.register(AgentPack)
class AgentPackAdmin(ModelAdmin):
    list_display = ["name", "key", "vendor", "version", "requires_gpu", "is_builtin", "updated_at"]
    list_filter = ["requires_gpu", "is_builtin", "vendor"]
    search_fields = ["name", "key", "vendor", "description"]
    readonly_fields = ["updated_at"]
    list_filter_submit = True
    fieldsets = [
        ("Identity", {"fields": ["key", "name", "vendor", "version", "description", "docs_url"]}),
        ("Runtime", {"fields": ["image_ref", "entrypoint", "ports", "env_slots", "healthcheck"]}),
        ("Browser and defaults", {"fields": ["browser", "defaults"]}),
        ("Policy and resources", {"fields": ["policy_profile", "requires_gpu", "is_builtin"]}),
        ("Install", {"fields": ["install"]}),
        ("Metadata", {"fields": ["updated_at"]}),
    ]
