import contextlib

from django.contrib import admin

from apps.core import models

for name, obj in vars(models).items():
    if isinstance(obj, type) and hasattr(obj, "_meta") and not obj._meta.abstract:
        with contextlib.suppress(admin.sites.AlreadyRegistered):
            admin.site.register(obj)
