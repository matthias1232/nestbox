"""Users, roles, profiles and the read-only audit trail.

The auth app registers its own admin classes during autodiscovery. They are replaced here so
the single admin surface is consistently Unfold and so operators can see the role groups that
:class:`apps.identity.models.Profile` derives its role from.
"""
from __future__ import annotations

from django.contrib import admin
from django.contrib.auth.admin import GroupAdmin as BaseGroupAdmin
from django.contrib.auth.admin import UserAdmin as BaseUserAdmin
from django.contrib.auth.models import Group, User
from unfold.admin import ModelAdmin, StackedInline

from apps.identity.models import AuditEvent, Profile


class ProfileInline(StackedInline):
    model = Profile
    extra = 0
    max_num = 1
    fields = ["language", "timezone"]


@admin.register(Profile)
class ProfileAdmin(ModelAdmin):
    list_display = ["user", "language", "timezone", "created_at"]
    search_fields = ["user__email", "user__username"]
    autocomplete_fields = ["user"]
    readonly_fields = ["created_at"]


@admin.register(AuditEvent)
class AuditEventAdmin(ModelAdmin):
    list_display = ["created_at", "action", "actor", "target_type", "target_id", "summary"]
    list_filter = ["action", "target_type"]
    search_fields = ["summary", "target_id", "actor__username"]
    readonly_fields = [field.name for field in AuditEvent._meta.fields]
    date_hierarchy = "created_at"
    list_filter_submit = True

    def has_add_permission(self, request) -> bool:  # noqa: ANN001, ARG002
        return False

    def has_change_permission(self, request, obj=None) -> bool:  # noqa: ANN001, ARG002
        return False

    def has_delete_permission(self, request, obj=None) -> bool:  # noqa: ANN001, ARG002
        return False


# --- auth app replacements -------------------------------------------------------------------
admin.site.unregister(User)
admin.site.unregister(Group)


@admin.register(User)
class UserAdmin(ModelAdmin, BaseUserAdmin):
    inlines = [ProfileInline]
    list_display = ["username", "email", "first_name", "last_name", "role_label", "is_staff",
                    "is_active"]
    list_filter = ["is_staff", "is_superuser", "is_active", "groups"]
    search_fields = ["username", "first_name", "last_name", "email"]
    list_filter_submit = True

    @admin.display(description="Role")
    def role_label(self, obj: User) -> str:
        try:
            return obj.profile.role
        except Profile.DoesNotExist:
            return "admin" if obj.is_superuser else "—"


@admin.register(Group)
class GroupAdmin(ModelAdmin, BaseGroupAdmin):
    list_display = ["name", "user_count"]
    search_fields = ["name"]

    @admin.display(description="Users")
    def user_count(self, obj: Group) -> int:
        return obj.user_set.count()
