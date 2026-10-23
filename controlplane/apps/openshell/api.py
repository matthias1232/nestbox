"""API surface of the OpenShell module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from rest_framework import serializers, viewsets
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly
from apps.openshell.models import Gateway, WorkspaceMapping


class GatewaySerializer(serializers.ModelSerializer):
    class Meta:
        model = Gateway
        fields = "__all__"
        read_only_fields = ["created_at", "updated_at"]
        extra_kwargs = {"client_key_pem": {"write_only": True}}


class WorkspaceMappingSerializer(serializers.ModelSerializer):
    class Meta:
        model = WorkspaceMapping
        fields = "__all__"
        read_only_fields = ["created_at"]


class GatewayViewSet(viewsets.ModelViewSet):
    queryset = Gateway.objects.all()
    serializer_class = GatewaySerializer
    permission_classes = [AdminOnly]


class WorkspaceMappingViewSet(viewsets.ModelViewSet):
    queryset = WorkspaceMapping.objects.select_related("gateway", "group")
    serializer_class = WorkspaceMappingSerializer
    permission_classes = [AdminOnly]


router = DefaultRouter()
router.register("gateways", GatewayViewSet, basename="gateway")
router.register("workspaces", WorkspaceMappingViewSet, basename="workspace")

urlpatterns = router.urls
