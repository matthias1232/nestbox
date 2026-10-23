"""API surface of the Policies module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from django.db import transaction
from drf_spectacular.utils import extend_schema
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.errors import NotFound
from apps.api.permissions import OperatorOnly
from apps.identity.services import audit
from apps.policies.models import (
    FilesystemRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    ProcessRule,
)
from apps.policies.services import apply_policy_to_gateway, diff_against_effective, validate_policy_yaml


class NetworkRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = NetworkRule
        fields = "__all__"
        read_only_fields = ["policy"]


class FilesystemRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = FilesystemRule
        fields = "__all__"
        read_only_fields = ["policy"]


class ProcessRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = ProcessRule
        fields = "__all__"
        read_only_fields = ["policy"]


class PolicyRevisionSerializer(serializers.ModelSerializer):
    class Meta:
        model = PolicyRevision
        fields = ["id", "version", "yaml_text", "note", "created_at", "author"]
        read_only_fields = ["version", "created_at"]


class PolicySerializer(serializers.ModelSerializer):
    network_rules = NetworkRuleSerializer(many=True, read_only=True)
    filesystem_rules = FilesystemRuleSerializer(many=True, read_only=True)
    process_rules = ProcessRuleSerializer(many=True, read_only=True)
    revisions = PolicyRevisionSerializer(many=True, read_only=True)

    class Meta:
        model = Policy
        fields = [
            "id",
            "name",
            "scope",
            "sandbox",
            "yaml_text",
            "structured",
            "version",
            "content_hash",
            "is_active",
            "network_rules",
            "filesystem_rules",
            "process_rules",
            "revisions",
            "created_at",
            "updated_at",
        ]
        read_only_fields = ["version", "content_hash", "updated_at"]


class PolicyValidationResult(serializers.Serializer):
    valid = serializers.BooleanField()
    errors = serializers.ListField(child=serializers.CharField(), default=list)
    warnings = serializers.ListField(child=serializers.CharField(), default=list)


class _PolicyChildViewSet(viewsets.ModelViewSet):
    permission_classes = [OperatorOnly]
    policy_param = "policy"

    def get_queryset(self):  # noqa: ANN201
        qs = super().get_queryset()
        policy_id = self.request.query_params.get(self.policy_param)
        return qs.filter(policy_id=policy_id) if policy_id else qs


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


class NetworkRuleViewSet(_PolicyChildViewSet):
    queryset = NetworkRule.objects.all()
    serializer_class = NetworkRuleSerializer


class FilesystemRuleViewSet(_PolicyChildViewSet):
    queryset = FilesystemRule.objects.all()
    serializer_class = FilesystemRuleSerializer


class ProcessRuleViewSet(_PolicyChildViewSet):
    queryset = ProcessRule.objects.all()
    serializer_class = ProcessRuleSerializer


router = DefaultRouter()
router.register("policies", PolicyViewSet, basename="policy")
router.register("network-rules", NetworkRuleViewSet, basename="network-rule")
router.register("filesystem-rules", FilesystemRuleViewSet, basename="filesystem-rule")
router.register("process-rules", ProcessRuleViewSet, basename="process-rule")

urlpatterns = router.urls
