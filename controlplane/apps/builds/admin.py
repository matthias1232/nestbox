"""ISO build admin: recipes, rendered downloads and queue visibility."""
from __future__ import annotations

import io
import json
import zipfile

from django import forms
from django.contrib import admin, messages
from django.http import Http404, HttpResponse
from django.urls import path, reverse
from django.utils.html import format_html
from unfold.admin import ModelAdmin, TabularInline

from apps.builds.models import BuildJob, BuildRecipe
from apps.builds.services import recipe_warnings, render_all, render_manifest, render_seed
from apps.core.admin_utils import audit_admin_action
from apps.core.crypto import encrypt
from apps.provisioning.services import render_install_script


class BuildRecipeAdminForm(forms.ModelForm):
    """Recipe form with a write-only bootstrap password.

    The stored field is ciphertext. Presenting it in a textarea would leak it and inviting an
    operator to paste plaintext into the encrypted column would store plaintext. This form keeps
    a blank password as "unchanged" and encrypts anything new before the model is saved.
    """

    warning_list = forms.CharField(
        required=False,
        label="Checks",
        disabled=True,
        widget=forms.Textarea(attrs={"rows": 5}),
        help_text="Derived from the same service that renders the artefacts.",
    )

    class Meta:
        model = BuildRecipe
        fields = "__all__"

    def __init__(self, *args, **kwargs):  # noqa: ANN002, ANN003
        super().__init__(*args, **kwargs)
        password_field = self.fields.get("admin_password_encrypted")
        if password_field is not None:
            password_field.widget = forms.PasswordInput(render_value=False)
            password_field.label = "Bootstrap admin password"
            password_field.help_text = (
                "Stored encrypted. Leave blank to keep the current password."
            )
            password_field.required = False
        if self.instance and self.instance.pk:
            self.fields["warning_list"].initial = "\n".join(recipe_warnings(self.instance))

    def clean_admin_password_encrypted(self):  # noqa: ANN201
        value = (self.cleaned_data.get("admin_password_encrypted") or "").strip()
        if not value:
            return self.instance.admin_password_encrypted if self.instance.pk else ""
        return encrypt(value)


class BuildJobInline(TabularInline):
    model = BuildJob
    extra = 0
    fields = ["state", "artifact_sha256", "artifact_size", "created_at"]
    readonly_fields = fields
    can_delete = False
    show_change_link = True
    max_num = 0


@admin.register(BuildRecipe)
class BuildRecipeAdmin(ModelAdmin):
    form = BuildRecipeAdminForm
    list_display = [
        "name",
        "target",
        "profile",
        "ubuntu_series",
        "hostname",
        "ui_url",
        "download_links",
        "updated_at",
    ]
    list_filter = ["target", "profile", "ubuntu_series", "tls_mode", "network_mode"]
    search_fields = ["name", "hostname", "tls_domain", "admin_email"]
    readonly_fields = ["created_at", "updated_at", "preview_url"]
    inlines = [BuildJobInline]
    actions = ["download_seed_bundles", "download_bundles", "download_manifests", "queue_builds"]
    list_filter_submit = True
    fieldsets = [
        ("Recipe", {"fields": ["name", "description", "target", "profile", "ubuntu_series"]}),
        ("Identity", {"fields": ["hostname", "username", "ssh_authorized_keys", "admin_email",
                                 "admin_password_encrypted", "force_password_change"]}),
        ("Packages and services", {"fields": ["packages", "install_docker", "install_openshell"]}),
        ("Network", {"fields": ["network_mode", "address", "gateway", "dns", "dns_domain",
                                "dns_search", "ntp_enabled", "ntp_servers"]}),
        ("Ports", {"fields": ["ui_port", "admin_port", "preseed_nests"]}),
        ("TLS", {"fields": ["tls_mode", "tls_domain", "tls_email", "acme_challenge",
                            "tls_cert_pem", "tls_key_pem_encrypted"]}),
        ("Locale", {"fields": ["locale", "keyboard", "timezone"]}),
        ("Rendered", {"fields": ["preview_url", "warning_list", "created_at", "updated_at"]}),
    ]

    @admin.display(description="UI URL")
    def ui_url(self, obj: BuildRecipe) -> str:
        return obj.ui_url

    @admin.display(description="Preview")
    def preview_url(self, obj: BuildRecipe):
        if not obj.pk:
            return "Save the recipe to render it."
        url = reverse("admin:builds_buildrecipe_download", args=[obj.pk, "manifest"])
        return format_html('<a class="text-primary-600" href="{}">Manifest JSON</a>', url)

    @admin.display(description="Downloads")
    def download_links(self, obj: BuildRecipe):
        if not obj.pk:
            return "—"
        links = []
        for kind, label in (("seed", "Seed"), ("bundle", "Bundle"), ("manifest", "Manifest")):
            url = reverse("admin:builds_buildrecipe_download", args=[obj.pk, kind])
            links.append(format_html('<a class="text-primary-600" href="{}">{}</a>', url, label))
        return format_html(" · ".join("{}" for _ in links), *links)

    def get_urls(self):
        custom = [
            path(
                "<path:object_id>/download/<str:kind>/",
                self.admin_site.admin_view(self.download_view),
                name="builds_buildrecipe_download",
            ),
        ]
        return custom + super().get_urls()

    # -- rendering ---------------------------------------------------------------------------
    def _zip_bytes(self, recipe: BuildRecipe, kind: str) -> tuple[bytes, str]:
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
            if kind == "seed":
                for name, content in render_seed(recipe).items():
                    archive.writestr(f"seed/{name}", content)
                filename = f"{recipe.name}-seed.zip"
            elif kind == "bundle":
                for name, content in render_seed(recipe).items():
                    archive.writestr(f"seed/{name}", content)
                archive.writestr("install.sh", render_install_script(recipe))
                archive.writestr(
                    "manifest.json",
                    json.dumps(render_all(recipe), indent=2, sort_keys=True, default=str),
                )
                filename = f"{recipe.name}-bundle.zip"
            elif kind == "manifest":
                archive.writestr(
                    "manifest.json",
                    json.dumps(render_manifest(recipe), indent=2, sort_keys=True, default=str),
                )
                filename = f"{recipe.name}-manifest.zip"
            else:
                raise Http404("Unknown download kind")
        return buffer.getvalue(), filename

    def download_view(self, request, object_id: str, kind: str):  # noqa: ANN001
        recipe = self.get_object(request, object_id)
        if recipe is None:
            raise Http404("Recipe not found")
        payload, filename = self._zip_bytes(recipe, kind)
        audit_admin_action(
            request,
            "other",
            "build",
            recipe.pk,
            f"downloaded build {kind} for {recipe.name}",
        )
        response = HttpResponse(payload, content_type="application/zip")
        response["Content-Disposition"] = f'attachment; filename="{filename}"'
        return response

    def _download_for_queryset(self, request, queryset, kind: str) -> None:  # noqa: ANN001
        if queryset.count() != 1:
            self.message_user(
                request,
                "Select exactly one recipe to download.",
                messages.WARNING,
            )
            return
        recipe = queryset.first()
        payload, filename = self._zip_bytes(recipe, kind)
        audit_admin_action(
            request,
            "other",
            "build",
            recipe.pk,
            f"downloaded build {kind} for {recipe.name}",
        )
        response = HttpResponse(payload, content_type="application/zip")
        response["Content-Disposition"] = f'attachment; filename="{filename}"'
        return response

    @admin.action(description="Download NoCloud seed (.zip)")
    def download_seed_bundles(self, request, queryset) -> None:  # noqa: ANN001
        return self._download_for_queryset(request, queryset, "seed")

    @admin.action(description="Download full build bundle (.zip)")
    def download_bundles(self, request, queryset) -> None:  # noqa: ANN001
        return self._download_for_queryset(request, queryset, "bundle")

    @admin.action(description="Download manifest (.zip)")
    def download_manifests(self, request, queryset) -> None:  # noqa: ANN001
        return self._download_for_queryset(request, queryset, "manifest")

    @admin.action(description="Queue a build job")
    def queue_builds(self, request, queryset) -> None:  # noqa: ANN001
        queued = 0
        for recipe in queryset:
            BuildJob.objects.create(recipe=recipe, state="queued")
            queued += 1
            audit_admin_action(
                request,
                "build.create",
                "build",
                recipe.pk,
                f"queued a build job for {recipe.name}",
            )
        self.message_user(request, f"{queued} build job(s) queued.", messages.SUCCESS)

    def save_model(self, request, obj, form, change) -> None:  # noqa: ANN001
        super().save_model(request, obj, form, change)
        audit_admin_action(
            request,
            "build.create",
            "build",
            obj.pk,
            f"{'updated' if change else 'created'} build recipe {obj.name}",
        )


@admin.register(BuildJob)
class BuildJobAdmin(ModelAdmin):
    list_display = ["__str__", "recipe", "state", "artifact_size", "created_at", "finished_at"]
    list_filter = ["state", "recipe"]
    search_fields = ["recipe__name", "artifact_path", "artifact_sha256"]
    readonly_fields = [
        "recipe",
        "state",
        "base_iso",
        "base_iso_sha256",
        "artifact_path",
        "artifact_sha256",
        "artifact_size",
        "log",
        "error",
        "started_at",
        "finished_at",
        "created_at",
    ]
    list_filter_submit = True

    def has_add_permission(self, request) -> bool:  # noqa: ANN001, ARG002
        # Jobs are created from a recipe action so the audit trail names the recipe.
        return False
