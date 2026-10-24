"""OpenShell gateway registration and workspace mapping."""
from __future__ import annotations

from django.contrib import admin
from unfold.admin import ModelAdmin, TabularInline

from apps.openshell.models import Gateway, WorkspaceMapping


class WorkspaceMappingInline(TabularInline):
    model = WorkspaceMapping
    extra = 0
    fields = ["workspace", "group"]


@admin.register(Gateway)
class GatewayAdmin(ModelAdmin):
    list_display = ["name", "endpoint", "auth_mode", "version", "is_active", "updated_at"]
    list_filter = ["auth_mode", "is_active"]
    search_fields = ["name", "endpoint", "version"]
    readonly_fields = ["created_at", "updated_at"]
    inlines = [WorkspaceMappingInline]
    list_filter_submit = True
    fieldsets = [
        ("Gateway", {"fields": ["name", "endpoint", "auth_mode", "version", "is_active"]}),
        ("Capabilities", {"fields": ["capabilities"]}),
        ("Operator credentials", {"fields": ["ca_pem", "client_cert_pem", "client_key_pem"]}),
        ("Metadata", {"fields": ["created_at", "updated_at"]}),
    ]


@admin.register(WorkspaceMapping)
class WorkspaceMappingAdmin(ModelAdmin):
    list_display = ["workspace", "gateway", "group", "created_at"]
    list_filter = ["gateway", "group"]
    search_fields = ["workspace", "gateway__name", "group__name"]
    autocomplete_fields = ["gateway", "group"]
    list_filter_submit = True
