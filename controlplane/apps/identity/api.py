"""API surface of the Identity module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from django.contrib.auth.models import Group, User
from rest_framework import serializers, viewsets
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly
from apps.identity.models import AuditEvent, Profile


class ProfileSerializer(serializers.ModelSerializer):
    email = serializers.CharField(source="user.email", read_only=True)
    username = serializers.CharField(source="user.username", read_only=True)
    role = serializers.CharField(read_only=True)

    class Meta:
        model = Profile
        fields = ["id", "email", "username", "role", "created_at"]


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


class AuditEventViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = AuditEvent.objects.select_related("actor")
    serializer_class = AuditEventSerializer
    permission_classes = [AdminOnly]
    filterset_fields = ["action", "target_type"]


class UserViewSet(viewsets.ReadOnlyModelViewSet):
    queryset = Profile.objects.select_related("user").prefetch_related("user__groups")
    serializer_class = ProfileSerializer
    permission_classes = [AdminOnly]


router = DefaultRouter()
router.register("audit", AuditEventViewSet, basename="audit-event")
router.register("users", UserViewSet, basename="user")

urlpatterns = router.urls
