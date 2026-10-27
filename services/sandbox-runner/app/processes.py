"""The process table of one sandbox.

`docker top` is the honest answer to "what is running in there": it is the same
list `ps` inside the container would show, without running a command through the
agent's shell. The column set is fixed; `ps` arguments come from this file and
never from the request, because `ps` accepts a format string and that is enough
to read a file the caller should not see.
"""
from __future__ import annotations

import time
from typing import Any

import docker
from docker.errors import APIError, NotFound
from fastapi import APIRouter, HTTPException

router = APIRouter()
client = docker.from_env()

# Fixed on purpose: a caller cannot add a column, an `-o` flag, or a sort key.
PS_ARGS = "-eo pid,ppid,user,%cpu,%mem,etime,args"
SAMPLE_LIMIT = 500


def container(name: str) -> Any:
    try:
        return client.containers.get(f"nestbox-sandbox-{name}")
    except NotFound as exc:
        raise HTTPException(404, f"no such sandbox: {name}") from exc


def _to_number(value: str) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


@router.get("/v1/sandboxes/{name}/processes")
def processes(name: str) -> dict[str, Any]:
    """Rows of `docker top`, capped and typed, with the raw header kept.

    A stopped container has no process table; that is a 409 with a reason, not an
    empty list, so the console can say "the sandbox is stopped" instead of
    drawing a table with no rows and leaving the operator to guess.
    """
    item = container(name)
    try:
        table = item.top(ps_args=PS_ARGS)
    except APIError as exc:
        raise HTTPException(409, f"the sandbox has no process table: {exc}") from exc

    titles = table.get("Titles") or []
    rows = table.get("Processes") or []
    index = {title.lower(): position for position, title in enumerate(titles)}

    def field(row: list[str], *names: str) -> str | None:
        for candidate in names:
            position = index.get(candidate)
            if position is not None and position < len(row):
                return row[position]
        return None

    entries = []
    for row in rows[:SAMPLE_LIMIT]:
        pid = int(_to_number(field(row, "pid") or "") or 0)
        entries.append(
            {
                "pid": pid,
                "ppid": int(field(row, "ppid") or 0) or None,
                "user": field(row, "user") or "",
                "cpu_pct": _to_number(field(row, "%cpu") or ""),
                "mem_pct": _to_number(field(row, "%mem") or ""),
                # The titles depend on the ps build: procps prints ELAPSED and
                # COMMAND, a busybox ps prints ETIME and CMD. Accept both rather
                # than reporting an empty column as "no command".
                "elapsed": field(row, "etime", "elapsed"),
                "command": field(row, "args", "cmd", "command") or "",
            }
        )
    return {
        "sandbox": name,
        "taken_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "columns": titles,
        "items": entries,
        "truncated": len(rows) > SAMPLE_LIMIT,
    }
