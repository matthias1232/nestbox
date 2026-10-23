"""Root URL configuration.

The appliance has exactly one user interface: the Unfold admin under ``/admin/``. ``/`` redirects
there, ``/accounts/`` carries the allauth identity flows and ``/api/`` is the versioned API with
Swagger and Redoc. Everything else belongs to a module and is composed from the catalog.
"""
from apps.core.api import api_root
from django.conf import settings
from django.conf.urls.static import static
from django.contrib import admin
from django.urls import include, path
from django.views.generic import RedirectView
from drf_spectacular.views import (
    SpectacularAPIView,
    SpectacularRedocView,
    SpectacularSwaggerView,
)

urlpatterns = [
    path("", RedirectView.as_view(pattern_name="admin:index", permanent=False), name="home"),
    path("admin/", admin.site.urls),
    path("accounts/", include("allauth.urls")),
    path("i18n/", include("django.conf.urls.i18n")),
    path("api/v1/", include("apps.api.urls")),
    path("api/schema/", SpectacularAPIView.as_view(), name="schema"),
    path("api/schema/v1/", SpectacularAPIView.as_view(), name="schema-v1"),
    path("api/schema/vNext/", SpectacularAPIView.as_view(), name="schema-vnext"),
    path("api/docs/", SpectacularSwaggerView.as_view(url_name="schema"), name="swagger-ui"),
    path("api/redoc/", SpectacularRedocView.as_view(url_name="schema"), name="redoc"),
    path("api/", api_root, name="api-root"),
]

if settings.DEBUG:
    urlpatterns += static(settings.MEDIA_URL, document_root=settings.MEDIA_ROOT)
