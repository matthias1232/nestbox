"""Root URL configuration."""
from apps.api import views as api_views
from apps.builds import views as build_views
from apps.core import views as core_views
from apps.policies import views as policy_views
from apps.sandboxes import views as sandbox_views
from django.conf import settings
from django.conf.urls.static import static
from django.contrib import admin
from django.urls import include, path
from drf_spectacular.views import (
    SpectacularAPIView,
    SpectacularRedocView,
    SpectacularSwaggerView,
)

urlpatterns = [
    path("admin/", admin.site.urls),
    path("accounts/", include("allauth.urls")),
    path("i18n/", include("django.conf.urls.i18n")),
    # UI (server-rendered)
    path("", core_views.dashboard, name="dashboard"),
    path("containers/", core_views.container_list, name="containers"),
    path("containers/<str:name>/", core_views.container_detail, name="container-detail"),
    path("sandboxes/", sandbox_views.sandbox_list, name="sandboxes"),
    path("sandboxes/new/", sandbox_views.sandbox_create, name="sandbox-create"),
    path("sandboxes/<str:name>/", sandbox_views.sandbox_detail, name="sandbox-detail"),
    path("policies/", policy_views.policy_list, name="policies"),
    path("policies/<int:pk>/", policy_views.policy_detail, name="policy-detail"),
    path("builds/", build_views.build_list, name="builds"),
    path("builds/<int:pk>/", build_views.build_detail, name="build-detail"),
    # Streaming
    path("stream/containers/", core_views.container_stream, name="stream-containers"),
    path("stream/sandboxes/", sandbox_views.sandbox_stream, name="stream-sandboxes"),
    # API
    path("api/v1/", include("apps.api.urls")),
    path("api/schema/", SpectacularAPIView.as_view(), name="schema"),
    path("api/schema/v1/", SpectacularAPIView.as_view(), name="schema-v1"),
    path("api/schema/vNext/", SpectacularAPIView.as_view(), name="schema-vnext"),
    path(
        "api/docs/",
        SpectacularSwaggerView.as_view(url_name="schema"),
        name="swagger-ui",
    ),
    path(
        "api/redoc/",
        SpectacularRedocView.as_view(url_name="schema"),
        name="redoc",
    ),
    path("api/", api_views.api_root, name="api-root"),
]

if settings.DEBUG:
    urlpatterns += static(settings.MEDIA_URL, document_root=settings.MEDIA_ROOT)
