"""Role-based permissions.

Roles map 1:1 onto OpenShell workspaces. Enforcement happens here, in one place, so the gateway
only ever sees the controller's operator identity.
"""
from __future__ import annotations

from rest_framework.permissions import BasePermission

_ALL = {"admin", "operator", "viewer"}


def role_of(user) -> str:  # noqa: ANN001
    if not getattr(user, "is_authenticated", False):
        return "anonymous"
    if user.is_superuser:
        return "admin"
    names = set(user.groups.values_list("name", flat=True))
    for candidate in ("admin", "operator", "viewer"):
        if candidate in names:
            return candidate
    return "viewer"


class RolePermission(BasePermission):
    allowed: set[str] = _ALL

    def has_permission(self, request, view):  # noqa: ANN001
        return role_of(request.user) in self.allowed


class ViewerOnly(RolePermission):
    allowed = _ALL


class OperatorOnly(RolePermission):
    allowed = {"admin", "operator"}


class AdminOnly(RolePermission):
    allowed = {"admin"}
