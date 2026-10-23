"""API v1: composed from the active modules.

Every module owns its router in ``apps.<module>.api``. This file only decides which of them are
mounted, so switching a module off removes its endpoints from the API, the schema and the docs.
"""
from __future__ import annotations

import importlib
from collections.abc import Iterable

from config.modules import CATALOG
from django.conf import settings


def build_api_urls(active_keys: Iterable[str]) -> list:
    urls: list = []
    seen: set[str] = set()
    for key in active_keys:
        spec = CATALOG.get(key)
        if spec is None or spec.app in seen:
            continue
        seen.add(spec.app)
        try:
            module = importlib.import_module(f"{spec.app}.api")
        except ModuleNotFoundError as exc:  # a module without an API is fine
            if exc.name not in {f"{spec.app}.api", spec.app}:
                raise
            continue
        urls.extend(getattr(module, "urlpatterns", []))
    return urls


urlpatterns = build_api_urls(settings.NESTBOX_ACTIVE_MODULES)
