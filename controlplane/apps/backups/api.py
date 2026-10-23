"""API surface of the Backups module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from rest_framework import serializers, viewsets
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly
from apps.backups.models import BackupSnapshot, BackupTarget, RestoreJob


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


router = DefaultRouter()
router.register("backups/targets", BackupTargetViewSet, basename="backup-target")
router.register("backups/snapshots", BackupSnapshotViewSet, basename="backup-snapshot")
router.register("backups/restores", RestoreJobViewSet, basename="restore-job")

urlpatterns = router.urls
