"""Locale discovery: dropping a catalog adds a language, no code change."""
from __future__ import annotations

from pathlib import Path

from apps.identity.services import language_catalog
from django.test import override_settings

CATALOG = '''msgid ""
msgstr ""
"Language: de\\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\\n"

msgid "Dashboard"
msgstr "Übersicht"

msgid "Containers"
msgstr "Container"
'''


def test_catalog_lists_english_and_discovers_new_locale(tmp_path: Path):  # noqa: ANN201
    po = tmp_path / "de" / "LC_MESSAGES"
    po.mkdir(parents=True)
    (po / "nestbox.po").write_text(CATALOG, encoding="utf-8")

    with override_settings(LOCALE_PATHS=[tmp_path]):
        catalog = language_catalog()

    codes = [row["code"] for row in catalog]
    assert codes[0] == "en"
    assert "de" in codes
    german = next(row for row in catalog if row["code"] == "de")
    assert german["completion"] == 100
    assert german["experimental"] is False
