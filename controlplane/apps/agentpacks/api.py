"""API surface of the Agent packs module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from rest_framework import serializers, viewsets
from rest_framework.routers import DefaultRouter

from apps.agentpacks.models import AgentPack
from apps.api.permissions import ViewerOnly


class AgentPackSerializer(serializers.ModelSerializer):
    class Meta:
        model = AgentPack
        fields = "__all__"
        read_only_fields = ["updated_at"]


class AgentPackViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = AgentPack.objects.all()
    serializer_class = AgentPackSerializer
    permission_classes = [ViewerOnly]
    search_fields = ["key", "name", "vendor"]


router = DefaultRouter()
router.register("agent-packs", AgentPackViewSet, basename="agent-pack")

urlpatterns = router.urls
