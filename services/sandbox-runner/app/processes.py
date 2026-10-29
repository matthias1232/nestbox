"""The process table of one sandbox.

`docker top` is the honest answer to "what is running in there": it is the same
list `ps` inside the container would show, without running a command through the
agent's shell. The column set is fixed; `ps` arguments come from this file and
never from the request, because `ps` accepts a format string and that is enough
to read a file the caller should not see.
"""
from __future__ import annotations
from app.workload import get_container, list_workloads, sandbox_name

import time
from typing import Any

import docker
from docker.errors import APIError, NotFound
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

router = APIRouter()
client = docker.from_env()

# Fixed on purpose: a caller cannot add a column, an `-o` flag, or a sort key.
PS_ARGS = "-eo pid,ppid,user,%cpu,%mem,etime,args"
SAMPLE_LIMIT = 500
ALLOWED_ACTIONS = {"terminate": "TERM", "kill": "KILL", "pause": "STOP", "resume": "CONT"}


class ProcessAction(BaseModel):
    action: str
    nice: int | None = None


def container(name: str) -> Any:
    try:
        return get_container(client, name)
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


@router.post("/v1/sandboxes/{name}/processes/{pid}/action")
def process_action(name: str, pid: int, request: ProcessAction) -> dict[str, Any]:
    """Apply one fixed signal or a lower CPU priority to a live child process.

    Docker's exec API is used with an argument vector, never a request-supplied
    shell command. PID 1 is deliberately protected: it is the sandbox workload,
    so its lifecycle belongs to the sandbox controls instead.
    """
    if pid < 2:
        raise HTTPException(422, "PID 1 is the sandbox entrypoint; use the sandbox controls instead")
    item = container(name)
    live = processes(name)["items"]
    if not any(row["pid"] == pid for row in live):
        raise HTTPException(404, f"no live process {pid} in sandbox {name}")
    if request.action == "set_nice":
        if request.nice is None or not -20 <= request.nice <= 19:
            raise HTTPException(422, "set_nice requires a nice value between -20 and 19")
        command = ["renice", "-n", str(request.nice), "-p", str(pid)]
        message = f"Priority of PID {pid} set to nice {request.nice}."
    else:
        signal = ALLOWED_ACTIONS.get(request.action)
        if not signal or request.nice is not None:
            raise HTTPException(422, "action must be terminate, kill, pause, resume, or set_nice")
        command = ["kill", f"-{signal}", str(pid)]
        message = f"Sent SIG{signal} to PID {pid}."
    result = item.exec_run(command, stdout=True, stderr=True)
    if result.exit_code != 0:
        raise HTTPException(409, (result.output or b"process action failed").decode(errors="replace")[:512])
    return {"pid": pid, "action": request.action, "message": message}
