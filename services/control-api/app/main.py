"""API-first phase-1 control plane for the Nestbox reference environment."""
from __future__ import annotations

import json
import os
from datetime import UTC, datetime
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen
from uuid import uuid4

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel, Field

app = FastAPI(title="Nestbox Control API", version="1.0.0")
STATE_PATH = Path(os.getenv("NESTBOX_STATE_PATH", "/data/state.json"))
RUNNER_URL = os.getenv("NESTBOX_RUNNER_URL", "http://sandbox-runner:9000")
DEFAULT = {"sandboxes": {}, "tool_profiles": {}, "bindings": {}, "deployments": {}, "providers": {}, "events": []}

PACKS = {
    "ubuntu-dev": {"name": "Ubuntu 24.04 development", "image": "nestbox-ubuntu-dev:local", "command": ["sleep", "infinity"], "tool_profile": "development-default", "description": "Empty Ubuntu 24.04 development sandbox."},
    "openclaw-gateway": {"name": "OpenClaw Gateway", "image": "nestbox-openclaw:local", "command": ["openclaw", "gateway", "run", "--dev", "--allow-unconfigured", "--bind", "lan", "--port", "8080", "--auth", "none"], "tool_profile": "development-default", "description": "Dedicated OpenClaw service sandbox."},
    "hermes-agent": {"name": "Hermes Agent", "image": "nestbox-hermes:local", "command": ["sleep", "infinity"], "tool_profile": "development-default", "description": "Official Nous Hermes Agent installation on Ubuntu 24.04."},
}
DEFAULT_TOOLS = ["git", "nano", "curl", "ca-certificates", "jq", "less", "procps", "iputils-ping", "dnsutils", "ripgrep", "vim"]


def now() -> str:
    return datetime.now(UTC).isoformat()


def load_state() -> dict:
    try:
        loaded = json.loads(STATE_PATH.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        loaded = {}
    result = {key: loaded.get(key, value.copy() if isinstance(value, dict) else value[:]) for key, value in DEFAULT.items()}
    result["tool_profiles"].setdefault("development-default", {"name": "development-default", "base": "ubuntu:24.04", "packages": DEFAULT_TOOLS, "description": "Standard developer tools for Ubuntu 24.04."})
    return result


state = load_state()


def save_state() -> None:
    STATE_PATH.parent.mkdir(parents=True, exist_ok=True)
    temporary = STATE_PATH.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, indent=2, sort_keys=True))
    temporary.replace(STATE_PATH)


def audit(kind: str, subject: str, detail: dict | None = None) -> dict:
    event = {"id": str(uuid4()), "time": now(), "kind": kind, "subject": subject, "detail": detail or {}}
    state["events"].append(event)
    save_state()
    return event


def runner(method: str, path: str, payload: dict | None = None) -> dict | list:
    request = Request(RUNNER_URL + path, data=json.dumps(payload).encode() if payload else None, method=method, headers={"Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=45) as response:
            return json.loads(response.read())
    except HTTPError as exc:
        detail = exc.read().decode(errors="replace")
        raise HTTPException(exc.code if exc.code < 500 else 502, detail) from exc
    except URLError as exc:
        raise HTTPException(502, f"sandbox runtime unavailable: {exc}") from exc


class SandboxCreate(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9-]{1,63}$")
    pack_key: str = "ubuntu-dev"
    tool_profile: str | None = None
    memory_limit: str = "1g"
    cpu_limit: float = Field(default=1.0, gt=0, le=8)
    pids_limit: int = Field(default=256, ge=32, le=4096)
    environment: dict[str, str] = Field(default_factory=dict)


class Provider(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9-]{1,63}$")
    kind: str = "openai-compatible"
    base_url: str
    model: str
    api_key: str = Field(default="", exclude=True)



class Action(BaseModel):
    action: str = Field(pattern=r"^(start|stop|restart|remove)$")


class ToolProfile(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9-]{1,63}$")
    packages: list[str] = Field(min_length=1)
    description: str = ""


class Binding(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9-]{1,63}$")
    source: str
    target: str
    protocol: str = "http"
    port: int = Field(default=8080, ge=1, le=65535)
    purpose: str = "service communication"


def sync_one(record: dict) -> dict:
    try:
        runtime = runner("GET", f"/v1/sandboxes/{record['name']}")
        record.update({"runtime": runtime, "status": runtime["status"], "updated_at": now()})
    except HTTPException as exc:
        record.update({"status": "unknown", "runtime_error": str(exc.detail), "updated_at": now()})
    return record


@app.get("/healthz")
def healthz() -> dict:
    return {"status": "ok", "mode": "phase-1-reference", "runtime_adapter": "docker-development", "packs": list(PACKS)}


@app.get("/v1/runtime")
def runtime() -> dict:
    try:
        runtime_health = runner("GET", "/healthz")
        items = runner("GET", "/v1/sandboxes")
    except HTTPException as exc:
        return {"status": "unavailable", "adapter": "docker-development", "detail": str(exc.detail), "sandboxes": []}
    return {"status": "ready", "adapter": runtime_health["adapter"], "network": runtime_health["network"], "sandboxes": items, "openshell": {"status": "not-configured", "note": "Official OpenShell adapter is a future runtime adapter; this phase-1 VM currently uses the visible Docker development adapter."}}


@app.post("/v1/providers", status_code=201)
def create_provider(body: Provider) -> dict:
    if body.name in state["providers"]: raise HTTPException(409, "provider name already exists")
    value = body.model_dump() | {"id": str(uuid4()), "created_at": now(), "api_key_configured": bool(body.api_key)}
    state["providers"][body.name] = value
    audit("provider.created", value["id"], {"name": body.name, "kind": body.kind, "base_url": body.base_url, "model": body.model})
    return {key: data for key, data in value.items() if key != "api_key"}
@app.get("/v1/providers")
def providers() -> list[dict]:
    return [{key: value for key, value in item.items() if key != "api_key"} for item in state["providers"].values()]

@app.get("/v1/packs")
def packs() -> list[dict]:
    return [{"key": key, **value} for key, value in PACKS.items()]


@app.get("/v1/tool-profiles")
def tool_profiles() -> list[dict]:
    return list(state["tool_profiles"].values())


@app.post("/v1/tool-profiles", status_code=201)
def create_tool_profile(body: ToolProfile) -> dict:
    if body.name in state["tool_profiles"]:
        raise HTTPException(409, "tool profile name already exists")
    value = body.model_dump() | {"base": "ubuntu:24.04", "created_at": now()}
    state["tool_profiles"][body.name] = value
    audit("tool_profile.created", body.name, {"packages": body.packages})
    return value


@app.get("/v1/tool-catalog")
def tool_catalog(query: str = "") -> list[dict]:
    catalog = [{"name": name, "source": "apt", "ubuntu": "24.04"} for name in DEFAULT_TOOLS]
    needle = query.lower()
    return [item for item in catalog if not needle or needle in item["name"]]


@app.get("/v1/sandboxes")
def list_sandboxes() -> list[dict]:
    return [sync_one(value) for value in state["sandboxes"].values()]


@app.get("/v1/sandboxes/{sandbox_id}")
def sandbox(sandbox_id: str) -> dict:
    value = state["sandboxes"].get(sandbox_id)
    if not value:
        raise HTTPException(404, "sandbox not found")
    return sync_one(value)


@app.post("/v1/sandboxes", status_code=201)
def create_sandbox(body: SandboxCreate) -> dict:
    if body.pack_key not in PACKS:
        raise HTTPException(422, "unknown agent pack")
    if any(item["name"] == body.name for item in state["sandboxes"].values()):
        raise HTTPException(409, "sandbox name already exists")
    pack = PACKS[body.pack_key]
    profile = body.tool_profile or pack["tool_profile"]
    if profile not in state["tool_profiles"]:
        raise HTTPException(422, "unknown tool profile")
    runtime = runner("POST", "/v1/sandboxes", {"name": body.name, "image": pack["image"], "command": pack["command"], "runtime": "docker-development", "pack_key": body.pack_key, "tool_profile": profile, "environment": body.environment, "memory_limit": body.memory_limit, "cpu_limit": body.cpu_limit, "pids_limit": body.pids_limit})
    ident = runtime["id"]
    value = {"id": ident, "name": body.name, "pack_key": body.pack_key, "tool_profile": profile, "configuration": {"memory_limit": body.memory_limit, "cpu_limit": body.cpu_limit, "pids_limit": body.pids_limit, "environment_slots": sorted(body.environment)}, "created_at": now(), "status": runtime["status"], "runtime": runtime}
    state["sandboxes"][ident] = value
    audit("sandbox.created", ident, {"name": body.name, "pack_key": body.pack_key, "tool_profile": profile})
    return value


@app.post("/v1/sandboxes/{sandbox_id}/lifecycle")
def lifecycle(sandbox_id: str, body: Action) -> dict:
    value = state["sandboxes"].get(sandbox_id)
    if not value:
        raise HTTPException(404, "sandbox not found")
    result = runner("POST", f"/v1/sandboxes/{value['name']}/{body.action}")
    if body.action == "remove":
        value["status"] = "deleted"
    else:
        value.update({"status": result["status"], "runtime": result})
    audit(f"sandbox.{body.action}", sandbox_id, {"name": value["name"]})
    return value


@app.get("/v1/sandboxes/{sandbox_id}/logs")
def sandbox_logs(sandbox_id: str, tail: int = 200) -> dict:
    value = state["sandboxes"].get(sandbox_id)
    if not value:
        raise HTTPException(404, "sandbox not found")
    return runner("GET", f"/v1/sandboxes/{value['name']}/logs?tail={tail}")


@app.get("/v1/bindings")
def bindings() -> list[dict]:
    return list(state["bindings"].values())


@app.post("/v1/bindings", status_code=201)
def create_binding(body: Binding) -> dict:
    sandbox_names = {item["name"] for item in state["sandboxes"].values()}
    if body.source not in sandbox_names or body.target not in sandbox_names:
        raise HTTPException(422, "binding source and target must be existing sandboxes")
    if body.name in state["bindings"]:
        raise HTTPException(409, "binding name already exists")
    value = body.model_dump() | {"id": str(uuid4()), "created_at": now(), "url": f"{body.protocol}://{body.target}:{body.port}"}
    state["bindings"][body.name] = value
    audit("binding.created", value["id"], {"source": body.source, "target": body.target})
    return value


@app.get("/v1/topology")
def topology() -> dict:
    sandboxes = list_sandboxes()
    return {"nodes": [{"id": "runtime", "label": "Docker development runtime", "kind": "runtime", "status": "ready"}] + [{"id": item["name"], "label": item["name"], "kind": item["pack_key"], "status": item["status"], "ip": (item.get("runtime", {}).get("network") or [{}])[0].get("ip", "")} for item in sandboxes], "edges": [{"source": "runtime", "target": item["name"], "label": "runs"} for item in sandboxes] + [{"source": item["source"], "target": item["target"], "label": f"{item['protocol']}:{item['port']} · {item['purpose']}"} for item in state["bindings"].values()]}


@app.post("/v1/reference/deploy", status_code=202)
def deploy_reference() -> dict:
    deployment = {"id": str(uuid4()), "status": "deploying", "created_at": now(), "steps": []}
    state["deployments"][deployment["id"]] = deployment
    specs = [("dev-01", "ubuntu-dev", {}), ("gateway-01", "openclaw-gateway", {}), ("hermes-01", "hermes-agent", {})]
    for name, pack, env in specs:
        existing = next((item for item in state["sandboxes"].values() if item["name"] == name), None)
        if existing:
            deployment["steps"].append({"name": name, "status": "existing"})
            continue
        try:
            result = create_sandbox(SandboxCreate(name=name, pack_key=pack, environment=env))
            deployment["steps"].append({"name": name, "status": result["status"], "id": result["id"]})
        except HTTPException as exc:
            deployment["status"] = "failed"
            deployment["error"] = str(exc.detail)
            audit("reference.deployment.failed", deployment["id"], deployment)
            return deployment
    if "hermes-to-gateway" not in state["bindings"]:
        create_binding(Binding(name="hermes-to-gateway", source="hermes-01", target="gateway-01", purpose="Hermes service integration"))
    deployment["status"] = "running"
    deployment["completed_at"] = now()
    audit("reference.deployment.running", deployment["id"], {"sandboxes": [name for name, _, _ in specs]})
    return deployment


@app.get("/v1/deployments")
def deployments() -> list[dict]:
    return list(state["deployments"].values())


@app.get("/v1/events")
def events() -> list[dict]:
    return state["events"][-500:]
