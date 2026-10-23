"""API surface.

Every viewset documents itself through drf-spectacular; the role matrix and blast radius are
attached by ``apps.api.schema_hooks.annotate_operations`` so the schema and the permission classes
cannot disagree silently.
"""
from __future__ import annotations

from django.conf import settings
from django.contrib.auth import authenticate
from django.db import transaction
from drf_spectacular.utils import OpenApiResponse, extend_schema, extend_schema_view
from rest_framework import status, viewsets
from rest_framework.authtoken.models import Token
from rest_framework.decorators import action, api_view, permission_classes
from rest_framework.permissions import AllowAny
from rest_framework.response import Response
from rest_framework.views import APIView

from apps.agentpacks.models import AgentPack
from apps.api.errors import NestboxError, NotFound
from apps.api.permissions import AdminOnly, OperatorOnly, ViewerOnly
from apps.api.serializers import (
    AgentPackSerializer,
    AuditEventSerializer,
    BackupSnapshotSerializer,
    BackupTargetSerializer,
    BuildJobSerializer,
    BuildRecipeSerializer,
    CertificateSerializer,
    ContainerActionSerializer,
    ContainerSerializer,
    DnsRecordSerializer,
    FilesystemRuleSerializer,
    GatewaySerializer,
    LanguageSerializer,
    NetworkRuleSerializer,
    NodeSerializer,
    OverlayPeerSerializer,
    PolicySerializer,
    PolicyValidationResult,
    ProcessRuleSerializer,
    ProfileSerializer,
    ProxyRouteSerializer,
    RestoreJobSerializer,
    SandboxSerializer,
    SandboxTemplateSerializer,
    ServiceSerializer,
    TokenRequest,
    TokenResponse,
    WorkspaceMappingSerializer,
)
from apps.backups.models import BackupSnapshot, BackupTarget, RestoreJob
from apps.builds.models import BuildJob, BuildRecipe
from apps.builds.services import render_seed
from apps.core.models import Container, Node, OverlayPeer, Service
from apps.core.runtime import runtime
from apps.identity.models import AuditEvent, Profile
from apps.identity.services import audit, language_catalog
from apps.network.models import Certificate, DnsRecord, ProxyRoute
from apps.openshell.models import Gateway, WorkspaceMapping
from apps.policies.models import FilesystemRule, NetworkRule, Policy, ProcessRule
from apps.policies.services import (
    apply_policy_to_gateway,
    diff_against_effective,
    validate_policy_yaml,
)
from apps.sandboxes.models import Sandbox, SandboxTemplate
from apps.sandboxes.services import create_sandbox, stop_sandbox


class SystemView(APIView):
    permission_classes = [AllowAny]
    authentication_classes: list = []

    @extend_schema(
        tags=["system"],
        summary="Liveness and capability",
        responses={200: OpenApiResponse(description="Version, build and runtime reachability.")},
    )
    def get(self, request):  # noqa: ANN001
        return Response(
            {
                "name": "nestbox",
                "version": settings.NESTBOX_VERSION,
                "runtime": "docker" if runtime.available() else "unavailable",
                "openshell": settings.NESTBOX_OPENSHELL_ENDPOINT,
                "authenticated": request.user.is_authenticated,
            }
        )


class MeView(APIView):
    @extend_schema(tags=["identity"], summary="Current user and role", responses=ProfileSerializer)
    def get(self, request):  # noqa: ANN001
        profile, _ = Profile.objects.get_or_create(user=request.user)
        return Response(ProfileSerializer(profile).data)


class TokenCreateView(APIView):
    permission_classes = [AllowAny]
    authentication_classes: list = []

    @extend_schema(
        tags=["identity"],
        summary="Exchange email and password for an API token",
        request=TokenRequest,
        responses={200: TokenResponse, 401: OpenApiResponse(description="Invalid credentials")},
    )
    def post(self, request):  # noqa: ANN001
        serializer = TokenRequest(data=request.data)
        serializer.is_valid(raise_exception=True)
        user = authenticate(
            request, email=serializer.validated_data["email"], password=serializer.validated_data["password"]
        )
        if user is None:
            raise NestboxError(
                "Email or password is incorrect.",
                code="auth.invalid_credentials",
            )
        token, _ = Token.objects.get_or_create(user=user)
        return Response({"token": token.key})


class LanguageListView(APIView):
    permission_classes = [AllowAny]

    @extend_schema(
        tags=["identity"],
        summary="Available UI and documentation languages",
        responses={200: LanguageSerializer(many=True)},
    )
    def get(self, request):  # noqa: ANN001
        return Response(language_catalog())


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


class PolicyViewSet(viewsets.ModelViewSet):
    queryset = Policy.objects.select_related("sandbox").prefetch_related("revisions")
    serializer_class = PolicySerializer
    permission_classes = [OperatorOnly]
    filterset_fields = ["scope", "sandbox", "is_active"]

    def perform_update(self, serializer):  # noqa: ANN001
        policy = self.get_object()
        with transaction.atomic():
            policy.revisions.create(version=policy.version, yaml_text=policy.yaml_text)
            policy.version += 1
            serializer.save(version=policy.version)

    @extend_schema(
        tags=["policies"],
        summary="Validate policy YAML against the OpenShell schema",
        request=None,
        responses={200: PolicyValidationResult},
    )
    @action(detail=True, methods=["post"])
    def validate(self, request, pk=None):  # noqa: ANN001
        policy = self.get_object()
        result = validate_policy_yaml(policy.yaml_text)
        return Response(result)

    @extend_schema(tags=["policies"], summary="Diff this policy against the gateway's effective policy")
    @action(detail=True, methods=["get"])
    def diff(self, request, pk=None):  # noqa: ANN001
        policy = self.get_object()
        return Response(diff_against_effective(policy))

    @extend_schema(tags=["policies"], summary="Push this policy to the gateway", request=None)
    @action(detail=True, methods=["post"])
    def apply(self, request, pk=None):  # noqa: ANN001
        policy = self.get_object()
        result = apply_policy_to_gateway(policy)
        audit(request.user, "policy.update", "policy", policy.pk, f"applied {policy.name}")
        return Response(result)

    @extend_schema(tags=["policies"], summary="Roll back to a previous revision", request=None)
    @action(detail=True, methods=["post"])
    def rollback(self, request, pk=None):  # noqa: ANN001
        policy = self.get_object()
        version = request.data.get("version")
        revision = policy.revisions.filter(version=version).first()
        if revision is None:
            raise NotFound(f"Revision {version!r} does not exist.", code="policy.revision_not_found")
        policy.revisions.create(version=policy.version, yaml_text=policy.yaml_text, note="before rollback")
        policy.yaml_text = revision.yaml_text
        policy.version += 1
        policy.save()
        audit(request.user, "policy.rollback", "policy", policy.pk, f"rolled back to v{version}")
        return Response(PolicySerializer(policy).data)


class _PolicyChildViewSet(viewsets.ModelViewSet):
    permission_classes = [OperatorOnly]
    policy_param = "policy"

    def get_queryset(self):  # noqa: ANN201
        qs = super().get_queryset()
        policy_id = self.request.query_params.get(self.policy_param)
        return qs.filter(policy_id=policy_id) if policy_id else qs


class NetworkRuleViewSet(_PolicyChildViewSet):
    queryset = NetworkRule.objects.all()
    serializer_class = NetworkRuleSerializer


class FilesystemRuleViewSet(_PolicyChildViewSet):
    queryset = FilesystemRule.objects.all()
    serializer_class = FilesystemRuleSerializer


class ProcessRuleViewSet(_PolicyChildViewSet):
    queryset = ProcessRule.objects.all()
    serializer_class = ProcessRuleSerializer


class AgentPackViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = AgentPack.objects.all()
    serializer_class = AgentPackSerializer
    permission_classes = [ViewerOnly]
    search_fields = ["key", "name", "vendor"]


class GatewayViewSet(viewsets.ModelViewSet):
    queryset = Gateway.objects.all()
    serializer_class = GatewaySerializer
    permission_classes = [AdminOnly]


class WorkspaceMappingViewSet(viewsets.ModelViewSet):
    queryset = WorkspaceMapping.objects.select_related("gateway", "group")
    serializer_class = WorkspaceMappingSerializer
    permission_classes = [AdminOnly]


class CertificateViewSet(viewsets.ModelViewSet):
    queryset = Certificate.objects.all()
    serializer_class = CertificateSerializer
    permission_classes = [OperatorOnly]


class DnsRecordViewSet(viewsets.ModelViewSet):
    queryset = DnsRecord.objects.all()
    serializer_class = DnsRecordSerializer
    permission_classes = [OperatorOnly]
    filterset_fields = ["zone", "rtype"]


class ProxyRouteViewSet(viewsets.ModelViewSet):
    queryset = ProxyRoute.objects.all()
    serializer_class = ProxyRouteSerializer
    permission_classes = [OperatorOnly]


class OverlayPeerViewSet(viewsets.ModelViewSet):
    queryset = OverlayPeer.objects.all()
    serializer_class = OverlayPeerSerializer
    permission_classes = [AdminOnly]


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


class BackupTargetViewSet(viewsets.ModelViewSet):
    queryset = BackupTarget.objects.all()
    serializer_class = BackupTargetSerializer
    permission_classes = [AdminOnly]


class BackupSnapshotViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = BackupSnapshot.objects.select_related("target")
    serializer_class = BackupSnapshotSerializer
    permission_classes = [AdminOnly]


class RestoreJobViewSet(viewsets.ModelViewSet):
    queryset = RestoreJob.objects.select_related("snapshot")
    serializer_class = RestoreJobSerializer
    permission_classes = [AdminOnly]


class AuditEventViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = AuditEvent.objects.select_related("actor")
    serializer_class = AuditEventSerializer
    permission_classes = [AdminOnly]
    filterset_fields = ["action", "target_type"]


class UserViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = Profile.objects.select_related("user").prefetch_related("user__groups")
    serializer_class = ProfileSerializer
    permission_classes = [AdminOnly]


@extend_schema(
    tags=["system"],
    summary="API root: every collection and the documentation entry points",
    responses={200: OpenApiResponse(description="Map of collection URLs and doc endpoints.")},
)
@api_view(["GET"])
@permission_classes([AllowAny])
def api_root(request):
    base = request.build_absolute_uri("/api/v1/")
    return Response(
        {
            "system": f"{base}system/",
            "me": f"{base}me/",
            "containers": f"{base}containers/",
            "sandboxes": f"{base}sandboxes/",
            "templates": f"{base}templates/",
            "policies": f"{base}policies/",
            "agent_packs": f"{base}agent-packs/",
            "builds": f"{base}builds/",
            "backups": f"{base}backups/",
            "audit": f"{base}audit/",
            "schema": request.build_absolute_uri("/api/schema/"),
            "docs": request.build_absolute_uri("/api/docs/"),
            "redoc": request.build_absolute_uri("/api/redoc/"),
        }
    )
