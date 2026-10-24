"""Container admin: runtime inventory, operator actions and logs."""
from __future__ import annotations

from django.contrib import admin, messages
from django.http import Http404
from django.template.response import TemplateResponse
from django.urls import path, reverse
from django.utils.html import format_html
from unfold.admin import ModelAdmin, TabularInline

from apps.api.errors import NotFound, RuntimeUnavailable
from apps.containers.models import Container, Service
from apps.core.admin_utils import audit_admin_action
from apps.core.runtime import runtime


class ServiceInline(TabularInline):
    model = Service
    extra = 0
    fields = ["name", "port", "url", "tls_domain", "is_public"]


@admin.register(Service)
class ServiceAdmin(ModelAdmin):
    list_display = ["name", "container", "port", "url", "tls_domain", "is_public"]
    list_filter = ["is_public"]
    search_fields = ["name", "url", "tls_domain", "container__name"]
    autocomplete_fields = ["container"]
    list_filter_submit = True


@admin.register(Container)
class ContainerAdmin(ModelAdmin):
    list_display = [
        "name",
        "role",
        "state",
        "health",
        "node",
        "image_short",
        "logs_link",
        "updated_at",
    ]
    list_filter = ["role", "state", "node"]
    search_fields = ["name", "image", "container_id"]
    readonly_fields = [
        "container_id",
        "image_digest",
        "status_text",
        "ports",
        "labels",
        "started_at",
        "updated_at",
        "health",
        "state",
        "role",
        "node",
    ]
    inlines = [ServiceInline]
    actions = ["start_containers", "stop_containers", "restart_containers", "kill_containers"]
    list_filter_submit = True

    def has_add_permission(self, request) -> bool:  # noqa: ANN001, ARG002
        # Rows are reconciliation output from the container runtime, not operator-created.
        return False

    @admin.display(description="Image")
    def image_short(self, obj: Container) -> str:
        return (obj.image or "—").split("@")[0][:48]

    @admin.display(description="Logs")
    def logs_link(self, obj: Container):
        url = reverse("admin:containers_container_logs", args=[obj.pk])
        return format_html('<a class="text-primary-600" href="{}">View logs</a>', url)

    def get_urls(self):
        custom = [
            path(
                "<path:object_id>/logs/",
                self.admin_site.admin_view(self.logs_view),
                name="containers_container_logs",
            ),
        ]
        return custom + super().get_urls()

    def _read_logs(self, container: Container, tail: int = 400) -> tuple[str, str | None]:
        """Read a bounded log tail without the streaming generator used by the CLI."""
        try:
            client = runtime.client()
            remote = client.containers.get(container.name)
            raw = remote.logs(tail=tail, timestamps=True)
        except RuntimeUnavailable as exc:
            return "", str(exc)
        except Exception as exc:  # noqa: BLE001 - Docker reports many concrete exception types
            return "", f"Could not read logs: {exc}"
        if isinstance(raw, bytes):
            return raw.decode("utf-8", errors="replace"), None
        return str(raw), None

    def logs_view(self, request, object_id: str):  # noqa: ANN001
        container = self.get_object(request, object_id)
        if container is None:
            raise Http404("Container not found")
        logs, error = self._read_logs(container)
        audit_admin_action(
            request,
            "other",
            "container",
            container.name,
            "viewed container logs",
        )
        context = {
            **self.admin_site.each_context(request),
            "title": f"Logs · {container.name}",
            "container": container,
            "logs": logs,
            "error": error,
            "opts": self.model._meta,
            "container_url": reverse(
                "admin:containers_container_change", args=[container.pk]
            ),
            "breadcrumbs": [
                ("Containers", reverse("admin:containers_container_changelist")),
                (container.name, reverse("admin:containers_container_change", args=[container.pk])),
                ("Logs", ""),
            ],
        }
        return TemplateResponse(request, "admin/containers/container/logs.html", context)

    def _run_action(self, request, queryset, verb: str) -> None:  # noqa: ANN001
        succeeded = 0
        failed: list[str] = []
        for container in queryset:
            try:
                result = runtime.action(container.name, verb)
            except (RuntimeUnavailable, NotFound) as exc:
                failed.append(f"{container.name}: {exc}")
                continue
            except Exception as exc:  # noqa: BLE001 - a runtime error must not break the list
                failed.append(f"{container.name}: {exc}")
                continue
            if not result.get("ok"):
                failed.append(f"{container.name}: {result.get('reason') or 'failed'}")
                continue
            succeeded += 1
            audit_admin_action(
                request,
                f"container.{verb}",
                "container",
                container.name,
                f"{verb} container from the admin",
            )
        if succeeded:
            self.message_user(
                request, f"{succeeded} container(s): {verb} requested.", messages.SUCCESS
            )
        if failed:
            self.message_user(
                request, "Some containers failed: " + "; ".join(failed), messages.ERROR
            )

    @admin.action(description="Start selected containers")
    def start_containers(self, request, queryset) -> None:  # noqa: ANN001
        self._run_action(request, queryset, "start")

    @admin.action(description="Stop selected containers")
    def stop_containers(self, request, queryset) -> None:  # noqa: ANN001
        self._run_action(request, queryset, "stop")

    @admin.action(description="Restart selected containers")
    def restart_containers(self, request, queryset) -> None:  # noqa: ANN001
        self._run_action(request, queryset, "restart")

    @admin.action(description="Kill selected containers")
    def kill_containers(self, request, queryset) -> None:  # noqa: ANN001
        self._run_action(request, queryset, "kill")
