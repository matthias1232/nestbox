"""Policy admin: a structured editor for beginners, raw YAML for everyone else.

The admin never invents a second source of truth. In structured mode the rule tables are edited and
the document is rendered on save. In raw mode the document is edited and the tables are read-only
views of it. Both paths go through the shared policy service, so revisions and audit entries are
identical to the API's.
"""
from __future__ import annotations

from django import forms
from django.contrib import admin, messages
from django.utils.html import format_html
from unfold.admin import ModelAdmin, StackedInline, TabularInline

from apps.policies import render as policy_render
from apps.policies import services
from apps.policies.models import (
    FilesystemRule,
    LandlockRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    PolicyTemplate,
    ProcessRule,
)


class PolicyAdminForm(forms.ModelForm):
    """Validate raw documents before the model form is saved."""

    class Meta:
        model = Policy
        fields = "__all__"

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        self.fields["yaml_source"].help_text = (
            "Structured: edit the rule tables below; Nestbox writes the YAML document for you. "
            "Raw: edit the document yourself; the tables become read-only and are kept only for "
            "display. Choose one mode per policy."
        )
        if "yaml_text" in self.fields:
            self.fields["yaml_text"].help_text = (
                "The exact OpenShell document the gateway receives. In structured mode this field is "
                "read-only and regenerated from the rules. In raw mode comments and fields Nestbox "
                "does not model are preserved byte-for-byte."
            )
            self.fields["yaml_text"].strip = False
            self.fields["yaml_text"].widget = forms.Textarea(
                attrs={"rows": 26, "spellcheck": "false", "class": "vLargeTextField"}
            )
        self.fields["scope"].help_text = (
            "Sandbox policies belong to one nest. Global policies apply where no sandbox policy is set."
        )
        self.fields["is_active"].help_text = (
            "Inactive policies stay in the history but are not offered for selection."
        )

    def clean(self):  # noqa: ANN201
        cleaned = super().clean()
        source = cleaned.get("yaml_source") or getattr(self.instance, "yaml_source", "structured")
        if source == "raw":
            result = services.validate_policy_yaml(cleaned.get("yaml_text") or "")
            if not result["valid"]:
                for error in result["errors"]:
                    self.add_error("yaml_text", error)
        return cleaned


class _PolicyInlineForm(forms.ModelForm):
    """Form fields get beginner-facing help without changing the database schema."""

    class Meta:
        exclude = ["policy"]


class NetworkRuleInlineForm(_PolicyInlineForm):
    class Meta(_PolicyInlineForm.Meta):
        model = NetworkRule

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        if "block" in self.fields:
            self.fields["block"].help_text = (
                "Group name shown in the document, for example 'model-api'. Rules in the same "
                "group share one binary allow-list."
            )
        if "endpoint_host" in self.fields:
            self.fields["endpoint_host"].help_text = "DNS name or IP address the agent may reach."
        if "endpoint_port" in self.fields:
            self.fields["endpoint_port"].help_text = "Usually 443 for HTTPS."
        if "protocol" in self.fields:
            self.fields["protocol"].help_text = "The OpenShell protocol, normally 'rest'."
        if "access" in self.fields:
            self.fields["access"].help_text = "Allow the endpoint or carve a deny exception out of a wider block."
        if "enforcement" in self.fields:
            self.fields["enforcement"].help_text = (
                "Enforced blocks traffic. Audit records the attempt and lets it through while you build the policy."
            )
        if "methods" in self.fields:
            self.fields["methods"].help_text = 'JSON list of HTTP methods, for example ["GET", "POST"]. Empty means any.'
            self.fields["methods"].widget = forms.Textarea(attrs={"rows": 2})
        if "binaries" in self.fields:
            self.fields["binaries"].help_text = (
                'JSON list of executable paths, for example ["/usr/local/bin/claude"]. '
                "Only these binaries may use the endpoint."
            )
            self.fields["binaries"].widget = forms.Textarea(attrs={"rows": 2})
        if "position" in self.fields:
            self.fields["position"].help_text = "Order inside the block; lower numbers appear first."


class FilesystemRuleInlineForm(_PolicyInlineForm):
    class Meta(_PolicyInlineForm.Meta):
        model = FilesystemRule

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        if "path" in self.fields:
            self.fields["path"].help_text = "Absolute path inside the nest, for example /usr or /sandbox."
        if "mode" in self.fields:
            self.fields["mode"].help_text = "Read-only or read-write. This is fixed when the nest starts."


class ProcessRuleInlineForm(_PolicyInlineForm):
    class Meta(_PolicyInlineForm.Meta):
        model = ProcessRule

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        if "run_as_user" in self.fields:
            self.fields["run_as_user"].help_text = "Unix user the agent runs as. Defaults to 'sandbox'."
        if "run_as_group" in self.fields:
            self.fields["run_as_group"].help_text = "Unix group the agent runs as. Defaults to 'sandbox'."


class LandlockInlineForm(_PolicyInlineForm):
    class Meta(_PolicyInlineForm.Meta):
        model = LandlockRule

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        if "compatibility" in self.fields:
            self.fields["compatibility"].help_text = (
                "'strict' refuses unsupported restrictions; 'best_effort' applies what the host supports. "
                "OpenShell fixes this when the nest starts, so a change needs a new nest."
            )
        if "include_workdir" in self.fields:
            self.fields["include_workdir"].help_text = (
                "Include the nest working directory in the filesystem policy."
            )


class _PolicyInline:
    """Share the raw-mode read-only rule between all inlines."""

    def _is_raw(self, parent, request=None) -> bool:  # noqa: ANN001
        posted = getattr(request, "POST", {}).get("yaml_source") if request is not None else None
        source = posted or getattr(parent, "yaml_source", None)
        return source == "raw"

    def get_readonly_fields(self, request, obj=None):  # noqa: ANN001, ANN002
        fields = list(super().get_readonly_fields(request, obj))
        if self._is_raw(obj, request):
            fields.extend(
                field.name for field in self.model._meta.fields if field.name not in fields
            )
        return fields

    def has_add_permission(self, request, obj=None):  # noqa: ANN001, ANN002
        return not self._is_raw(obj, request) and super().has_add_permission(request, obj)

    def has_delete_permission(self, request, obj=None):  # noqa: ANN001, ANN002
        return not self._is_raw(obj, request) and super().has_delete_permission(request, obj)


class NetworkRuleInline(_PolicyInline, TabularInline):
    model = NetworkRule
    form = NetworkRuleInlineForm
    extra = 0
    fields = [
        "block",
        "endpoint_host",
        "endpoint_port",
        "protocol",
        "access",
        "enforcement",
        "methods",
        "binaries",
        "position",
    ]
    ordering = ["block", "position"]
    verbose_name_plural = "Network rules (hot-reloadable)"


class FilesystemRuleInline(_PolicyInline, TabularInline):
    model = FilesystemRule
    form = FilesystemRuleInlineForm
    extra = 0
    fields = ["path", "mode"]
    verbose_name_plural = "Filesystem rules (fixed at nest start)"


class ProcessRuleInline(_PolicyInline, StackedInline):
    model = ProcessRule
    form = ProcessRuleInlineForm
    extra = 0
    max_num = 1
    fields = ["run_as_user", "run_as_group"]
    verbose_name_plural = "Process identity (fixed at nest start)"


class LandlockInline(_PolicyInline, StackedInline):
    model = LandlockRule
    form = LandlockInlineForm
    extra = 0
    max_num = 1
    fields = ["compatibility", "include_workdir"]
    verbose_name_plural = "Landlock and workdir (fixed at nest start)"


@admin.register(Policy)
class PolicyAdmin(ModelAdmin):
    form = PolicyAdminForm
    list_display = ["name", "scope", "sandbox", "source_badge", "version", "active_badge", "updated_at"]
    list_filter = ["scope", "yaml_source", "is_active"]
    search_fields = ["name", "sandbox__name"]
    autocomplete_fields = ["sandbox"]
    readonly_fields = ["content_hash", "version", "created_at", "updated_at", "document_preview"]
    actions = ["publish_selected_structured", "check_documents"]
    inlines = [LandlockInline, FilesystemRuleInline, ProcessRuleInline, NetworkRuleInline]
    fieldsets = [
        (
            "Policy",
            {
                "fields": ["name", "scope", "sandbox", "is_active", "yaml_source"],
                "description": (
                    "A policy is one OpenShell document. Pick structured mode to edit the tables, "
                    "or raw mode to edit YAML. Nestbox never keeps two competing copies: the mode "
                    "you choose decides which side is written and which side is shown."
                ),
            },
        ),
        (
            "Document",
            {
                "fields": ["yaml_text", "document_preview"],
                "description": (
                    "Network rules can be reloaded on a running nest. Filesystem, Landlock and "
                    "process settings are fixed when a nest starts, so changing them needs a new "
                    "nest. Saving here only updates the desired document — use Apply on the API or "
                    "the policy page to submit it, and read the result the gateway returns."
                ),
            },
        ),
        (
            "Metadata",
            {
                "fields": ["version", "content_hash", "created_at", "updated_at"],
                "description": "Every save keeps the previous document as an immutable revision.",
            },
        ),
    ]

    def get_readonly_fields(self, request, obj=None):  # noqa: ANN001, ANN002
        fields = list(super().get_readonly_fields(request, obj))
        posted = getattr(request, "POST", {}).get("yaml_source")
        source = posted or (getattr(obj, "yaml_source", None) if obj is not None else "structured")
        if source != "raw":
            fields.append("yaml_text")
        return fields

    @admin.display(description="Source", ordering="yaml_source")
    def source_badge(self, obj):  # noqa: ANN001
        colour = "#34d399" if obj.yaml_source == "structured" else "#fbbf24"
        return format_html('<span style="color:{}">●</span> {}', colour, obj.yaml_source)

    @admin.display(description="Active", ordering="is_active")
    def active_badge(self, obj):  # noqa: ANN001
        colour = "#34d399" if obj.is_active else "#8b97b3"
        return format_html('<span style="color:{}">●</span> {}', colour, "active" if obj.is_active else "inactive")

    @admin.display(description="Document preview")
    def document_preview(self, obj):  # noqa: ANN001
        if obj is None or not obj.pk:
            return "Save the policy once to see the generated document."
        text = policy_render.render_yaml(obj) if obj.yaml_source == "structured" else obj.yaml_text
        return format_html("<pre style='white-space:pre-wrap;max-height:28rem;overflow:auto'>{}</pre>", text)

    def save_model(self, request, obj, form, change):  # noqa: ANN001
        obj._previous_yaml = (
            Policy.objects.filter(pk=obj.pk).values_list("yaml_text", flat=True).first() or ""
            if obj.pk
            else ""
        )
        obj._previous_version = (
            Policy.objects.filter(pk=obj.pk).values_list("version", flat=True).first() or 1
            if obj.pk
            else 1
        )
        super().save_model(request, obj, form, change)

    def save_related(self, request, form, formsets, change):  # noqa: ANN001
        super().save_related(request, form, formsets, change)
        instance = form.instance
        policy = self.model.objects.get(pk=instance.pk)
        previous_yaml = getattr(instance, "_previous_yaml", "")
        previous_version = getattr(instance, "_previous_version", 1)
        try:
            saved = services.save_policy(
                policy,
                user=request.user,
                note="saved in admin",
                source=policy.yaml_source,
                yaml_text=policy.yaml_text if policy.yaml_source == "raw" else None,
                previous_yaml=previous_yaml,
                previous_version=previous_version,
                action="policy.update" if change else "policy.create",
                force_audit=not change,
            )
        except services.PolicyValidationError as exc:
            self.message_user(request, " ".join(exc.result.get("errors", [])), messages.ERROR)
            return
        form.instance = saved
        if saved.yaml_source == "structured":
            warnings = services.validate_policy_yaml(saved.yaml_text).get("warnings", [])
            if warnings:
                self.message_user(request, " ".join(warnings), messages.WARNING)

    @admin.action(description="Regenerate the document for structured policies")
    def publish_selected_structured(self, request, queryset):  # noqa: ANN001
        changed = 0
        skipped = 0
        for policy in queryset:
            if policy.yaml_source != "structured":
                skipped += 1
                continue
            services.save_policy(policy, user=request.user, note="republished from structured rules")
            changed += 1
        self.message_user(
            request,
            f"{changed} structured policy(ies) regenerated; {skipped} raw policy(ies) left untouched.",
            messages.SUCCESS,
        )

    @admin.action(description="Validate the document")
    def check_documents(self, request, queryset):  # noqa: ANN001
        for policy in queryset:
            result = services.validate_policy_yaml(policy.yaml_text)
            if result["valid"]:
                detail = "valid"
                level = messages.SUCCESS
            else:
                detail = "; ".join(result["errors"])
                level = messages.ERROR
            if result["warnings"]:
                detail = f"{detail} Warnings: {'; '.join(result['warnings'])}"
            self.message_user(request, f"{policy.name}: {detail}", level)


class PolicyTemplateAdminForm(forms.ModelForm):
    class Meta:
        model = PolicyTemplate
        fields = "__all__"

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        self.fields["document"].help_text = (
            "A complete OpenShell policy document as JSON. Creating a policy from the template "
            "turns it into rule rows and a generated YAML document; it does not apply anything."
        )


@admin.register(PolicyTemplate)
class PolicyTemplateAdmin(ModelAdmin):
    form = PolicyTemplateAdminForm
    list_display = ["name", "builtin_badge", "description", "created_at"]
    search_fields = ["name", "description"]
    readonly_fields = ["created_at"]
    actions = ["install_builtin_presets", "create_policy_from_selected"]

    @admin.display(description="Built-in", ordering="is_builtin")
    def builtin_badge(self, obj):  # noqa: ANN001
        colour = "#34d399" if obj.is_builtin else "#8b97b3"
        return format_html('<span style="color:{}">●</span> {}', colour, "yes" if obj.is_builtin else "no")

    @admin.action(description="Install or refresh the built-in presets")
    def install_builtin_presets(self, request, queryset):  # noqa: ANN001
        count = services.ensure_builtin_templates()
        self.message_user(request, f"{count} built-in preset(s) installed or refreshed.", messages.SUCCESS)

    @admin.action(description="Create a structured policy from the selected template")
    def create_policy_from_selected(self, request, queryset):  # noqa: ANN001
        created = 0
        for template in queryset:
            name = self._unique_policy_name(template.name)
            services.create_policy_from_template(
                template,
                name=name,
                user=request.user,
                note=f"created from template {template.name}",
            )
            created += 1
        self.message_user(request, f"{created} policy(ies) created from templates.", messages.SUCCESS)

    def _unique_policy_name(self, base: str) -> str:
        slug = "".join(ch.lower() if ch.isalnum() else "-" for ch in base).strip("-") or "policy"
        candidate = slug[:100]
        suffix = 2
        while Policy.objects.filter(name=candidate).exists():
            tail = f"-{suffix}"
            candidate = f"{slug[: 100 - len(tail)]}{tail}"
            suffix += 1
        return candidate


@admin.register(PolicyRevision)
class PolicyRevisionAdmin(ModelAdmin):
    list_display = ["policy", "version", "author", "note", "created_at"]
    list_filter = ["created_at"]
    search_fields = ["policy__name", "note"]
    readonly_fields = ["policy", "version", "yaml_text", "author", "note", "created_at"]
    date_hierarchy = "created_at"

    def has_add_permission(self, request):  # noqa: ANN001
        return False

    def has_change_permission(self, request, obj=None):  # noqa: ANN001, ANN002
        return False

    def has_delete_permission(self, request, obj=None):  # noqa: ANN001, ANN002
        return False
