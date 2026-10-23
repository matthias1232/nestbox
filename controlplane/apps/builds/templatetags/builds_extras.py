"""Template helpers for artefact dictionaries whose keys are file names."""
from __future__ import annotations

from django import template

register = template.Library()


@register.filter
def dict_get(mapping, key):
    """Look a value up by a key that a template cannot express, such as 'user-data'."""
    try:
        return mapping.get(key, "")
    except AttributeError:
        return ""
