"""Nest admin: typed module sections, lifecycle through the service layer, audit."""
from __future__ import annotations

import copy

from django import forms
from django.contrib import admin, messages
from django.core.exceptions import PermissionDenied, ValidationError
from django.db.models import QuerySet
from django.forms.models import modelform_factory
from django.http import HttpResponseRedirect
from django.utils.html import format_html, format_html_join
from unfold.admin import ModelAdmin

from apps.api.errors import Conflict, NotFound, RuntimeUnavailable
from apps.core.admin_utils import audit_admin_action
from apps.core.nest_sections import NestSection, sections, validate_spec
from apps.sandboxes import services as lifecycle
from apps.sandboxes.models import Sandbox, SandboxTemplate


def section_field_name(section: NestSection, field_name: str) -> str:
    """Admin form name for a module-contributed field.

    A prefix is required because two modules may offer a field with the same short name. The
    database stores the unprefixed name inside the section object.
    """
    return f"section__{section.key}__{field_name}"


class SandboxAdminForm(forms.ModelForm):
    """Form with one fieldset per active module section.

    ``Sandbox.spec`` is never a form field. The form assembles it from typed section fields and
    validates it with the same service the API uses. Sections that belonged to a module which is
    now disabled stay on the record untouched, but their inputs cannot be changed.
    """

    class Meta:
        model = Sandbox
        fields = "__all__"

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        self.sections = sections()
        previous = self.instance.spec if self.instance and isinstance(self.instance.spec, dict) else {}
        for section in self.sections:
            for field_name, base_field in section.fields.items():
                name = section_field_name(section, field_name)
                field = copy.deepcopy(base_field)
                field.label = getattr(base_field, "label", None) or field_name.replace("_", " ").title()
                self.fields[name] = field
                if self.instance and self.instance.pk:
                    if section.key in previous and field_name in previous[section.key]:
                        self.initial[name] = previous[section.key][field_name]
                    elif base_field.initial is not None:
                        self.initial[name] = copy.deepcopy(base_field.initial)

    def clean(self):  # noqa: ANN201
        cleaned_data = super().clean()
        previous = self.instance.spec if self.instance and isinstance(self.instance.spec, dict) else {}
        active_keys = {section.key for section in self.sections}

        # Keep the stored configuration of modules that are no longer active. The validator
        # accepts it only when it is byte-for-byte unchanged, so this is not an edit path.
        data = {
            key: copy.deepcopy(value)
            for key, value in previous.items()
            if key not in active_keys
        }
        for section in self.sections:
            section_data = {}
            for field_name in section.fields:
                value = cleaned_data.get(section_field_name(section, field_name))
                if value is not None:
                    section_data[field_name] = value
            data[section.key] = section_data

        try:
            self.instance.spec = validate_spec(data, previous)
        except ValidationError as exc:
            self.add_error(None, exc)
        return cleaned_data


class SandboxAdmin(ModelAdmin):
    form = SandboxAdminForm
    list_display = [
        "name",
        "state",
        "template",
        "agent_pack_key",
        "port",
        "module_keys",
        "updated_at",
    ]
    list_filter = ["state", "template"]
    search_fields = ["name", "agent_pack_key", "openshell_id"]
    readonly_fields = [
        "openshell_id",
        "state",
        "overlay_ip",
        "last_error",
        "created_by",
        "spec_summary",
        "created_at",
        "updated_at",
    ]
    actions = ["start_nests", "stop_nests", "restart_nests", "delete_nests"]
    list_filter_submit = True

    # -- form ---------------------------------------------------------------------------------
    def get_form(self, request, obj=None, change=False, **kwargs):  # noqa: ANN001, ARG002
        # The change form has dynamically contributed fields that do not exist on the model.
        # Build the ModelForm from the editable model fields plus the form's own section fields;
        # never derive it from the dynamic fieldsets.
        readonly = set(self.get_readonly_fields(request, obj))
        exclude = set(self.get_exclude(request, obj) or [])
        fields = [
            field.name
            for field in self.model._meta.fields
            if field.editable and field.name not in readonly and field.name not in exclude
            and field.name != "spec"
        ]

        def formfield_for_dbfield(db_field, **field_kwargs):  # noqa: ANN001
            return self.formfield_for_dbfield(db_field, request=request, **field_kwargs)

        return modelform_factory(
            self.model,
            form=self.form,
            fields=fields,
            formfield_callback=formfield_for_dbfield,
        )

    def get_fieldsets(self, request, obj=None):  # noqa: ANN001, ARG002
        fieldsets = [
            ("Nest", {"fields": ["name", "workspace", "template", "agent_pack_key", "port", "image"]}),
        ]
        for section in sections():
            fieldsets.append(
                (
                    section.label,
                    {
                        "fields": [
                            section_field_name(section, name) for name in section.fields
                        ],
                        "description": (
                            f"{section.help_text} These values describe desired configuration "
                            "and are not proof that a running gateway has applied them."
                        ),
                    },
                )
            )
        fieldsets.append(
            (
                "Gateway state",
                {
                    "fields": [
                        "state",
                        "openshell_id",
                        "overlay_ip",
                        "last_error",
                        "created_by",
                        "spec_summary",
                        "created_at",
                        "updated_at",
                    ],
                    "description": (
                        "Observed and derived values are read-only. A saved configuration is a "
                        "request to the next lifecycle action, not an applied gateway state."
                    ),
                },
            )
        )
        return fieldsets

    @admin.display(description="Modules")
    def module_keys(self, obj: Sandbox) -> str:
        if not isinstance(obj.spec, dict) or not obj.spec:
            return "—"
        return ", ".join(sorted(obj.spec))

    @admin.display(description="Desired configuration")
    def spec_summary(self, obj: Sandbox):
        if not isinstance(obj.spec, dict) or not obj.spec:
            return "No module configuration stored."
        rows = []
        for key in sorted(obj.spec):
            values = obj.spec[key]
            if isinstance(values, dict):
                rendered = ", ".join(f"{name}={value}" for name, value in sorted(values.items()))
            else:
                rendered = str(values)
            rows.append((key, rendered))
        return format_html_join(
            "",
            "<div><strong>{}</strong>: {}</div>",
            rows,
        )

    # -- lifecycle ----------------------------------------------------------------------------
    def _create_via_service(self, request, obj: Sandbox) -> Sandbox:
        created = lifecycle.create_sandbox(
            user=request.user,
            name=obj.name,
            template=obj.template,
            agent_pack_key=obj.agent_pack_key,
            workspace=obj.workspace,
            image=obj.image,
            port=obj.port,
            spec=obj.spec if isinstance(obj.spec, dict) else {},
        )
        # Desired configuration is controller-owned. Persist it even when a minimal gateway
        # adapter ignores the optional kwargs, without performing a second lifecycle call.
        changed = []
        if created.spec != (obj.spec or {}):
            created.spec = obj.spec or {}
            changed.append("spec")
        if created.port != obj.port:
            created.port = obj.port
            changed.append("port")
        if changed:
            created.save(update_fields=[*changed, "updated_at"])
        return created

    def save_model(self, request, obj, form, change) -> None:  # noqa: ANN001
        if change:
            super().save_model(request, obj, form, change)
            audit_admin_action(
                request,
                "sandbox.update",
                "sandbox",
                obj.pk,
                f"updated desired configuration for {obj.name}",
            )
            return

        created = self._create_via_service(request, obj)
        obj.pk = created.pk
        obj._state.adding = False
        obj.refresh_from_db()
        audit_admin_action(
            request,
            "sandbox.create",
            "sandbox",
            created.pk,
            f"created nest {created.name} through the lifecycle service",
        )
        self.message_user(
            request,
            "The nest was created through the OpenShell lifecycle service. "
            "Configuration is desired state until the gateway confirms it.",
            messages.SUCCESS,
        )

    def add_view(self, request, form_url="", extra_context=None):  # noqa: ANN001
        try:
            return super().add_view(request, form_url, extra_context)
        except (Conflict, ValidationError, RuntimeUnavailable, NotFound) as exc:
            self.message_user(request, str(exc), messages.ERROR)
            return HttpResponseRedirect(request.path)

    def _delete_one(self, request, sandbox: Sandbox) -> None:  # noqa: ANN001
        deleter = getattr(lifecycle, "delete_sandbox", None)
        if deleter is None:
            raise PermissionDenied(
                "Nest deletion is only available through the lifecycle service, which this "
                "image does not provide."
            )
        deleter(sandbox.name)
        audit_admin_action(
            request,
            "sandbox.delete",
            "sandbox",
            sandbox.pk,
            f"deleted nest {sandbox.name} through the lifecycle service",
        )

    def delete_model(self, request, obj) -> None:  # noqa: ANN001
        self._delete_one(request, obj)

    def delete_queryset(self, request, queryset: QuerySet[Sandbox]) -> None:  # noqa: ANN001
        for sandbox in queryset:
            self._delete_one(request, sandbox)

    def _run_lifecycle(self, request, queryset, verb: str, action: str) -> None:  # noqa: ANN001
        handler = getattr(lifecycle, f"{verb}_sandbox", None)
        if handler is None:
            self.message_user(
                request,
                f"The lifecycle service does not provide {verb} on this image.",
                messages.WARNING,
            )
            return
        succeeded = 0
        failed: list[str] = []
        for sandbox in queryset:
            try:
                handler(sandbox.name)
            except (NotFound, Conflict, RuntimeUnavailable, ValidationError) as exc:
                failed.append(f"{sandbox.name}: {exc}")
                continue
            succeeded += 1
            audit_admin_action(
                request,
                action,
                "sandbox",
                sandbox.pk,
                f"{verb} nest through the lifecycle service",
            )
        if succeeded:
            self.message_user(
                request, f"{succeeded} nest(s): {verb} requested.", messages.SUCCESS
            )
        if failed:
            self.message_user(
                request, "Some nests failed: " + "; ".join(failed), messages.ERROR
            )

    @admin.action(description="Start selected nests")
    def start_nests(self, request, queryset) -> None:  # noqa: ANN001
        self._run_lifecycle(request, queryset, "start", "sandbox.start")

    @admin.action(description="Stop selected nests")
    def stop_nests(self, request, queryset) -> None:  # noqa: ANN001
        self._run_lifecycle(request, queryset, "stop", "sandbox.stop")

    @admin.action(description="Restart selected nests")
    def restart_nests(self, request, queryset) -> None:  # noqa: ANN001
        self._run_lifecycle(request, queryset, "restart", "sandbox.start")

    @admin.action(description="Delete selected nests")
    def delete_nests(self, request, queryset) -> None:  # noqa: ANN001
        removed = 0
        for sandbox in queryset:
            try:
                self._delete_one(request, sandbox)
            except PermissionDenied as exc:
                self.message_user(request, str(exc), messages.ERROR)
                return
            except (NotFound, Conflict, RuntimeUnavailable) as exc:
                self.message_user(request, f"{sandbox.name}: {exc}", messages.ERROR)
                continue
            removed += 1
        self.message_user(request, f"{removed} nest(s) deleted.", messages.SUCCESS)


@admin.register(SandboxTemplate)
class SandboxTemplateAdmin(ModelAdmin):
    list_display = ["name", "slug", "agent_pack_key", "cpu", "memory_mb", "gpu",
                    "policy_profile", "is_builtin"]
    list_filter = ["is_builtin", "policy_profile"]
    search_fields = ["name", "slug", "agent_pack_key"]
    readonly_fields = ["created_at"]
    list_filter_submit = True


admin.site.register(Sandbox, SandboxAdmin)
