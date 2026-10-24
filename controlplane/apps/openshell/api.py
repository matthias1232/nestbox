"""API surface of the OpenShell module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from drf_spectacular.utils import extend_schema
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly
from apps.openshell.models import Gateway, WorkspaceMapping
from apps.openshell.services import credential_state, readiness


class GatewaySerializer(serializers.ModelSerializer):
    ca_configured = serializers.SerializerMethodField()
    client_cert_configured = serializers.SerializerMethodField()
    client_key_configured = serializers.SerializerMethodField()

    class Meta:
        model = Gateway
        fields = [
            "id", "name", "endpoint", "auth_mode", "version", "capabilities", "is_active",
            "ca_pem", "client_cert_pem", "client_key_pem", "ca_configured",
            "client_cert_configured", "client_key_configured", "created_at", "updated_at",
        ]
        read_only_fields = [
            "created_at", "updated_at", "ca_configured", "client_cert_configured", "client_key_configured",
        ]
        extra_kwargs = {
            "ca_pem": {"write_only": True, "required": False},
            "client_cert_pem": {"write_only": True, "required": False},
            "client_key_pem": {"write_only": True, "required": False},
        }

    def get_ca_configured(self, obj: Gateway) -> bool:
        return credential_state(obj)["ca_configured"]

    def get_client_cert_configured(self, obj: Gateway) -> bool:
        return credential_state(obj)["client_cert_configured"]

    def get_client_key_configured(self, obj: Gateway) -> bool:
        return credential_state(obj)["client_key_configured"]

    def validate(self, attrs: dict) -> dict:
        """Run model validation against the effective update, including secrets."""
        instance = self.instance or Gateway()
        for field, value in attrs.items():
            setattr(instance, field, value)
        instance.full_clean()
        return attrs


class WorkspaceMappingSerializer(serializers.ModelSerializer):
    class Meta:
        model = WorkspaceMapping
        fields = "__all__"
        read_only_fields = ["created_at"]


class GatewayViewSet(viewsets.ModelViewSet):
    queryset = Gateway.objects.all()
    serializer_class = GatewaySerializer
    permission_classes = [AdminOnly]

    @extend_schema(summary="Gateway configuration readiness")
    @action(detail=True, methods=["get"])
    def readiness(self, request, pk=None):  # noqa: ANN001
        return Response(readiness(self.get_object()))


class WorkspaceMappingViewSet(viewsets.ModelViewSet):
    queryset = WorkspaceMapping.objects.select_related("gateway", "group")
    serializer_class = WorkspaceMappingSerializer
    permission_classes = [AdminOnly]


router = DefaultRouter()
router.register("gateways", GatewayViewSet, basename="gateway")
router.register("workspaces", WorkspaceMappingViewSet, basename="workspace")

urlpatterns = router.urls
