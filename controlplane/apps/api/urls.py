"""API v1 routes."""
from django.urls import path
from rest_framework.routers import DefaultRouter

from apps.api import views

router = DefaultRouter()
router.register("nodes", views.NodeViewSet, basename="node")
router.register("containers", views.ContainerViewSet, basename="container")
router.register("services", views.ServiceViewSet, basename="service")
router.register("templates", views.SandboxTemplateViewSet, basename="template")
router.register("sandboxes", views.SandboxViewSet, basename="sandbox")
router.register("policies", views.PolicyViewSet, basename="policy")
router.register("network-rules", views.NetworkRuleViewSet, basename="network-rule")
router.register("filesystem-rules", views.FilesystemRuleViewSet, basename="filesystem-rule")
router.register("process-rules", views.ProcessRuleViewSet, basename="process-rule")
router.register("agent-packs", views.AgentPackViewSet, basename="agent-pack")
router.register("gateways", views.GatewayViewSet, basename="gateway")
router.register("workspaces", views.WorkspaceMappingViewSet, basename="workspace")
router.register("certificates", views.CertificateViewSet, basename="certificate")
router.register("dns-records", views.DnsRecordViewSet, basename="dns-record")
router.register("proxy-routes", views.ProxyRouteViewSet, basename="proxy-route")
router.register("peers", views.OverlayPeerViewSet, basename="peer")
router.register("builds/recipes", views.BuildRecipeViewSet, basename="build-recipe")
router.register("builds/jobs", views.BuildJobViewSet, basename="build-job")
router.register("backups/targets", views.BackupTargetViewSet, basename="backup-target")
router.register("backups/snapshots", views.BackupSnapshotViewSet, basename="backup-snapshot")
router.register("backups/restores", views.RestoreJobViewSet, basename="restore-job")
router.register("audit", views.AuditEventViewSet, basename="audit")
router.register("users", views.UserViewSet, basename="user")

urlpatterns = [
    path("system/", views.SystemView.as_view(), name="system"),
    path("me/", views.MeView.as_view(), name="me"),
    path("identity/tokens/", views.TokenCreateView.as_view(), name="token-create"),
    path("identity/languages/", views.LanguageListView.as_view(), name="languages"),
    *router.urls,
]
