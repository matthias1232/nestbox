"""API surface of the ISO builds module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

import io
import json
import zipfile

from django.http import HttpResponse
from drf_spectacular.utils import OpenApiResponse, extend_schema
from rest_framework import serializers, status, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.errors import NotFound
from apps.api.permissions import AdminOnly
from apps.builds.models import BuildJob, BuildRecipe
from apps.builds.services import recipe_warnings, render_all, render_seed
from apps.identity.services import audit


class BuildRecipeSerializer(serializers.ModelSerializer):
    ui_url = serializers.CharField(read_only=True)
    warnings = serializers.SerializerMethodField()

    class Meta:
        model = BuildRecipe
        fields = "__all__"
        read_only_fields = ["created_at", "updated_at"]
        extra_kwargs = {"admin_password_encrypted": {"write_only": True}}

    def get_warnings(self, obj: BuildRecipe) -> list[str]:
        from apps.builds.services import recipe_warnings

        return recipe_warnings(obj)


class BuildJobSerializer(serializers.ModelSerializer):
    recipe_name = serializers.CharField(source="recipe.name", read_only=True)

    class Meta:
        model = BuildJob
        fields = "__all__"
        read_only_fields = [
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


class BuildRecipeViewSet(viewsets.ModelViewSet):
    queryset = BuildRecipe.objects.all()
    serializer_class = BuildRecipeSerializer
    permission_classes = [AdminOnly]

    @extend_schema(
        tags=["builds"],
        summary="Render the cloud-init seed for this recipe (does not build an ISO)",
        responses={200: OpenApiResponse(description="autoinstall.yaml, user-data, meta-data, sha256")},
    )
    @action(detail=True, methods=["get"], url_path="seed")
    def seed(self, request, pk=None):  # noqa: ANN001
        return Response(render_seed(self.get_object()))

    @extend_schema(
        tags=["builds"],
        summary="Render the ISO seed and manifest for this recipe",
        responses={200: OpenApiResponse(description="NoCloud seed, manifest and warnings")},
    )
    @action(detail=True, methods=["get"], url_path="preview")
    def preview(self, request, pk=None):  # noqa: ANN001
        return Response(render_all(self.get_object()))


    @extend_schema(
        tags=["builds"],
        summary="Download the ISO seed bundle (seed files, manifest) as a zip",
        responses={200: OpenApiResponse(description="application/zip")},
    )
    @action(detail=True, methods=["get"], url_path="bundle")
    def bundle(self, request, pk=None):  # noqa: ANN001
        recipe = self.get_object()
        seed = render_seed(recipe)
        artefacts = render_all(recipe)

        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w", zipfile.ZIP_DEFLATED) as archive:
            for name, body in seed.items():
                archive.writestr(f"seed/{name}", body)
            for name, body in artefacts.items():
                if isinstance(body, str):
                    archive.writestr(name, body)
            warnings = recipe_warnings(recipe)
            archive.writestr("warnings.txt", "\n".join(f"- {w}" for w in warnings) or "no warnings\n")
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
            archive.writestr(
                "README.txt",
                "Build the image with:\n\n"
                "  scripts/iso/build_iso.sh --base ubuntu-24.04-live-server-amd64.iso \\\n"
                f"    --out nestbox-{recipe.hostname}.iso --seed seed --payload nestbox.tar.gz\n",
            )

        response = HttpResponse(buffer.getvalue(), content_type="application/zip")
        response["Content-Disposition"] = f'attachment; filename="nestbox-{recipe.name}.zip"'
        return response


class BuildJobViewSet(viewsets.ModelViewSet):
    queryset = BuildJob.objects.select_related("recipe")
    serializer_class = BuildJobSerializer
    permission_classes = [AdminOnly]
    filterset_fields = ["state", "recipe"]

    @extend_schema(tags=["builds"], summary="Queue a build for this recipe", request=None)
    @action(detail=False, methods=["post"], url_path="launch")
    def launch(self, request):  # noqa: ANN001
        recipe_id = request.data.get("recipe")
        recipe = BuildRecipe.objects.filter(pk=recipe_id).first()
        if recipe is None:
            raise NotFound("Recipe not found.", code="build.recipe_not_found")
        job = BuildJob.objects.create(recipe=recipe, state="queued")
        audit(request.user, "build.create", "build", job.pk, f"queued {recipe.name}")
        return Response(BuildJobSerializer(job).data, status=status.HTTP_201_CREATED)


router = DefaultRouter()
router.register("builds/recipes", BuildRecipeViewSet, basename="build-recipe")
router.register("builds/jobs", BuildJobViewSet, basename="build-job")

urlpatterns = router.urls
