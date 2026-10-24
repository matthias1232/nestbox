"""API surface of the Policies module.

The API follows the same rule as the admin: ``yaml_source`` decides which side is authoritative.
Mutations of rule rows in structured mode regenerate the document and create a revision; editing
``yaml_text`` is a raw edit and switches the policy to raw mode.
"""
from __future__ import annotations

from django.db import transaction
from drf_spectacular.utils import extend_schema
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.exceptions import ValidationError as DrfValidationError
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.errors import NotFound
from apps.api.permissions import OperatorOnly
from apps.policies import presets, services
from apps.policies.models import (
    FilesystemRule,
    LandlockRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    PolicyTemplate,
    ProcessRule,
)


class NetworkRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = NetworkRule
        fields = "__all__"
        extra_kwargs = {"policy": {"required": False}}


class FilesystemRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = FilesystemRule
        fields = "__all__"
        extra_kwargs = {"policy": {"required": False}}


class ProcessRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = ProcessRule
        fields = "__all__"
        extra_kwargs = {"policy": {"required": False}}


class LandlockRuleSerializer(serializers.ModelSerializer):
    class Meta:
        model = LandlockRule
        fields = ["id", "compatibility", "include_workdir"]


class PolicyRevisionSerializer(serializers.ModelSerializer):
    class Meta:
        model = PolicyRevision
        fields = ["id", "version", "yaml_text", "note", "created_at", "author"]
        read_only_fields = ["version", "created_at"]


class PolicyTemplateSerializer(serializers.ModelSerializer):
    class Meta:
        model = PolicyTemplate
        fields = ["id", "name", "description", "document", "is_builtin", "created_at"]
        read_only_fields = ["created_at"]


class PolicySerializer(serializers.ModelSerializer):
    yaml_text = serializers.CharField(allow_blank=True, required=False, trim_whitespace=False)
    network_rules = NetworkRuleSerializer(many=True, read_only=True)
    filesystem_rules = FilesystemRuleSerializer(many=True, read_only=True)
    process_rules = ProcessRuleSerializer(many=True, read_only=True)
    landlock = LandlockRuleSerializer(read_only=True)
    revisions = PolicyRevisionSerializer(many=True, read_only=True)
    template_id = serializers.PrimaryKeyRelatedField(
        queryset=PolicyTemplate.objects.all(), write_only=True, required=False, allow_null=True
    )
    template_key = serializers.CharField(write_only=True, required=False, allow_blank=True)

    class Meta:
        model = Policy
        fields = [
            "id",
            "name",
            "scope",
            "sandbox",
            "yaml_source",
            "yaml_text",
            "structured",
            "version",
            "content_hash",
            "is_active",
            "network_rules",
            "filesystem_rules",
            "process_rules",
            "landlock",
            "revisions",
            "template_id",
            "template_key",
            "created_at",
            "updated_at",
        ]
        read_only_fields = ["version", "content_hash", "structured", "updated_at"]

    _nested_keys = ("network_rules", "filesystem_rules", "process_rules", "landlock")

    def _nested_payload(self) -> dict:
        if not hasattr(self, "initial_data"):
            return {}
        return {key: self.initial_data[key] for key in self._nested_keys if key in self.initial_data}

    def _wants_yaml(self) -> bool:
        return hasattr(self, "initial_data") and "yaml_text" in self.initial_data

    def _template(self, validated_data):  # noqa: ANN001
        saved = validated_data.pop("template_id", None)
        key = validated_data.pop("template_key", None)
        if saved is not None:
            return saved
        if key:
            return key
        return None

    @staticmethod
    def _raise_service_error(exc: services.PolicyValidationError):
        raise DrfValidationError({"yaml_text": exc.result.get("errors", [str(exc)])}) from exc

    @transaction.atomic
    def create(self, validated_data):  # noqa: ANN001
        nested = self._nested_payload()
        yaml_text = validated_data.pop("yaml_text", None)
        source = validated_data.pop("yaml_source", None)
        if source is None:
            source = "raw" if yaml_text is not None else "structured"
        if nested and source == "raw":
            raise DrfValidationError(
                {"yaml_source": ["A raw policy takes YAML, not rule rows. Send yaml_text or choose structured."]}
            )
        name = validated_data.pop("name")
        template = self._template(validated_data)
        try:
            if template is not None:
                policy = services.create_policy_from_template(
                    template,
                    name=name,
                    scope=validated_data.get("scope", "sandbox"),
                    sandbox=validated_data.get("sandbox"),
                    user=self.context.get("request").user if self.context.get("request") else None,
                    source=source,
                    is_active=validated_data.get("is_active", True),
                )
                if nested:
                    services.apply_structured_payload(policy, nested)
                    policy = services.save_policy(
                        policy,
                        user=self.context.get("request").user if self.context.get("request") else None,
                        source="structured",
                    )
                return policy
            return services.create_policy(
                name=name,
                scope=validated_data.get("scope", "sandbox"),
                sandbox=validated_data.get("sandbox"),
                user=self.context.get("request").user if self.context.get("request") else None,
                yaml_text=yaml_text,
                source=source,
                payload=nested,
                is_active=validated_data.get("is_active", True),
            )
        except services.PolicyValidationError as exc:
            self._raise_service_error(exc)

    @transaction.atomic
    def update(self, instance, validated_data):  # noqa: ANN001
        nested = self._nested_payload()
        yaml_present = self._wants_yaml()
        yaml_text = validated_data.pop("yaml_text", None)
        source = validated_data.pop("yaml_source", None)
        template = self._template(validated_data)

        requested_source = source or ("raw" if yaml_present else instance.yaml_source)
        if nested and requested_source == "raw":
            raise DrfValidationError(
                {"yaml_source": ["A raw policy takes YAML, not rule rows. Send yaml_text or choose structured."]}
            )
        if yaml_present and requested_source == "structured":
            raise DrfValidationError(
                {
                    "yaml_text": [
                        "This policy is structured. Change yaml_source to 'raw' before editing the "
                        "document, or send rule rows instead."
                    ]
                }
            )

        previous_yaml = instance.yaml_text
        previous_version = instance.version

        for key, value in validated_data.items():
            setattr(instance, key, value)
        instance.save()

        try:
            if template is not None:
                document = services.template_document(template)
                if nested:
                    raise DrfValidationError(
                        {"template_id": ["Use either a template or explicit rule rows, not both."]}
                    )
                services.sync_rows_from_document(instance, document)
                return services.save_policy(
                    instance,
                    user=self.context.get("request").user if self.context.get("request") else None,
                    yaml_text=instance.yaml_text,
                    source="structured",
                    previous_yaml=previous_yaml,
                    previous_version=previous_version,
                )

            if nested:
                services.apply_structured_payload(instance, nested)
                return services.save_policy(
                    instance,
                    user=self.context.get("request").user if self.context.get("request") else None,
                    source="structured",
                    previous_yaml=previous_yaml,
                    previous_version=previous_version,
                )

            return services.save_policy(
                instance,
                user=self.context.get("request").user if self.context.get("request") else None,
                yaml_text=yaml_text if yaml_present else None,
                source=source,
                previous_yaml=previous_yaml,
                previous_version=previous_version,
            )
        except services.PolicyValidationError as exc:
            self._raise_service_error(exc)


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

    def _policy(self, instance):  # noqa: ANN001
        return instance.policy

    def _ensure_structured(self, policy):  # noqa: ANN001
        if policy.yaml_source != "structured":
            raise DrfValidationError(
                {"policy": ["This policy is in raw mode; edit its YAML document or switch it to structured."]}
            )

    def _publish(self, policy):  # noqa: ANN001
        self._ensure_structured(policy)
        services.save_policy(
            policy,
            user=self.request.user,
            note="rule changed through the API",
            source="structured",
        )

    @transaction.atomic
    def perform_create(self, serializer):  # noqa: ANN001
        policy = serializer.validated_data.get("policy") or self._policy_from_query()
        if policy is None:
            raise DrfValidationError({"policy": ["This field is required."]})
        self._ensure_structured(policy)
        serializer.save(policy=policy)
        self._publish(policy)

    @transaction.atomic
    def perform_update(self, serializer):  # noqa: ANN001
        instance = serializer.instance
        policy = self._policy(instance)
        self._ensure_structured(policy)
        serializer.save()
        self._publish(policy)

    @transaction.atomic
    def perform_destroy(self, instance):  # noqa: ANN001
        policy = self._policy(instance)
        self._ensure_structured(policy)
        super().perform_destroy(instance)
        self._publish(policy)

    def _policy_from_query(self):  # noqa: ANN201
        policy_id = self.request.query_params.get(self.policy_param)
        return Policy.objects.filter(pk=policy_id).first() if policy_id else None


class PolicyViewSet(viewsets.ModelViewSet):
    queryset = (
        Policy.objects.select_related("sandbox")
        .prefetch_related("revisions", "network_rules", "filesystem_rules", "process_rules")
    )
    serializer_class = PolicySerializer
    permission_classes = [OperatorOnly]

    def get_queryset(self):  # noqa: ANN201
        qs = super().get_queryset()
        for key in ("scope", "sandbox", "is_active"):
            value = self.request.query_params.get(key)
            if value not in (None, ""):
                qs = qs.filter(**{key: value})
        return qs

    @extend_schema(
        tags=["policies"],
        summary="Validate policy YAML against the OpenShell schema",
        request=None,
        responses={200: PolicyValidationResult},
    )
    @action(detail=True, methods=["post"])
    def validate(self, request, pk=None):  # noqa: ANN001, ARG002
        policy = self.get_object()
        return Response(services.validate_policy_yaml(policy.yaml_text))

    @extend_schema(tags=["policies"], summary="Diff this policy against the gateway's effective policy")
    @action(detail=True, methods=["get"])
    def diff(self, request, pk=None):  # noqa: ANN001, ARG002
        policy = self.get_object()
        version = request.query_params.get("version")
        result = services.diff_against_effective(policy)
        if version:
            revision = policy.revisions.filter(version=version).first()
            if revision is None:
                raise NotFound(f"Revision {version!r} does not exist.", code="policy.revision_not_found")
            result["revision_diff"] = services.diff_texts(
                revision.yaml_text,
                policy.yaml_text,
                fromfile=f"revision {version}",
                tofile=f"current v{policy.version}",
            )
        return Response(result)

    @extend_schema(tags=["policies"], summary="Push this policy to the gateway", request=None)
    @action(detail=True, methods=["post"])
    def apply(self, request, pk=None):  # noqa: ANN001, ARG002
        policy = self.get_object()
        return Response(services.apply_policy_to_gateway(policy, user=request.user))

    @extend_schema(
        tags=["policies"],
        summary="Roll back to a previous revision (the document is restored exactly)",
        request=None,
    )
    @action(detail=True, methods=["post"])
    def rollback(self, request, pk=None):  # noqa: ANN001, ARG002
        policy = self.get_object()
        version = request.data.get("version")
        revision = policy.revisions.filter(version=version).first()
        if revision is None:
            raise NotFound(f"Revision {version!r} does not exist.", code="policy.revision_not_found")
        with transaction.atomic():
            restored = services.save_policy(
                policy,
                user=request.user,
                note=f"rollback to v{version}",
                yaml_text=revision.yaml_text,
                source="raw",
            )
            services.audit(
                request.user,
                "policy.rollback",
                "policy",
                restored.pk,
                f"rolled back {restored.name} to v{version}",
            )
        return Response(PolicySerializer(restored).data)

    @extend_schema(tags=["policies"], summary="List shared built-in policy presets")
    @action(detail=False, methods=["get"])
    def templates(self, request):  # noqa: ANN001, ARG002
        saved = {template.name: template for template in PolicyTemplate.objects.all()}
        items = []
        for preset in presets.presets():
            row = saved.get(preset["name"])
            items.append(
                {
                    "id": row.pk if row else None,
                    "key": preset["key"],
                    "name": preset["name"],
                    "description": preset["description"],
                    "document": preset["document"],
                    "is_builtin": True,
                }
            )
        for name, row in saved.items():
            if row.is_builtin:
                continue
            items.append(
                {
                    "id": row.pk,
                    "key": "",
                    "name": row.name,
                    "description": row.description,
                    "document": row.document,
                    "is_builtin": False,
                }
            )
        return Response(items)


class NetworkRuleViewSet(_PolicyChildViewSet):
    queryset = NetworkRule.objects.all()
    serializer_class = NetworkRuleSerializer


class FilesystemRuleViewSet(_PolicyChildViewSet):
    queryset = FilesystemRule.objects.all()
    serializer_class = FilesystemRuleSerializer


class ProcessRuleViewSet(_PolicyChildViewSet):
    queryset = ProcessRule.objects.all()
    serializer_class = ProcessRuleSerializer


class LandlockRuleViewSet(_PolicyChildViewSet):
    queryset = LandlockRule.objects.all()
    serializer_class = LandlockRuleSerializer


class PolicyTemplateViewSet(viewsets.ModelViewSet):
    queryset = PolicyTemplate.objects.all()
    serializer_class = PolicyTemplateSerializer
    permission_classes = [OperatorOnly]

    def perform_create(self, serializer):  # noqa: ANN001
        serializer.save()

    @extend_schema(tags=["policies"], summary="Install or refresh the built-in presets")
    @action(detail=False, methods=["post"])
    def install_builtins(self, request):  # noqa: ANN001, ARG002
        count = services.ensure_builtin_templates()
        return Response({"installed": count})


router = DefaultRouter()
router.register("policies", PolicyViewSet, basename="policy")
router.register("policy-templates", PolicyTemplateViewSet, basename="policy-template")
router.register("network-rules", NetworkRuleViewSet, basename="network-rule")
router.register("filesystem-rules", FilesystemRuleViewSet, basename="filesystem-rule")
router.register("process-rules", ProcessRuleViewSet, basename="process-rule")
router.register("landlock-rules", LandlockRuleViewSet, basename="landlock-rule")

urlpatterns = router.urls
