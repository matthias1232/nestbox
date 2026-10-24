"""OpenShell gateway registration and workspace mapping."""
from __future__ import annotations

from django import forms
from django.contrib import admin
from unfold.admin import ModelAdmin, TabularInline

from apps.openshell.models import Gateway, WorkspaceMapping


class GatewayAdminForm(forms.ModelForm):
    """Never render stored operator credentials back into the admin page."""

    class Meta:
        model = Gateway
        fields = "__all__"
        widgets = {
            "ca_pem": forms.PasswordInput(render_value=False),
            "client_cert_pem": forms.PasswordInput(render_value=False),
            "client_key_pem": forms.PasswordInput(render_value=False),
        }

    def clean(self):
        cleaned = super().clean()
        # PasswordInput intentionally posts an empty value when an operator
        # leaves a credential untouched.  Retain the stored value before
        # ModelForm runs Gateway.clean(), while never putting it in initial
        # form data or rendering it in the widget.
        if self.instance.pk:
            for field in ("ca_pem", "client_cert_pem", "client_key_pem"):
                if not cleaned.get(field):
                    cleaned[field] = getattr(self.instance, field)
        return cleaned


class GatewaySettingsAdminMixin:
    """Present gateway registrations as an operator configuration page.

    The runtime currently supports multiple registered endpoints, so this is
    deliberately not a fake singleton.  Operators configure the active
    registration here; capability and version fields are populated by the
    gateway integration and remain informational.
    """

    fieldsets = (
        ("Connection", {"fields": ("name", "endpoint", "is_active")} ),
        (
            "Authentication",
            {
                "description": (
                    "Use no authentication only for the local appliance gateway. "
                    "mTLS requires all three certificate fields. Existing credentials "
                    "are never shown; leave a field blank to keep its current value."
                ),
                "fields": ("auth_mode", "ca_pem", "client_cert_pem", "client_key_pem"),
            },
        ),
        (
            "Detected runtime",
            {
                "description": "Version and capabilities are discovered by the runtime integration.",
                "fields": ("version", "capabilities", "created_at", "updated_at"),
            },
        ),
    )


class WorkspaceMappingInline(TabularInline):
    model = WorkspaceMapping
    extra = 0
    fields = ["workspace", "group"]


@admin.register(Gateway)
class GatewayAdmin(GatewaySettingsAdminMixin, ModelAdmin):
    form = GatewayAdminForm
    list_display = ["name", "endpoint", "auth_mode", "version", "is_active", "updated_at"]
    list_filter = ["auth_mode", "is_active"]
    search_fields = ["name", "endpoint", "version"]
    readonly_fields = ["version", "capabilities", "created_at", "updated_at"]
    inlines = [WorkspaceMappingInline]
    list_filter_submit = True


@admin.register(WorkspaceMapping)
class WorkspaceMappingAdmin(ModelAdmin):
    list_display = ["workspace", "gateway", "group", "created_at"]
    list_filter = ["gateway", "group"]
    search_fields = ["workspace", "gateway__name", "group__name"]
    autocomplete_fields = ["gateway", "group"]
    list_filter_submit = True
