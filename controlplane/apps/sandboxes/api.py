"""API surface of the Nests module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from drf_spectacular.utils import extend_schema
from rest_framework import serializers, status, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.permissions import OperatorOnly
from apps.identity.services import audit
from apps.sandboxes.models import Sandbox, SandboxTemplate
from apps.sandboxes.services import create_sandbox, stop_sandbox


class SandboxTemplateSerializer(serializers.ModelSerializer):
    class Meta:
        model = SandboxTemplate
        fields = "__all__"
        read_only_fields = ["created_at"]


class SandboxSerializer(serializers.ModelSerializer):
    url = serializers.SerializerMethodField()

    class Meta:
        model = Sandbox
        fields = [
            "id",
            "name",
            "openshell_id",
            "workspace",
            "template",
            "agent_pack_key",
            "state",
            "port",
            "spec",
            "overlay_ip",
            "image",
            "last_error",
            "url",
            "created_at",
            "updated_at",
        ]
        read_only_fields = ["openshell_id", "state", "overlay_ip", "last_error", "updated_at"]
        # The service owns name uniqueness so a duplicate returns 409 with a stable code,
        # not a generic field error.
        extra_kwargs = {"name": {"validators": []}}

    def get_url(self, obj: Sandbox) -> str:
        return f"https://{obj.url_host}/"


class SandboxTemplateViewSet(viewsets.ModelViewSet):
    queryset = SandboxTemplate.objects.all()
    serializer_class = SandboxTemplateSerializer
    permission_classes = [OperatorOnly]


class SandboxViewSet(viewsets.ModelViewSet):
    queryset = Sandbox.objects.select_related("template")
    serializer_class = SandboxSerializer
    permission_classes = [OperatorOnly]
    lookup_field = "name"

    @extend_schema(
        tags=["sandboxes"],
        summary="Create a nest from a template or an agent pack",
        responses={201: SandboxSerializer},
    )
    def create(self, request, *args, **kwargs):  # noqa: ANN002, ANN003
        serializer = self.get_serializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        sandbox = create_sandbox(user=request.user, **serializer.validated_data)
        audit(request.user, "sandbox.create", "sandbox", sandbox.name, f"created {sandbox.name}")
        return Response(SandboxSerializer(sandbox).data, status=status.HTTP_201_CREATED)

    @extend_schema(tags=["sandboxes"], summary="Stop a nest without destroying it", request=None)
    @action(detail=True, methods=["post"])
    def stop(self, request, name=None):  # noqa: ANN001
        sandbox = stop_sandbox(name)
        audit(request.user, "sandbox.stop", "sandbox", name, f"stopped {name}")
        return Response(SandboxSerializer(sandbox).data)

    @extend_schema(tags=["sandboxes"], summary="Start a stopped nest", request=None)
    @action(detail=True, methods=["post"])
    def start(self, request, name=None):  # noqa: ANN001
        sandbox = Sandbox.objects.get(name=name)
        sandbox.state = "running"
        sandbox.last_error = ""
        sandbox.save(update_fields=["state", "last_error", "updated_at"])
        audit(request.user, "sandbox.start", "sandbox", name, f"started {name}")
        return Response(SandboxSerializer(sandbox).data)

    @extend_schema(tags=["sandboxes"], summary="Services and reachable URLs of a nest")
    @action(detail=True, methods=["get"])
    def services(self, request, name=None):  # noqa: ANN001
        sandbox = Sandbox.objects.get(name=name)
        return Response(
            {
                "name": sandbox.name,
                "url": f"https://{sandbox.url_host}/",
                "overlay_ip": sandbox.overlay_ip,
                "ports": sandbox.template.agent_pack_key if sandbox.template else None,
            }
        )


router = DefaultRouter()
router.register("templates", SandboxTemplateViewSet, basename="template")
router.register("sandboxes", SandboxViewSet, basename="sandbox")

urlpatterns = router.urls
