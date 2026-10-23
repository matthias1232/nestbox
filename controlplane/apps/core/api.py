"""API surface of the Core endpoints module.

Each module owns its serializers, viewsets and routes; ``apps/api/urls.py`` composes them and only
includes the modules that are active.
"""
from __future__ import annotations

from django.conf import settings
from django.contrib.auth import authenticate
from django.urls import path
from drf_spectacular.utils import OpenApiResponse, extend_schema
from rest_framework import serializers
from rest_framework.authtoken.models import Token
from rest_framework.decorators import api_view, permission_classes
from rest_framework.permissions import AllowAny
from rest_framework.response import Response
from rest_framework.views import APIView

from apps.api.errors import NestboxError
from apps.core.language import language_catalog
from apps.core.runtime import runtime


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




class MeSerializer(serializers.Serializer):
    email = serializers.EmailField()
    username = serializers.CharField()
    role = serializers.CharField()
    is_staff = serializers.BooleanField()


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
    @extend_schema(tags=["identity"], summary="Current user and role", responses=MeSerializer)
    def get(self, request):  # noqa: ANN001
        roles = set(request.user.groups.values_list("name", flat=True))
        role = next((name for name in ("admin", "operator", "viewer") if name in roles), "")
        return Response(
            MeSerializer(
                {
                    "email": request.user.email,
                    "username": request.user.username,
                    "role": role,
                    "is_staff": request.user.is_staff,
                }
            ).data
        )


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


urlpatterns = [
    path("system/", SystemView.as_view(), name="system"),
    path("me/", MeView.as_view(), name="me"),
    path("identity/tokens/", TokenCreateView.as_view(), name="token-create"),
    path("identity/languages/", LanguageListView.as_view(), name="languages"),
]
