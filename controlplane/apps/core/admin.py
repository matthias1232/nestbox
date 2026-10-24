"""Core admin: nodes, overlay peers and the module switches."""
from __future__ import annotations

from django.contrib import admin, messages
from django.db.models import QuerySet
from django.utils.html import format_html
from unfold.admin import ModelAdmin

from apps.core import modules as runtime
from apps.core import restart
from apps.core.admin_site import NestboxAdminSite
from apps.core.admin_utils import audit_admin_action
from apps.core.models import ModuleState, Node, OverlayPeer
from config.modules import CATALOG

# The catalog is code and its human labels used to be mixed-language. The admin speaks one
# language: the technical key stays stable, the operator sees a concise English name.
MODULE_TITLES = {
    "core": "Core controller",
    "api": "API",
    "identity": "Identity and audit",
    "containers": "Containers",
    "sandboxes": "Nests",
    "agentpacks": "Agent packs",
    "openshell": "OpenShell gateway",
    "policies": "Policies",
    "network": "Network",
    "tls": "Certificates",
    "netconfig": "Network configuration",
    "builds": "ISO builds",
    "provisioning": "Provisioning",
    "backups": "Backups",
}

MODULE_CATEGORIES = {
    "core": "Core",
    "api": "Core",
    "identity": "Core",
    "containers": "Operations",
    "sandboxes": "Operations",
    "agentpacks": "Operations",
    "openshell": "Operations",
    "policies": "Operations",
    "network": "Infrastructure",
    "tls": "Infrastructure",
    "netconfig": "Infrastructure",
    "builds": "Delivery",
    "provisioning": "Delivery",
    "backups": "Operations",
}


def module_title(key: str) -> str:
    return MODULE_TITLES.get(key, key.replace("_", " ").title())


def module_category(key: str) -> str:
    spec = CATALOG.get(key)
    if spec is not None and spec.category in {
        "Core",
        "Operations",
        "Infrastructure",
        "Delivery",
    }:
        return spec.category
    return MODULE_CATEGORIES.get(key, "Other")


@admin.register(Node)
class NodeAdmin(ModelAdmin):
    list_display = ["name", "hostname", "is_local", "gpu_model", "last_seen"]
    search_fields = ["name", "hostname", "gpu_model"]
    list_filter = ["is_local"]
    readonly_fields = ["created_at", "last_seen"]
    list_filter_submit = True


@admin.register(OverlayPeer)
class OverlayPeerAdmin(ModelAdmin):
    list_display = ["name", "overlay_ip", "endpoint", "created_at"]
    search_fields = ["name", "overlay_ip", "endpoint"]
    ordering = ["overlay_ip"]


@admin.register(ModuleState)
class ModuleStateAdmin(ModelAdmin):
    """Module switches: desired state, restart status and safe restart actions.

    Every value on the row is read-only. Enabling or disabling a module has to go through
    :mod:`apps.core.modules`, because that service also resolves dependencies, records the row
    and writes the state file the next process start reads. Letting the change form save
    ``enabled`` directly would bypass all of it.
    """

    list_display = [
        "key",
        "title",
        "category",
        "enabled_badge",
        "pending_badge",
        "toggleable",
        "updated_at",
    ]
    list_filter = ["enabled", "pending_restart"]
    search_fields = ["key"]
    actions = ["enable_modules", "disable_modules", "apply_modules", "restart_controller"]
    change_list_template = "admin/core/modulestate/change_list.html"
    list_filter_submit = True

    def has_add_permission(self, request) -> bool:  # noqa: ANN001, ARG002
        return False

    def has_delete_permission(self, request, obj=None) -> bool:  # noqa: ANN001, ARG002
        return False

    def get_queryset(self, request) -> QuerySet[ModuleState]:  # noqa: ANN001
        runtime.ensure_states()
        return super().get_queryset(request)

    def get_readonly_fields(self, request, obj=None):  # noqa: ANN001, ARG002
        # Deliberately every field: actions are the only supported write path.
        return tuple(field.name for field in ModuleState._meta.fields)

    def changelist_view(self, request, extra_context=None):  # noqa: ANN001
        extra_context = extra_context or {}
        extra_context["nestbox_pending_restart"] = runtime.pending_restart()
        extra_context["nestbox_running_modules"] = sorted(runtime.running())
        extra_context["nestbox_desired_modules"] = list(runtime.desired_keys())
        extra_context["nestbox_restart_available"] = restart.available()
        return super().changelist_view(request, extra_context)

    @admin.display(description="Module")
    def title(self, obj: ModuleState) -> str:
        return module_title(obj.key)

    @admin.display(description="Area")
    def category(self, obj: ModuleState) -> str:
        return module_category(obj.key)

    @admin.display(description="Enabled", boolean=True, ordering="enabled")
    def enabled_badge(self, obj: ModuleState) -> bool:
        return obj.enabled

    @admin.display(description="Restart needed", boolean=True, ordering="pending_restart")
    def pending_badge(self, obj: ModuleState) -> bool:
        return obj.pending_restart

    @admin.display(description="Switchable", boolean=True)
    def toggleable(self, obj: ModuleState) -> bool:
        spec = CATALOG.get(obj.key)
        return bool(spec and spec.toggleable)

    def _set_enabled(self, request, queryset, enabled: bool) -> None:  # noqa: ANN001
        changed = []
        skipped = []
        for row in queryset:
            try:
                runtime.set_enabled(row.key, enabled)
            except (KeyError, ValueError) as exc:
                skipped.append(f"{row.key}: {exc}")
                continue
            changed.append(row.key)
            audit_admin_action(
                request,
                "other",
                "module",
                row.key,
                f"{'enabled' if enabled else 'disabled'} module {row.key}",
            )
        if changed:
            self.message_user(
                request,
                f"{len(changed)} module(s) marked to be {'enabled' if enabled else 'disabled'}. "
                "Restart the controller to apply the change.",
                messages.SUCCESS,
            )
        if skipped:
            self.message_user(
                request, "Not changed: " + "; ".join(skipped), messages.WARNING
            )

    @admin.action(description="Enable selected modules")
    def enable_modules(self, request, queryset) -> None:  # noqa: ANN001
        self._set_enabled(request, queryset, True)

    @admin.action(description="Disable selected modules")
    def disable_modules(self, request, queryset) -> None:  # noqa: ANN001
        self._set_enabled(request, queryset, False)

    @admin.action(description="Apply desired module state")
    def apply_modules(self, request, queryset) -> None:  # noqa: ANN001, ARG002
        effective = runtime.desired_keys()
        pending = runtime.pending_restart()
        audit_admin_action(
            request,
            "other",
            "module",
            "state",
            f"applied desired module state ({len(effective)} modules)",
        )
        message = f"Desired state contains {len(effective)} module(s)."
        if pending:
            message += " A controller restart is still required."
        self.message_user(request, message, messages.INFO)

    @admin.action(description="Restart controller to apply module changes")
    def restart_controller(self, request, queryset) -> None:  # noqa: ANN001, ARG002
        if not runtime.pending_restart():
            self.message_user(
                request,
                "No module change is waiting for a restart. The running state already matches "
                "the desired state.",
                messages.INFO,
            )
            return
        if not restart.available():
            self.message_user(
                request,
                "The controller cannot reach the host container runtime. Restart the "
                "controller container manually, then reload this page.",
                messages.ERROR,
            )
            return
        audit_admin_action(
            request,
            "other",
            "controller",
            "self",
            "requested a controller restart to apply module changes",
        )
        if restart.restart_self():
            self.message_user(
                request,
                "The controller is restarting. This page becomes unavailable for a few seconds.",
                messages.SUCCESS,
            )
        else:
            self.message_user(
                request, "The restart request could not be sent.", messages.ERROR
            )


# Re-export the site class so callers and tests can use a stable import location without
# importing Django's admin registry.
AdminSite = NestboxAdminSite
