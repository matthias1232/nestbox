"""API surface of the Network module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from drf_spectacular.utils import OpenApiResponse, extend_schema
from rest_framework import serializers, viewsets
from rest_framework.decorators import action
from rest_framework.response import Response
from rest_framework.routers import DefaultRouter

from apps.api.permissions import AdminOnly, OperatorOnly
from apps.core.models import OverlayPeer
from apps.identity.services import audit
from apps.network.models import Certificate, DnsRecord, NetworkProfile, ProxyRoute


class CertificateSerializer(serializers.ModelSerializer):
    days_left = serializers.SerializerMethodField()

    class Meta:
        model = Certificate
        fields = [
            "id", "fqdn", "issuer", "source", "status", "expires_at", "days_left",
            "cert_pem", "chain_pem", "key_pem_encrypted", "last_error",
            "created_at", "updated_at",
        ]
        read_only_fields = ["created_at", "updated_at", "status"]
        extra_kwargs = {"key_pem_encrypted": {"write_only": True}}

    def get_days_left(self, obj: Certificate) -> int | None:
        from apps.tls.services import days_left

        return days_left(obj)


class NetworkProfileSerializer(serializers.ModelSerializer):
    class Meta:
        model = NetworkProfile
        fields = "__all__"
        read_only_fields = ["created_at", "updated_at", "last_applied_at", "last_good_yaml"]


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


class CertificateViewSet(viewsets.ModelViewSet):
    queryset = Certificate.objects.all()
    serializer_class = CertificateSerializer
    permission_classes = [OperatorOnly]

    @extend_schema(
        tags=["network"],
        summary="Issue or renew a certificate",
        responses={200: OpenApiResponse(description="Renewal result with a stable error code on failure.")},
    )
    @action(detail=True, methods=["post"])
    def renew(self, request, pk=None):  # noqa: ANN001
        from apps.tls.services import refresh_status, request_letsencrypt

        cert = self.get_object()
        refresh_status(cert)
        if cert.source == "letsencrypt":
            return Response(
                request_letsencrypt(
                    cert,
                    email=request.data.get("email", ""),
                    challenge=request.data.get("challenge", "http01"),
                )
            )
        from apps.tls.services import issue_self_signed, store_certificate

        pair = issue_self_signed(cert.fqdn)
        store_certificate(cert.fqdn, source="internal_ca", cert_pem=pair.cert_pem,
                          key_pem=pair.key_pem, issuer="Nestbox internal CA")
        return Response({"ok": True, "certificate": cert.fqdn, "source": "internal_ca"})


class NetworkProfileViewSet(viewsets.ModelViewSet):
    queryset = NetworkProfile.objects.all()
    serializer_class = NetworkProfileSerializer
    permission_classes = [AdminOnly]

    @extend_schema(
        tags=["network"],
        summary="Plan a network change: diff, critical fields and risks",
        responses={200: OpenApiResponse(description="Changes, risk text and the rendered netplan.")},
    )
    @action(detail=True, methods=["get"])
    def plan(self, request, pk=None):  # noqa: ANN001
        from apps.netconfig.services import plan_change, render_diff

        profile = self.get_object()
        return Response({**plan_change(profile).as_dict(), "diff": render_diff(profile)})

    @extend_schema(
        tags=["network"],
        summary="Apply with a rollback watchdog",
        responses={200: OpenApiResponse(description="Applied or refused, with the reason.")},
    )
    @action(detail=True, methods=["post"])
    def apply(self, request, pk=None):  # noqa: ANN001
        from apps.netconfig.services import apply as apply_profile

        result = apply_profile(
            self.get_object(),
            confirm_hostname=request.data.get("confirm_hostname", ""),
            dry_run=bool(request.data.get("dry_run", False)),
        )
        audit(request.user, "other", "network_profile", pk, result["code"])
        return Response(result)


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


router = DefaultRouter()
router.register("certificates", CertificateViewSet, basename="certificate")
router.register("dns-records", DnsRecordViewSet, basename="dns-record")
router.register("proxy-routes", ProxyRouteViewSet, basename="proxy-route")
router.register("peers", OverlayPeerViewSet, basename="peer")

urlpatterns = router.urls
