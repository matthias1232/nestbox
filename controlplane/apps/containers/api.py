"""API surface of the Containers module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from drf_spectacular.utils import OpenApiResponse, extend_schema, extend_schema_view
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.errors import NestboxError, NotFound
from apps.api.permissions import OperatorOnly, ViewerOnly
from apps.containers.models import Container, Service
from apps.core.models import Node
from apps.core.runtime import runtime
from apps.identity.services import audit


class NodeSerializer(serializers.ModelSerializer):
    class Meta:
        model = Node
        fields = "__all__"
        read_only_fields = ["created_at", "last_seen"]


class ServiceSerializer(serializers.ModelSerializer):
    class Meta:
        model = Service
        fields = ["id", "name", "port", "url", "tls_domain", "is_public", "created_at"]
        read_only_fields = ["created_at"]


class ContainerSerializer(serializers.ModelSerializer):
    services = ServiceSerializer(many=True, read_only=True)

    class Meta:
        model = Container
        fields = [
            "id",
            "node",
            "name",
            "container_id",
            "image",
            "image_digest",
            "role",
            "state",
            "health",
            "status_text",
            "ports",
            "labels",
            "services",
            "started_at",
            "updated_at",
        ]
        read_only_fields = ["container_id", "image", "image_digest", "state", "health", "updated_at"]


class ContainerActionSerializer(serializers.Serializer):
    verb = serializers.ChoiceField(choices=["start", "stop", "restart", "kill"])


class NodeViewSet(viewsets.ModelViewSet):
    queryset = Node.objects.all()
    serializer_class = NodeSerializer
    permission_classes = [ViewerOnly]
    filterset_fields = ["is_local"]


@extend_schema_view(
    destroy=extend_schema(description="Containers are reconciled from the runtime; deleting a row hides stale data."),
)
class ContainerViewSet(viewsets.ModelViewSet):
    queryset = Container.objects.select_related("node").prefetch_related("services")
    serializer_class = ContainerSerializer
    permission_classes = [OperatorOnly]
    lookup_field = "name"
    search_fields = ["name", "image"]
    ordering_fields = ["name", "role", "state"]
    http_method_names = ["get", "post", "head", "options"]

    @extend_schema(
        tags=["containers"],
        summary="Reconcile the inventory with the container runtime",
        request=None,
        responses={200: OpenApiResponse(description="Number of containers synchronised.")},
    )
    @action(detail=False, methods=["post"], url_path="sync")
    def sync(self, request):  # noqa: ANN001
        touched = runtime.sync()
        return Response({"synced": touched})

    @extend_schema(
        tags=["containers"],
        summary="Start, stop, restart or kill a container",
        request=ContainerActionSerializer,
        responses={200: OpenApiResponse(description="New state.")},
    )
    @action(detail=True, methods=["post"], url_path="actions")
    def act(self, request, name=None):  # noqa: ANN001
        serializer = ContainerActionSerializer(data=request.data)
        serializer.is_valid(raise_exception=True)
        verb = serializer.validated_data["verb"]
        result = runtime.action(name, verb)
        audit(request.user, f"container.{verb}", "container", name, f"{verb} {name}")
        return Response(result)

    @extend_schema(tags=["containers"], summary="Container logs (last N lines, not streamed)")
    @action(detail=True, methods=["get"], url_path="logs")
    def logs(self, request, name=None):  # noqa: ANN001
        tail = int(request.query_params.get("tail", 200))
        runtime.client()
        container = Container.objects.filter(name=name).first()
        if container is None:
            raise NotFound(f"Container {name!r} is unknown.", code="container.not_found")
        try:
            client = runtime.client()
            log = client.containers.get(name).logs(tail=tail, timestamps=True).decode(
                "utf-8", errors="replace"
            )
        except NotFound:
            raise
        except Exception as exc:  # noqa: BLE001
            raise NestboxError(f"Logs unavailable: {exc}", code="container.logs_unavailable") from exc
        return Response({"name": name, "tail": tail, "log": log})


class ServiceViewSet(viewsets.ModelViewSet):
    queryset = Service.objects.select_related("container")
    serializer_class = ServiceSerializer
    permission_classes = [ViewerOnly]


router = DefaultRouter()
router.register("nodes", NodeViewSet, basename="node")
router.register("containers", ContainerViewSet, basename="container")
router.register("services", ServiceViewSet, basename="service")

urlpatterns = router.urls
