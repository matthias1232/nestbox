"""Image Creator UI: wizard, artefact preview, build queue."""
from __future__ import annotations

import io
import json
import zipfile

from django.contrib import messages
from django.contrib.auth.decorators import login_required
from django.http import HttpResponse
from django.shortcuts import get_object_or_404, redirect, render
from django.views.decorators.http import require_POST

from apps.builds.models import BuildJob, BuildRecipe
from apps.builds.services import recipe_warnings, render_all, render_seed
from apps.core.crypto import encrypt
from apps.network.models import Certificate


def _csv(value: str) -> list[str]:
    return [part.strip() for part in (value or "").split(",") if part.strip()]


def _collect_nests(post) -> list[dict]:  # noqa: ANN001
    nests: list[dict] = []
    for index in (1, 2, 3):
        name = (post.get(f"nest_name_{index}") or "").strip()
        if not name:
            continue
        try:
            port = int(post.get(f"nest_port_{index}") or 8080)
        except ValueError:
            port = 8080
        nests.append(
            {
                "name": name,
                "pack": (post.get(f"nest_pack_{index}") or "").strip(),
                "port": port,
            }
        )
    return nests


@login_required
def build_list(request):  # noqa: ANN001
    if request.method == "POST":
        recipe = BuildRecipe.objects.filter(pk=request.POST.get("recipe")).first()
        if recipe:
            job = BuildJob.objects.create(recipe=recipe, state="queued")
            messages.success(request, f"Build #{job.pk} queued for {recipe.name}.")
        return redirect("builds")
    return render(
        request,
        "builds/list.html",
        {
            "recipes": BuildRecipe.objects.all(),
            "jobs": BuildJob.objects.select_related("recipe")[:25],
        },
    )


@login_required
def build_wizard(request):  # noqa: ANN001
    if request.method == "POST":
        name = (request.POST.get("name") or "").strip()
        if not name:
            messages.error(request, "A recipe needs a name.")
            return render(request, "builds/wizard.html", _wizard_context(request))
        recipe, _ = BuildRecipe.objects.update_or_create(
            name=name,
            defaults={
                "description": request.POST.get("description", ""),
                "ubuntu_series": request.POST.get("ubuntu_series", "24.04"),
                "profile": request.POST.get("profile", "slim"),
                "target": request.POST.get("target", "iso"),
                "locale": request.POST.get("locale", "en_US.UTF-8"),
                "keyboard": request.POST.get("keyboard", "us"),
                "hostname": request.POST.get("hostname", "nestbox"),
                "username": request.POST.get("username", "nestbox"),
                "ssh_authorized_keys": [k for k in (request.POST.get("ssh_keys") or "").splitlines() if k.strip()],
                "admin_email": request.POST.get("admin_email", ""),
                "force_password_change": request.POST.get("force_password_change") == "on",
                "install_docker": request.POST.get("install_docker") == "on",
                "install_openshell": request.POST.get("install_openshell") == "on",
                "network_mode": request.POST.get("network_mode", "dhcp"),
                "address": request.POST.get("address", ""),
                "gateway": request.POST.get("gateway", ""),
                "dns": _csv(request.POST.get("dns", "")),
                "dns_domain": request.POST.get("dns_domain", ""),
                "dns_search": _csv(request.POST.get("dns_search", "")),
                "ntp_enabled": request.POST.get("ntp_enabled") == "on",
                "ntp_servers": _csv(request.POST.get("ntp_servers", "")),
                "ui_port": int(request.POST.get("ui_port") or 443),
                "admin_port": int(request.POST.get("admin_port") or 443),
                "tls_mode": request.POST.get("tls_mode", "self_signed"),
                "tls_domain": request.POST.get("tls_domain", ""),
                "tls_email": request.POST.get("tls_email", ""),
                "acme_challenge": request.POST.get("acme_challenge", "http01"),
                "preseed_nests": _collect_nests(request.POST),
            },
        )
        password = request.POST.get("admin_password") or ""
        if password:
            recipe.admin_password_encrypted = encrypt(password)
            recipe.save(update_fields=["admin_password_encrypted"])
        cert_id = request.POST.get("certificate")
        if cert_id:
            recipe.certificate = Certificate.objects.filter(pk=cert_id).first()
            recipe.save(update_fields=["certificate"])
        messages.success(request, f"Recipe {recipe.name} saved.")
        return redirect("build-detail", pk=recipe.pk)
    return render(request, "builds/wizard.html", _wizard_context(request))


def _wizard_context(request) -> dict:  # noqa: ANN001
    from apps.agentpacks.models import AgentPack

    return {
        "packs": AgentPack.objects.all(),
        "certificates": Certificate.objects.all(),
        "warnings": [],
    }


@login_required
def build_detail(request, pk: int):  # noqa: ANN001
    job = get_object_or_404(BuildJob, pk=pk) if BuildJob.objects.filter(pk=pk).exists() else None
    recipe = job.recipe if job else get_object_or_404(BuildRecipe, pk=pk)
    artefacts = render_all(recipe)
    return render(
        request,
        "builds/detail.html",
        {
            "job": job,
            "recipe": recipe,
            "artefacts": artefacts,
            "warnings": recipe_warnings(recipe),
            "seed": render_seed(recipe),
        },
    )


@require_POST
@login_required
def recipe_build(request, pk: int):  # noqa: ANN001
    recipe = get_object_or_404(BuildRecipe, pk=pk)
    job = BuildJob.objects.create(recipe=recipe, state="queued")
    messages.success(request, f"Build #{job.pk} queued.")
    return redirect("build-detail", pk=job.pk)


@login_required
def recipe_bundle(request, pk: int):  # noqa: ANN001
    """Download everything the ISO builder needs, as one zip.

    The appliance has no Ubuntu base ISO on disk, so the creator cannot remaster by itself; it hands
    the operator a bundle that `scripts/iso/build_iso.sh --seed <dir>` consumes unchanged.
    """
    recipe = get_object_or_404(BuildRecipe, pk=pk)
    seed = render_seed(recipe)
    artefacts = render_all(recipe)

    buffer = io.BytesIO()
    with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, body in seed.items():
            archive.writestr(f"seed/{name}", body)
        for name, body in artefacts.items():
            if isinstance(body, str):  # "seed" and "warnings" are not files
                archive.writestr(name, body)
        archive.writestr(
            "warnings.txt",
            "\n".join(f"- {w}" for w in recipe_warnings(recipe)) or "no warnings\n",
        )
        archive.writestr(
            "README.txt",
            "Build the image with:\n\n"
            "  scripts/iso/build_iso.sh --base ubuntu-24.04-live-server-amd64.iso \\\n"
            f"    --out nestbox-{recipe.hostname}.iso --seed seed --payload nestbox.tar.gz\n",
        )
        archive.writestr(
            "recipe.json",
            json.dumps(
                {
                    "name": recipe.name,
                    "hostname": recipe.hostname,
                    "ubuntu_series": recipe.ubuntu_series,
                    "profile": recipe.profile,
                    "ui_port": recipe.ui_port,
                    "tls_mode": recipe.tls_mode,
                    "tls_domain": recipe.tls_domain,
                    "network_mode": recipe.network_mode,
                    "preseed_nests": recipe.preseed_nests,
                },
                indent=2,
            ),
        )

    response = HttpResponse(buffer.getvalue(), content_type="application/zip")
    response["Content-Disposition"] = f'attachment; filename="nestbox-{recipe.name}.zip"'
    return response
