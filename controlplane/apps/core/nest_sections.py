"""Typed, module-contributed desired configuration for a nest.

Inactive sections are retained on existing records, but cannot be edited. Settings
are desired state, not proof that a gateway has applied them.
"""
from dataclasses import dataclass
from importlib import import_module

from django import forms
from django.conf import settings
from django.core.exceptions import ValidationError

from config.modules import CATALOG


@dataclass(frozen=True)
class NestSection:
    key: str
    label: str
    help_text: str
    fields: dict


def sections():
    result = []
    for key in settings.NESTBOX_ACTIVE_MODULES:
        app = CATALOG[key].app
        try:
            module = import_module(f"{app}.nest_config")
        except ModuleNotFoundError as exc:
            if exc.name != f"{app}.nest_config":
                raise
            continue
        result.extend(module.SECTIONS)
    return result


def validate_spec(value, previous=None):
    if not isinstance(value, dict):
        raise ValidationError("Nest configuration must be an object.")
    previous = previous or {}
    active = {section.key: section for section in sections()}
    cleaned = {}
    for key, data in value.items():
        if key not in active:
            if key not in previous or data != previous[key]:
                raise ValidationError(f"Section {key!r} is unavailable or disabled.")
            cleaned[key] = data
            continue
        if not isinstance(data, dict):
            raise ValidationError(f"Section {key!r} must be an object.")
        section = active[key]
        unknown = set(data) - set(section.fields)
        if unknown:
            raise ValidationError(f"Unknown fields in {key}: {', '.join(sorted(unknown))}")
        form_type = type(f"{key.title()}ConfigForm", (forms.Form,), section.fields.copy())
        form = form_type(data=data)
        if not form.is_valid():
            raise ValidationError(f"{section.label}: {form.errors.as_text()}")
        cleaned[key] = form.cleaned_data
    return cleaned
