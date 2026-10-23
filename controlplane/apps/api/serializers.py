"""Request/response models for the public API."""
from __future__ import annotations

from django.contrib.auth.models import Group, User
from rest_framework import serializers

from apps.agentpacks.models import AgentPack
from apps.backups.models import BackupSnapshot, BackupTarget, RestoreJob
from apps.builds.models import BuildJob, BuildRecipe
from apps.core.models import Container, Node, OverlayPeer, Service
from apps.identity.models import AuditEvent, Profile
from apps.network.models import Certificate, DnsRecord, ProxyRoute
from apps.openshell.models import Gateway, WorkspaceMapping
from apps.policies.models import (
    FilesystemRule,
    NetworkRule,
    Policy,
    PolicyRevision,
    ProcessRule,
)
from apps.sandboxes.models import Sandbox, SandboxTemplate


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


class AgentPackSerializer(serializers.ModelSerializer):
    class Meta:
        model = AgentPack
        fields = "__all__"
        read_only_fields = ["updated_at"]


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


class CertificateSerializer(serializers.ModelSerializer):
    class Meta:
        model = Certificate
        fields = ["id", "fqdn", "issuer", "status", "expires_at", "cert_pem", "key_pem_encrypted", "created_at"]
        read_only_fields = ["created_at", "updated_at"]
        extra_kwargs = {"key_pem_encrypted": {"write_only": True}}


class DnsRecordSerializer(serializers.ModelSerializer):
    class Meta:
        model = DnsRecord
        fields = "__all__"
        read_only_fields = ["created_at"]


class ProxyRouteSerializer(serializers.ModelSerializer):
    class Meta:
        model = ProxyRoute
        fields = "__all__"
        read_only_fields = ["created_at"]


class OverlayPeerSerializer(serializers.ModelSerializer):
    class Meta:
        model = OverlayPeer
        fields = "__all__"
        read_only_fields = ["created_at"]


class BuildRecipeSerializer(serializers.ModelSerializer):
    class Meta:
        model = BuildRecipe
        fields = "__all__"
        read_only_fields = ["created_at", "updated_at"]


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


class BackupTargetSerializer(serializers.ModelSerializer):
    class Meta:
        model = BackupTarget
        fields = "__all__"
        read_only_fields = ["created_at"]


class BackupSnapshotSerializer(serializers.ModelSerializer):
    target_name = serializers.CharField(source="target.name", read_only=True)

    class Meta:
        model = BackupSnapshot
        fields = "__all__"
        read_only_fields = ["created_at"]


class RestoreJobSerializer(serializers.ModelSerializer):
    class Meta:
        model = RestoreJob
        fields = "__all__"
        read_only_fields = ["state", "log", "created_at"]


class AuditEventSerializer(serializers.ModelSerializer):
    actor_email = serializers.CharField(source="actor.email", read_only=True, default="")

    class Meta:
        model = AuditEvent
        fields = [
            "id",
            "actor",
            "actor_email",
            "action",
            "target_type",
            "target_id",
            "summary",
            "diff",
            "request_id",
            "created_at",
        ]
        read_only_fields = fields


class ProfileSerializer(serializers.ModelSerializer):
    email = serializers.CharField(source="user.email", read_only=True)
    username = serializers.CharField(source="user.username", read_only=True)
    role = serializers.CharField(read_only=True)

    class Meta:
        model = Profile
        fields = ["email", "username", "language", "timezone", "role"]
        read_only_fields = ["role"]


class GroupSerializer(serializers.ModelSerializer):
    class Meta:
        model = Group
        fields = ["id", "name"]


class UserSerializer(serializers.ModelSerializer):
    role = serializers.SerializerMethodField()

    class Meta:
        model = User
        fields = ["id", "username", "email", "is_active", "is_superuser", "role", "groups"]
        read_only_fields = ["is_superuser"]

    def get_role(self, obj: User) -> str:
        try:
            return obj.profile.role
        except Profile.DoesNotExist:
            return "viewer"


class TokenRequest(serializers.Serializer):
    email = serializers.EmailField()
    password = serializers.CharField(style={"input_type": "password"})


class TokenResponse(serializers.Serializer):
    token = serializers.CharField()
    expires = serializers.CharField(required=False)


class LanguageSerializer(serializers.Serializer):
    code = serializers.CharField()
    name = serializers.CharField()
    endonym = serializers.CharField()
    direction = serializers.CharField()
    completion = serializers.IntegerField()
    experimental = serializers.BooleanField()
