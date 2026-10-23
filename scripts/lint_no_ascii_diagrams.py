#!/usr/bin/env python3
"""Fail when a tracked text file contains a box-drawing diagram.

The project rule is that diagrams are rendered (Mermaid -> SVG) or generated with the image tool.
Monospace "drawings" break on mobile, in dark mode, and for screen readers.
"""
from __future__ import annotations

from pathlib import Path

BOX = set("─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋")
SKIP_DIRS = {".git", ".venv", "node_modules", "staticfiles", "migrations", "__pycache__"}
SUFFIXES = {".md", ".py", ".html", ".txt", ".yaml", ".yml"}


def offenders() -> list[tuple[Path, int]]:
    root = Path(__file__).resolve().parents[1]
    found: list[tuple[Path, int]] = []
    for path in root.rglob("*"):
        if not path.is_file() or path.suffix not in SUFFIXES:
            continue
        if any(part in SKIP_DIRS for part in path.parts):
            continue
        if path.name == "lint_no_ascii_diagrams.py":
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        for lineno, line in enumerate(text.splitlines(), start=1):
            if sum(ch in BOX for ch in line) >= 3:
                found.append((path.relative_to(root), lineno))
    return found


def main() -> int:
    hits = offenders()
    for path, lineno in hits:
        print(f"{path}:{lineno}: box-drawing diagram detected — render it instead")
    if hits:
        print(f"\n{len(hits)} offending line(s). See docs/style-guide.md.")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
