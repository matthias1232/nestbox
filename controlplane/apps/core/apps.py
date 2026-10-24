from django.apps import AppConfig


class CoreConfig(AppConfig):
    default_auto_field = "django.db.models.BigAutoField"
    name = "apps.core"
    verbose_name = "Core"

    def ready(self) -> None:
        # The project needs one admin: Unfold with allauth login and the staff/role/MFA
        # gate. Installing it here keeps the URLconf and INSTALLED_APPS untouched.
        from apps.core.admin_site import install_admin_site

        install_admin_site()
