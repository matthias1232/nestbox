"""API surface of the Provisioning module.

Provisioning reads the recipes owned by the builds module and renders deployment artefacts for
them. Its routes are mounted only while the module is active; ``apps/api/urls.py`` composes them
with the other modules.
"""
from __future__ import annotations

import io
import json
import zipfile

from django.http import HttpResponse
from drf_spectacular.utils import OpenApiResponse, extend_schema
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly
from apps.builds.models import BuildRecipe
from apps.provisioning.services import render_provisioning


class ProvisioningRecipeSerializer(serializers.ModelSerializer):
    """A recipe as provisioning sees it: identity and target, no build settings."""

    ui_url = serializers.CharField(read_only=True)

    class Meta:
        model = BuildRecipe
        fields = ["id", "name", "description", "hostname", "target", "ui_url", "updated_at"]
        read_only_fields = fields


class ProvisioningRecipeViewSet(viewsets.ReadOnlyModelViewSet):
    """List recipes and render or download their deployment artefacts.

    Rendering is synchronous and side-effect free: it never queues a build job and never claims
    that an image was produced.
    """

    queryset = BuildRecipe.objects.all()
    serializer_class = ProvisioningRecipeSerializer
    permission_classes = [AdminOnly]

    @extend_schema(
        tags=["provisioning"],
        summary="Render the deployment artefacts for this recipe",
        responses={
            200: OpenApiResponse(
                description="NoCloud seed, install.sh, Brev setup, Proxmox plan, manifest, warnings"
            )
        },
    )
    @action(detail=True, methods=["get"], url_path="render")
    def render(self, request, pk=None):  # noqa: ANN001
        return Response(render_provisioning(self.get_object()))

    @extend_schema(
        tags=["provisioning"],
        summary="Download the deployment bundle as a zip",
        responses={200: OpenApiResponse(description="application/zip")},
    )
    @action(detail=True, methods=["get"], url_path="download")
    def download(self, request, pk=None):  # noqa: ANN001
        recipe = self.get_object()
        artefacts = render_provisioning(recipe)

        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, body in artefacts["seed"].items():
                archive.writestr(f"seed/{name}", body)
            for name in (
                "install.sh",
                "brev-setup.sh",
                "brev-command.txt",
                "proxmox-plan.sh",
                "manifest.json",
            ):
                archive.writestr(name, artefacts[name])
            archive.writestr(
                "warnings.txt",
                "\n".join(f"- {warning}" for warning in artefacts["warnings"]) or "no warnings\n",
            )
            archive.writestr(
                "recipe.json",
                json.dumps(
                    {
                        "name": recipe.name,
                        "hostname": recipe.hostname,
                        "ubuntu_series": recipe.ubuntu_series,
                        "profile": recipe.profile,
                        "target": recipe.target,
                        "ui_port": recipe.ui_port,
                        "tls_mode": recipe.tls_mode,
                        "tls_domain": recipe.tls_domain,
                        "network_mode": recipe.network_mode,
                        "preseed_nests": recipe.preseed_nests,
                    },
                    indent=2,
                ),
            )
            archive.writestr(
                "README.txt",
                "Deployment artefacts rendered from the Nestbox recipe.\n\n"
                "  install.sh        run on an existing Ubuntu host\n"
                "  brev-setup.sh     pass to: brev start --setup-path ./brev-setup.sh\n"
                "  brev-command.txt  the full brev invocation\n"
                "  proxmox-plan.sh   run on the Proxmox host\n"
                "  seed/             the same NoCloud seed the ISO uses\n",
            )

        response = HttpResponse(buffer.getvalue(), content_type="application/zip")
        response["Content-Disposition"] = (
            f'attachment; filename="nestbox-{recipe.name}-provisioning.zip"'
        )
        return response


router = DefaultRouter()
router.register("provisioning/recipes", ProvisioningRecipeViewSet, basename="provisioning-recipe")

urlpatterns = router.urls
