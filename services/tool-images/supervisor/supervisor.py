#!/usr/bin/env python3
"""The supervisor inside a sandbox.

Started by the generated supervisor script next to the agents. It reports what
it measures, and it executes a fixed whitelist of commands left in
~/.nestbox-supervisor/commands/. A command whose type is not one of the three
below is answered with `rejected` and never run: the control plane validates
too, and this is the boundary that holds if the control plane is wrong.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

ALLOWED = {"apply_policy", "restart_agent", "run_check"}

HOME = Path.home()
# The control plane writes settings and commands here, so the path is agreed by
# both sides and passed in; `~/.nestbox-supervisor` is only the fallback for a
# hand-run copy of this script.
STATE = Path(os.getenv("NESTBOX_SUPERVISOR_STATE") or (HOME / ".nestbox-supervisor"))
COMMANDS = STATE / "commands"
RESULTS = STATE / "results"
SETTINGS = STATE / "settings.json"
APPLIED = STATE / "applied.json"

def env_float(name: str, default: float) -> float:
    """A number from an environment variable that may be exported empty.

    The generated supervisor script exports every slot the image declares, so an
    unset interval arrives as the empty string rather than being absent. `float("")`
    raises, and a supervisor that dies on its first line reports nothing at all.
    """
    raw = (os.getenv(name) or "").strip()
    if not raw:
        return default
    try:
        return float(raw)
    except ValueError:
        return default


INTERVAL = env_float("NESTBOX_SUPERVISOR_INTERVAL", 15.0)
CONTROL_URL = os.getenv("NESTBOX_CONTROL_URL", "http://control-api:8000").rstrip("/")
SUPERVISOR_ID = os.getenv("NESTBOX_SUPERVISOR_ID", "")
TOKEN = os.getenv("NESTBOX_SUPERVISOR_TOKEN", "")


def now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def read_json(path: Path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=1))


def check(key: str, state: str, detail: str) -> dict:
    return {"key": key, "state": state, "detail": detail}


def model_reachable() -> dict:
    base = os.getenv("OPENAI_BASE_URL", "")
    if not base:
        # A sandbox without an agent is not supposed to talk to a model. That is
        # a configuration the recipe chose, not a broken sandbox, so it is a
        # warning with the reason rather than a failure.
        return check("model_reachable", "warn", "no OPENAI_BASE_URL: this recipe asked for no agent")
    try:
        with urllib.request.urlopen(base.rstrip("/") + "/models", timeout=5) as answer:
            return check("model_reachable", "ok", f"{base} answered {answer.status}")
    except urllib.error.HTTPError as failure:
        return check("model_reachable", "fail", f"{base} answered {failure.code}")
    except Exception as failure:  # noqa: BLE001
        return check("model_reachable", "fail", f"{base} unreachable: {failure}")


def proxy_set() -> dict:
    upper = [k for k in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY") if os.getenv(k)]
    lower = [k for k in ("http_proxy", "https_proxy", "all_proxy") if os.getenv(k)]
    if not upper or not lower:
        return check("proxy_set", "warn", f"upper={len(upper)} lower={len(lower)}; a tool may not see the proxy")
    bypass = os.getenv("NO_PROXY", "")
    base = os.getenv("OPENAI_BASE_URL", "")
    host = base.split("://")[-1].split("/")[0].split(":")[0]
    if host and host not in bypass:
        return check("proxy_set", "warn", f"NO_PROXY does not name {host}: {bypass}")
    return check("proxy_set", "ok", f"proxy in both spellings, NO_PROXY={bypass}")


# A catalogue key is not always the binary name: `fd` installs as `fdfind` on
# Debian and Ubuntu. Checking the key alone would report a tool that is there.
BINARY_ALIASES = {"fd": ["fd", "fdfind"], "nodejs": ["node", "nodejs"]}


def wanted_tools() -> list[str]:
    """What the recipe asked for, then the settings, then the interpreter."""
    declared = read_json(SETTINGS, {}).get("tools")
    if declared:
        return list(declared)
    from_recipe = [part for part in os.getenv("NESTBOX_TOOLS", "").split(",") if part]
    return from_recipe or ["python3"]


def tools_present() -> dict:
    wanted = wanted_tools()
    missing = [
        name
        for name in wanted
        if not any(shutil.which(candidate) for candidate in BINARY_ALIASES.get(name, [name]))
    ]
    if missing:
        return check("tools_present", "fail", f"missing: {', '.join(sorted(missing))}")
    return check("tools_present", "ok", f"{len(wanted)} tools present")


def agent_running() -> dict:
    try:
        table = subprocess.run(
            ["ps", "-eo", "args", "--no-headers"], capture_output=True, text=True, timeout=5
        ).stdout
    except Exception as failure:  # noqa: BLE001
        return check("agent_running", "fail", f"ps failed: {failure}")
    if "nestbox-supervisor" not in table and "hermes" not in table:
        return check("agent_running", "warn", "no agent process found")
    return check("agent_running", "ok", "agent process is running")


def memory() -> dict:
    try:
        values = {}
        for line in Path("/proc/meminfo").read_text().splitlines():
            key, _, rest = line.partition(":")
            values[key] = int(rest.split()[0])
        free = values.get("MemAvailable", 0) // 1024
        total = values.get("MemTotal", 1) // 1024
        state = "ok" if free * 100 // total >= 15 else "warn"
        return check("memory", state, f"{free} MiB available of {total} MiB")
    except OSError as failure:
        return check("memory", "warn", f"meminfo unreadable: {failure}")


def policy_drift() -> dict:
    desired = read_json(SETTINGS, {}).get("policy_hash")
    applied = read_json(APPLIED, {}).get("policy_hash")
    if not desired:
        return check("policy_drift", "warn", "no desired policy hash delivered yet")
    if desired != applied:
        return check("policy_drift", "warn", f"desired {desired[:12]} != applied {(applied or 'none')[:12]}")
    return check("policy_drift", "ok", f"applied hash {desired[:12]}")


def last_result() -> dict | None:
    """The newest result the supervisor wrote, or None before the first one."""
    try:
        newest = max(RESULTS.glob("*.json"), key=lambda path: path.stat().st_mtime)
    except (OSError, ValueError):
        return None
    return read_json(newest, None)


def report() -> dict:
    checks = [model_reachable(), proxy_set(), tools_present(), agent_running(), memory(), policy_drift()]
    states = {c["state"] for c in checks}
    status = "failed" if "fail" in states else ("degraded" if "warn" in states else "running")
    applied = read_json(APPLIED, {})
    desired = read_json(SETTINGS, {})
    dirtiness = 0 if desired.get("policy_hash") in (None, applied.get("policy_hash")) else 1
    return {
        "status": status,
        "version": os.getenv("NESTBOX_SUPERVISOR_VERSION", "1"),
        "config_dirtiness": dirtiness,
        "last_policy_applied_at": applied.get("applied_at"),
        "checks": checks,
        "facts": {
            "host": os.uname().nodename,
            "agent": os.getenv("HERMES_MODEL", ""),
            "self_healing": bool(desired.get("self_healing", False)),
            # The last thing an operator asked for, and what came of it. Without
            # it the console can say "queued" and never learn the outcome.
            "last_action": last_result(),
        },
    }


def post(payload: dict) -> None:
    if not SUPERVISOR_ID or not TOKEN:
        print("nestbox-supervisor: no id or token in the environment", flush=True)
        return
    request = urllib.request.Request(
        f"{CONTROL_URL}/api/v1/internal/supervisors/{SUPERVISOR_ID}/report",
        data=json.dumps(payload).encode(),
        method="POST",
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {TOKEN}"},
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as answer:
            answer.read()
    except Exception as failure:  # noqa: BLE001
        print(f"nestbox-supervisor: report failed: {failure}", flush=True)


def result(command_id: str, state: str, detail: str) -> None:
    write_json(RESULTS / f"{command_id}.json", {"id": command_id, "state": state, "detail": detail, "at": now()})


def apply_policy(payload: dict) -> tuple[str, str]:
    desired_hash = str(payload.get("policy_hash") or "")
    if not desired_hash:
        return "rejected", "apply_policy needs a policy_hash"
    write_json(APPLIED, {"policy_hash": desired_hash, "applied_at": now()})
    return "done", f"applied {desired_hash[:12]}"


def restart_agent(payload: dict, settings: dict) -> tuple[str, str]:
    del payload
    if not settings.get("self_healing", False):
        return "rejected", "disabled_by_policy: self_healing is off"
    command = settings.get("restart_command")
    if not command:
        return "rejected", "not_configured: no restart_command in the settings"
    if isinstance(command, str):
        command = command.split()
    if not isinstance(command, list) or not all(isinstance(part, str) for part in command):
        return "rejected", "not_configured: restart_command is not an argument vector"
    try:
        subprocess.run(command, check=True, timeout=30)
    except Exception as failure:  # noqa: BLE001
        return "failed", f"restart_command failed: {failure}"
    return "done", "restart_command exited 0"


def handle(path: Path, settings: dict) -> None:
    try:
        command = read_json(path, None)
    except OSError:
        return
    if not isinstance(command, dict):
        path.unlink(missing_ok=True)
        return
    command_id = str(command.get("id") or path.stem)
    kind = str(command.get("type") or "")
    payload = command.get("payload") or {}
    if kind not in ALLOWED:
        result(command_id, "rejected", f"type {kind!r} is not on the whitelist")
    elif kind == "apply_policy":
        result(command_id, *apply_policy(payload))
    elif kind == "restart_agent":
        result(command_id, *restart_agent(payload, settings))
    else:
        result(command_id, "done", "check requested")
    path.unlink(missing_ok=True)


def main() -> int:
    for directory in (STATE, COMMANDS, RESULTS):
        directory.mkdir(parents=True, exist_ok=True)
    print(f"nestbox-supervisor: up as {SUPERVISOR_ID or 'unconfigured'}", flush=True)
    while True:
        settings = read_json(SETTINGS, {})
        for path in sorted(COMMANDS.glob("*.json")):
            handle(path, settings)
        post(report())
        time.sleep(INTERVAL)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        raise SystemExit(0)
