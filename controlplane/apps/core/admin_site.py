"""The single admin site and its allauth-backed access rules.

Nestbox has one user interface. Authentication lives in allauth (``/accounts/``), so the
admin site does not render a second login form: anonymous requests are redirected to the
allauth login with the original admin URL preserved as ``next``.

Access is deliberately stricter than Django's default. A user needs an active staff account,
an operator-or-admin role and, when they have enrolled a second factor, an MFA-verified
session. This keeps the appliance's single admin surface behind the same identity rules as
the rest of the system.
"""
from __future__ import annotations

from urllib.parse import urlencode

from django.contrib import messages
from django.contrib.auth import logout as auth_logout
from django.http import HttpRequest, HttpResponse, HttpResponseForbidden, HttpResponseRedirect
from django.urls import reverse
from django.utils.http import url_has_allowed_host_and_scheme
from unfold.sites import UnfoldAdminSite

#: Roles from :class:`apps.identity.models.Profile` that may use the admin.
ADMIN_ROLES = ("admin", "operator")

#: allauth records every authentication step in this session key. A password step alone is
#: not enough once the account has a second factor.
AUTHENTICATION_METHODS_SESSION_KEY = "account_authentication_methods"
MFA_SESSION_KEY = "nestbox_mfa_verified"


def role_for_user(user) -> str | None:  # noqa: ANN001
    """Return the effective admin role without creating a Profile row."""
    if not getattr(user, "is_authenticated", False):
        return None
    if getattr(user, "is_superuser", False):
        return "admin"
    names = set(user.groups.values_list("name", flat=True))
    for role in ("admin", "operator", "viewer"):
        if role in names:
            return role
    return None


def mfa_is_enabled(user) -> bool:  # noqa: ANN001
    """Whether the account has an MFA authenticator enrolled."""
    try:
        from allauth.mfa.utils import is_mfa_enabled
    except ImportError:  # pragma: no cover - allauth mfa is a project dependency
        return False
    try:
        return bool(is_mfa_enabled(user))
    except Exception:  # noqa: BLE001 - a missing table must not open or crash the admin
        return False


def mfa_satisfied(request: HttpRequest, user) -> bool:  # noqa: ANN001
    """A user with a second factor must have completed it in this session."""
    if not mfa_is_enabled(user):
        return True
    if request.session.get(MFA_SESSION_KEY) is True:
        return True
    methods = request.session.get(AUTHENTICATION_METHODS_SESSION_KEY, [])
    return any(
        isinstance(entry, dict) and entry.get("method") == "mfa" for entry in methods
    )


def can_access_admin(request: HttpRequest, user=None) -> bool:  # noqa: ANN001
    """The complete admin gate: active, staff, role and MFA."""
    user = user or request.user
    if not getattr(user, "is_authenticated", False):
        return False
    if not getattr(user, "is_active", False) or not getattr(user, "is_staff", False):
        return False
    if role_for_user(user) not in ADMIN_ROLES:
        return False
    return mfa_satisfied(request, user)


def admin_login_url(request: HttpRequest) -> str:
    """Build the allauth login URL while preserving the requested admin path."""
    login_url = reverse("account_login")
    next_url = request.get_full_path()
    if not url_has_allowed_host_and_scheme(
        next_url,
        allowed_hosts={request.get_host()},
        require_https=request.is_secure(),
    ):
        next_url = reverse("admin:index")
    return f"{login_url}?{urlencode({'next': next_url})}"


class NestboxAdminSite(UnfoldAdminSite):
    """Unfold admin with allauth login and staff/role/MFA enforcement."""

    def has_permission(self, request: HttpRequest) -> bool:
        return can_access_admin(request)

    def login(self, request: HttpRequest, extra_context=None) -> HttpResponse:  # noqa: ANN001, ARG002
        if request.user.is_authenticated:
            if role_for_user(request.user) not in ADMIN_ROLES:
                return HttpResponseForbidden(
                    "Your account is signed in but has no operator role for this appliance. "
                    "Ask an administrator to grant the operator or admin role."
                )
            if not mfa_satisfied(request, request.user):
                # A forced/legacy session can be signed in without completing the second
                # factor. End it and start the normal allauth flow, which includes MFA.
                auth_logout(request)
                messages.warning(
                    request,
                    "Your session did not complete two-factor authentication. Please sign in again.",
                )
                return HttpResponseRedirect(admin_login_url(request))
        return HttpResponseRedirect(admin_login_url(request))

    def each_context(self, request: HttpRequest) -> dict:
        context = super().each_context(request)
        context["nestbox_admin_role"] = role_for_user(request.user)
        return context


def install_admin_site() -> NestboxAdminSite | None:
    """Replace the lazily created default site after admin autodiscovery.

    Django builds ``admin.site`` from ``AdminConfig.default_site`` before a project app's
    ``ready()`` can run. Replacing the wrapped instance keeps every existing
    ``@admin.register`` call and gives the URLconf the custom site without changing the
    root URL module or ``INSTALLED_APPS``.
    """
    from django.contrib import admin
    from django.contrib.admin.sites import DefaultAdminSite

    site_proxy = admin.site
    if not isinstance(site_proxy, DefaultAdminSite):
        # A project that installed its own site already owns the decision.
        return site_proxy if isinstance(site_proxy, NestboxAdminSite) else None

    site_proxy._setup()  # noqa: SLF001 - required to reach the already-populated registry
    current = site_proxy._wrapped  # noqa: SLF001
    if isinstance(current, NestboxAdminSite):
        return current

    site = NestboxAdminSite(name="admin")
    site._registry = current._registry  # noqa: SLF001 - same model -> ModelAdmin mapping
    for model_admin in site._registry.values():
        model_admin.admin_site = site
    site_proxy._wrapped = site  # noqa: SLF001 - the documented lazy proxy slot
    return site
