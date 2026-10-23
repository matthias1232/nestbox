"""Audit trail and language discovery."""
from __future__ import annotations

import re
from pathlib import Path

from django.conf import settings

from apps.identity.models import AuditEvent

_PO_MSGID = re.compile(r'^msgid\s+"(.*)"')
_PO_MSGSTR = re.compile(r'^msgstr\s+"(.*)"')


def audit(user, action: str, target_type: str = "", target_id: str | int = "", summary: str = "", diff=None):  # noqa: ANN001
    return AuditEvent.objects.create(
        actor=user if getattr(user, "is_authenticated", False) else None,
        action=action,
        target_type=target_type,
        target_id=str(target_id),
        summary=summary[:300],
        diff=diff or {},
    )


def _po_stats(path: Path) -> tuple[int, int, str | None]:
    """Return (translated, total, language_name) for a gettext catalog."""
    translated = total = 0
    language: str | None = None
    current_is_msgstr = False
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw.strip()
        if line.startswith('"Language:'):
            language = line.split(":", 1)[1].strip().strip('"')
            continue
        if m := _PO_MSGID.match(line):
            if m.group(1):
                total += 1
            current_is_msgstr = False
        elif m := _PO_MSGSTR.match(line):
            if m.group(1) and total:
                translated += 1
            current_is_msgstr = True
        elif line == "" and current_is_msgstr:
            current_is_msgstr = False
    return translated, total, language


def language_catalog() -> list[dict]:
    """Derived from the filesystem: dropping a .po file adds a language.

    English is the source language and is therefore always complete without a catalog.
    """
    result: list[dict] = [
        {
            "code": "en",
            "name": "English",
            "endonym": "English",
            "direction": "ltr",
            "completion": 100,
            "experimental": False,
        }
    ]
    known = {"en"}
    for po in sorted(Path(settings.LOCALE_PATHS[0]).glob("*/LC_MESSAGES/*.po")):
        code = po.parent.parent.name
        if code in known:
            continue
        translated, total, language = _po_stats(po)
        completion = round(100 * translated / total) if total else 0
        result.append(
            {
                "code": code,
                "name": (language or code).split(";")[0].strip() or code,
                "endonym": (language or code).split(";")[0].strip() or code,
                "direction": "rtl" if code in {"ar", "fa", "he", "ur"} else "ltr",
                "completion": completion,
                "experimental": completion < 80,
            }
        )
        known.add(code)
    return result
