"""One readable log out of the several a sandbox produces.

An OpenShell sandbox has three voices: the agent's own output (the workload container),
the supervisor that mediates it, and the gateway that manages both. Each logs in its own
format. This module puts them in one list ordered by time, each line tagged with who said
it, so "why did that connection fail" and "what did the agent print" read as one story.

Pure functions, so the ordering and the parsing are tested without a gateway or Docker.
"""
from __future__ import annotations

import re
from datetime import datetime, timezone

# `[1793267740.277] [sandbox] [OCSF ] [ocsf] NET:OPEN ...`  (OpenShell)
OPENSHELL = re.compile(r"^\[(?P<ts>\d+\.\d+)\]\s*\[(?P<source>[A-Za-z]+)\s*\]\s*(?P<rest>.*)$")
# `2026-10-29T13:55:01.123456789Z message`  (docker logs --timestamps)
DOCKER = re.compile(r"^(?P<ts>\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z)\s?(?P<rest>.*)$")


def _iso(epoch: float) -> str:
    return datetime.fromtimestamp(epoch, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.") + f"{int((epoch % 1) * 1000):03d}Z"


def parse_openshell(line: str) -> tuple[float, str, str] | None:
    match = OPENSHELL.match(line.strip())
    if not match:
        return None
    return float(match.group("ts")), match.group("source").lower(), match.group("rest")


def parse_docker(line: str) -> tuple[float, str, str] | None:
    match = DOCKER.match(line.strip())
    if not match:
        return None
    stamp = match.group("ts")
    head, _, fraction = stamp.rstrip("Z").partition(".")
    base = datetime.strptime(head, "%Y-%m-%dT%H:%M:%S").replace(tzinfo=timezone.utc).timestamp()
    seconds = base + (float("0." + fraction[:6]) if fraction else 0.0)
    return seconds, "agent", match.group("rest")


def merge(agent: str, openshell: str, tail: int) -> str:
    """Both logs as one: `<iso time> [<source>] <message>`, oldest first, at most `tail` lines."""
    rows: list[tuple[float, str, str]] = []
    for line in agent.splitlines():
        parsed = parse_docker(line)
        if parsed:
            rows.append(parsed)
        elif line.strip() and rows:
            # A continuation of the previous agent line (a stack trace, a wrapped message).
            last = rows[-1]
            rows[-1] = (last[0], last[1], last[2] + " " + line.strip())
    for line in openshell.splitlines():
        parsed = parse_openshell(line)
        if parsed:
            rows.append(parsed)
    rows.sort(key=lambda row: row[0])
    return "\n".join(f"{_iso(ts)} [{source}] {text}" for ts, source, text in rows[-tail:])
